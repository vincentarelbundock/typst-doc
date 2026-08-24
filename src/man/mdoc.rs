//! The mdoc(7) reader: BSD semantic manual-page source to a [`Document`].
//!
//! mdoc is the other manual macro package, standard on the BSDs and macOS and
//! used by a good deal of Linux software besides. Where man(7) says "bold this
//! text", mdoc says *what the text is*: `.Fl` is a flag, `.Ar` an argument,
//! `.Pa` a path, `.Xr` a cross-reference. That is strictly more information
//! than the man(7) reader can recover from font changes, so this reader is the
//! shorter of the two despite covering more macros.
//!
//! It produces the same [`Document`] the man(7) pass produces, and
//! [`super::assemble`] turns either into a [`Topic`](crate::ir::Topic). Only
//! the syntax differs; section routing, name resolution, and parameter
//! extraction are shared.
//!
//! Three mdoc facts shape the parser:
//!
//! - **Macros nest.** `.Op Fl f Ar file` is an optional group containing a
//!   flag and an argument, so argument lists are parsed recursively rather
//!   than treated as text. A word is a macro only where a macro may appear.
//! - **Trailing punctuation is not part of the thing.** `.Ar file .` renders
//!   the argument, then the period outside it, so closing punctuation is
//!   split off before styling.
//! - **`.Nm` remembers.** After `.Nm ls` in the NAME section, a bare `.Nm`
//!   anywhere in the page means `ls`.

use crate::ir::{Block, Inline, LinkDest, Term};

use super::{Document, Header, RawSection};

/// Parse mdoc(7) source into the shared [`Document`] shape.
pub(super) fn parse(source: &str) -> Document {
    let mut parser = Parser::default();
    for line in source.lines() {
        parser.line(line);
    }
    parser.finish()
}

#[derive(Default)]
struct Parser {
    header: Option<Header>,
    sections: Vec<RawSection>,
    /// The section being filled: its title, its blocks, and the plain-text
    /// rendering of each line, which NAME and SYNOPSIS are read from.
    title: Option<String>,
    blocks: Vec<Block>,
    lines: Vec<String>,
    /// The paragraph being filled, if any.
    pending: Vec<Inline>,
    /// The page's own name, from the first `.Nm`, so a later bare `.Nm` can
    /// stand for it.
    name: Option<String>,
    /// Every name the NAME section lists, in order: one page may document
    /// several commands, and each is a name the page answers to.
    names: Vec<String>,
    /// `.Nd`'s one-line description, which the NAME line is built from.
    description: Option<String>,
    /// Open `.Bl` lists, innermost last.
    lists: Vec<List>,
    /// An open `.Bd` display: its lines, kept verbatim.
    display: Option<Vec<String>>,
    /// `.Sm off` suspends the space between arguments until `.Sm on`, which
    /// is how a page spells out `[user@]hostname` from separate words.
    spacing_off: bool,
}

/// An open `.Bl` list.
struct List {
    /// `-tag` and `-hang` lists pair a term with a body; the rest are plain
    /// items.
    tagged: bool,
    ordered: bool,
    items: Vec<(Vec<Inline>, Vec<Block>)>,
}

impl Parser {
    fn line(&mut self, line: &str) {
        let line = strip_comment(line);

        // A display holds its lines exactly as written, macros and all, until
        // `.Ed`: that verbatim shape is the whole point of one.
        if let Some(lines) = &mut self.display {
            if line.trim_start().starts_with(".Ed") {
                let value = std::mem::take(lines).join("\n");
                self.display = None;
                self.push_block(Block::Code {
                    lang: None,
                    value: value.trim_end().to_owned(),
                });
            } else {
                lines.push(line);
            }
            return;
        }

        let Some(rest) = line.strip_prefix('.') else {
            if line.trim().is_empty() {
                self.flush_paragraph();
            } else {
                let inlines = self.render(&split_args(&line));
                self.push_inlines(inlines);
                self.lines.push(line.trim().to_owned());
            }
            return;
        };

        let args = split_args(rest.trim_start());
        let Some((macro_name, args)) = args.split_first() else {
            return;
        };
        self.macro_line(macro_name, args);
    }

    fn macro_line(&mut self, name: &str, args: &[String]) {
        match name {
            // -- prologue ---------------------------------------------------
            "Dd" | "Os" => {}
            "Dt" => {
                self.header = Some(Header {
                    name: args.first().cloned().unwrap_or_default(),
                    section: args.get(1).cloned().unwrap_or_default(),
                });
            }

            // -- structure --------------------------------------------------
            "Sh" => {
                self.close_section();
                self.title = Some(args.join(" "));
            }
            "Ss" => {
                self.flush_paragraph();
                let content = self.render(args);
                self.push_block(Block::Heading { level: 1, content });
            }
            "Pp" | "Lp" | "Sp" => self.flush_paragraph(),
            "Sm" => {
                self.spacing_off = match args.first().map(String::as_str) {
                    Some("off") => true,
                    Some("on") => false,
                    _ => !self.spacing_off,
                };
            }
            // Keep-together and no-fill toggles change line breaking, which
            // Typst decides for itself.
            "Bk" | "Ek" => {}

            // -- the page's own name and summary ----------------------------
            //
            // A page documenting several commands writes one `.Nm` per name,
            // each but the last ending in the comma that separates them:
            // punctuation of the NAME line, not part of any name.
            "Nm" if self.in_name_section() => {
                for word in args {
                    let (name, _) = split_punctuation(word);
                    let name = name.trim();
                    if !name.is_empty() && !self.names.iter().any(|seen| seen == name) {
                        self.names.push(name.to_owned());
                    }
                }
                if self.name.is_none() {
                    self.name = self.names.first().cloned();
                }
            }
            "Nd" => {
                let text = plain(&self.render(args));
                self.description = Some(text);
            }

            // -- lists ------------------------------------------------------
            "Bl" => {
                self.flush_paragraph();
                let kind = args.iter().find(|arg| arg.starts_with('-'));
                let kind = kind.map(String::as_str).unwrap_or("-item");
                self.lists.push(List {
                    tagged: matches!(kind, "-tag" | "-hang" | "-ohang" | "-inset" | "-diag"),
                    ordered: kind == "-enum",
                    items: Vec::new(),
                });
            }
            "It" => {
                self.flush_paragraph();
                let term = self.render(args);
                match self.lists.last_mut() {
                    Some(list) => list.items.push((term, Vec::new())),
                    // An `.It` with no open list is malformed; keeping its
                    // content as a paragraph loses nothing.
                    None => {
                        let term = term.clone();
                        self.push_inlines(term);
                        self.flush_paragraph();
                    }
                }
            }
            "El" => {
                self.flush_paragraph();
                if let Some(list) = self.lists.pop() {
                    self.push_block(finish_list(list));
                }
            }

            // -- displays ---------------------------------------------------
            "Bd" => {
                self.flush_paragraph();
                self.display = Some(Vec::new());
            }
            "Dl" => {
                self.flush_paragraph();
                self.push_block(Block::Code {
                    lang: None,
                    value: plain(&self.render(args)),
                });
            }

            // -- citations --------------------------------------------------
            "Rs" | "Re" => self.flush_paragraph(),
            _ if name.starts_with('%') => {
                let inlines = self.render(args);
                self.push_inlines(inlines);
            }

            // -- anything else is inline content ----------------------------
            _ => {
                let mut all = vec![name.to_owned()];
                all.extend(args.iter().cloned());
                let inlines = self.render(&all);
                if !inlines.is_empty() {
                    let text = plain(&inlines);
                    self.push_inlines(inlines);
                    self.lines.push(text);
                }
            }
        }
    }

    /// Render a macro's arguments, expanding the macros nested among them.
    ///
    /// Arguments are separated by a space in the output, except where the next
    /// one opens with closing punctuation: `.Xr chmod 1 ,` ends in a comma
    /// that belongs against the reference, not a space away from it.
    fn render(&self, args: &[String]) -> Vec<Inline> {
        self.render_until(args, "\u{0}").0
    }

    /// Render the first macro or word of `args`, returning what is left.
    fn render_one<'a>(&self, args: &'a [String]) -> (Vec<Inline>, &'a [String]) {
        let (head, tail) = args.split_first().expect("callers check for empty");

        // An enclosure wraps everything up to its closing macro, or to the
        // end of the line where the closing macro is implied.
        if let Some((open, close)) = enclosure(head) {
            let (inner, tail) = self.render_until(tail, closing_of(head));
            let mut out = vec![Inline::text(open)];
            out.extend(inner);
            out.push(Inline::text(close));
            return (out, tail);
        }

        match head.as_str() {
            // `.Xr ls 1` is a cross-reference, and resolves like an Rd link.
            "Xr" => {
                let topic = tail.first().cloned().unwrap_or_default();
                let section = tail.get(1).cloned().unwrap_or_default();
                let consumed = usize::from(!topic.is_empty()) + usize::from(!section.is_empty());
                let mut out = vec![Inline::Link {
                    dest: LinkDest::Topic {
                        package: None,
                        topic,
                    },
                    children: Vec::new(),
                }];
                if !section.is_empty() {
                    out.push(Inline::text(format!("({section})")));
                }
                (out, &tail[consumed.min(tail.len())..])
            }
            // A bare `.Nm` stands for the page's own name.
            "Nm" if tail.is_empty() => (
                vec![Inline::Code(self.name.clone().unwrap_or_default())],
                tail,
            ),
            // The operating-system macros stand for their own names, with
            // any version the page gives appended: `.Bx 4.3` is "BSD 4.3".
            _ if os_name(head).is_some() => {
                let name = os_name(head).expect("just matched").to_owned();
                let (words, rest) = take_words(tail);
                let text = if words.is_empty() {
                    name
                } else {
                    format!("{name} {}", words.join(" "))
                };
                (vec![Inline::text(text)], rest)
            }
            // A bare `.Ar` is mdoc's shorthand for an unnamed operand list.
            "Ar" if tail.is_empty() => (vec![Inline::Emph(vec![Inline::text("file ...")])], tail),
            // `.Ex -std` and `.Rv -std` stand for a fixed sentence, which is
            // the whole reason a page writes them instead of prose.
            "Ex" | "Rv" => {
                let name = self.name.clone().unwrap_or_default();
                let sentence = if head == "Ex" {
                    format!("The {name} utility exits 0 on success, and >0 if an error occurs.")
                } else {
                    format!(
                        "The {name} function returns the value 0 if successful; otherwise \
                         the value -1 is returned and the global variable errno is set to \
                         indicate the error."
                    )
                };
                (vec![Inline::text(sentence)], &tail[tail.len()..])
            }
            // `.St -p1003.2` names a standard; the identifier is closer to
            // useful than nothing, minus the flag spelling.
            "St" => match tail.split_first() {
                Some((standard, rest)) => (
                    vec![Inline::Code(standard.trim_start_matches('-').to_owned())],
                    rest,
                ),
                None => (Vec::new(), tail),
            },
            // `.Fl f` is `-f`; a bare `.Fl` is a lone dash.
            "Fl" => match tail.split_first() {
                Some((flag, rest)) if !is_macro(flag) => {
                    let (word, punctuation) = split_punctuation(flag);
                    let mut out = vec![Inline::Code(format!("-{word}"))];
                    out.extend(punctuation);
                    (out, rest)
                }
                _ => (vec![Inline::Code("-".to_owned())], tail),
            },
            // `.Fn strlen s` is a function and its arguments.
            "Fn" => match tail.split_first() {
                Some((function, rest)) if !is_macro(function) => {
                    let (arguments, rest) = take_words(rest);
                    (
                        vec![Inline::Code(format!(
                            "{function}({})",
                            arguments.join(", ")
                        ))],
                        rest,
                    )
                }
                _ => (Vec::new(), tail),
            },
            // Macros that style the words following them, up to the next
            // macro. The style says what the words *are*.
            other => match style(other) {
                Some(style) => {
                    let (words, rest) = take_words(tail);
                    if words.is_empty() {
                        return (Vec::new(), rest);
                    }
                    let (text, punctuation) = split_punctuation(&words.join(" "));
                    let mut out = vec![style(text)];
                    out.extend(punctuation);
                    (out, rest)
                }
                // Not a macro: an ordinary word.
                None => (vec![Inline::text(unescape(head))], tail),
            },
        }
    }

    /// Render arguments up to `close`, which is consumed if found.
    fn render_until<'a>(&self, args: &'a [String], close: &str) -> (Vec<Inline>, &'a [String]) {
        let mut out: Vec<Inline> = Vec::new();
        let mut rest = args;
        // `.Ns` suppresses the space that would follow it, which is how mdoc
        // glues `Ar port` onto what came before.
        let mut joined = false;
        while let Some(head) = rest.first() {
            if head == close {
                return (out, &rest[1..]);
            }
            if head == "Ns" {
                joined = true;
                rest = &rest[1..];
                continue;
            }
            let (inlines, tail) = self.render_one(rest);
            if !inlines.is_empty() {
                if !out.is_empty()
                    && !joined
                    && !self.spacing_off
                    && !opens_with_punctuation(&inlines)
                {
                    out.push(Inline::text(" "));
                }
                out.extend(inlines);
                joined = false;
            }
            rest = tail;
        }
        (out, rest)
    }

    // -- accumulation -----------------------------------------------------

    fn in_name_section(&self) -> bool {
        self.title
            .as_deref()
            .is_some_and(|title| title.eq_ignore_ascii_case("NAME"))
    }

    fn push_inlines(&mut self, inlines: Vec<Inline>) {
        if inlines.is_empty() {
            return;
        }
        if !self.pending.is_empty() {
            self.pending.push(Inline::text(" "));
        }
        self.pending.extend(inlines);
    }

    fn flush_paragraph(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let inlines = std::mem::take(&mut self.pending);
        self.push_block(Block::Paragraph(inlines));
    }

    /// Blocks land in the innermost open list item, if there is one.
    fn push_block(&mut self, block: Block) {
        match self.lists.last_mut().and_then(|list| list.items.last_mut()) {
            Some((_, body)) => body.push(block),
            None => self.blocks.push(block),
        }
    }

    fn close_section(&mut self) {
        self.flush_paragraph();
        while let Some(list) = self.lists.pop() {
            let block = finish_list(list);
            self.push_block(block);
        }
        let Some(title) = self.title.take() else {
            // Content before the first `.Sh` is the prologue, which carries
            // nothing a topic needs.
            self.blocks.clear();
            self.lines.clear();
            return;
        };
        let mut lines = std::mem::take(&mut self.lines);
        // NAME is `.Nm ls` plus `.Nd list directory contents`, which the
        // shared assembly reads in man(7)'s one-line spelling.
        if title.eq_ignore_ascii_case("NAME") {
            lines = vec![format!(
                "{} - {}",
                self.names.join(", "),
                self.description.clone().unwrap_or_default()
            )];
        }
        self.sections.push(RawSection {
            title,
            blocks: std::mem::take(&mut self.blocks),
            lines,
        });
    }

    fn finish(mut self) -> Document {
        self.close_section();
        Document {
            header: self.header,
            sections: self.sections,
        }
    }
}

/// Close an open list into the block it describes.
fn finish_list(list: List) -> Block {
    if list.tagged {
        return Block::Terms(
            list.items
                .into_iter()
                .map(|(term, body)| Term { term, body })
                .collect(),
        );
    }
    Block::List {
        ordered: list.ordered,
        items: list
            .items
            .into_iter()
            .map(|(term, mut body)| {
                // An untagged `.It` may still carry content on its own line.
                if !term.is_empty() {
                    body.insert(0, Block::Paragraph(term));
                }
                body
            })
            .collect(),
    }
}

/// The delimiters an enclosure macro wraps its content in.
fn enclosure(name: &str) -> Option<(&'static str, &'static str)> {
    match name {
        "Op" | "Oo" => Some(("[", "]")),
        "Aq" | "Ao" => Some(("\u{27e8}", "\u{27e9}")),
        "Bq" | "Bo" => Some(("[", "]")),
        "Brq" | "Bro" => Some(("{", "}")),
        "Pq" | "Po" => Some(("(", ")")),
        "Dq" | "Do" => Some(("\u{201c}", "\u{201d}")),
        "Sq" | "So" | "Ql" => Some(("\u{2018}", "\u{2019}")),
        _ => None,
    }
}

/// The macro that closes a multi-line enclosure, if it has one.
fn closing_of(name: &str) -> &'static str {
    match name {
        "Oo" => "Oc",
        "Ao" => "Ac",
        "Bo" => "Bc",
        "Bro" => "Brc",
        "Po" => "Pc",
        "Do" => "Dc",
        "So" => "Sc",
        // A single-line enclosure has no closing macro; the line ends it.
        _ => "\u{0}",
    }
}

/// How a macro styles the words that follow it, if it is one.
///
/// mdoc names what a thing *is*, and the IR has three inline styles to say it
/// with: code for anything a reader would type or a program would see,
/// emphasis for placeholders, strong for what the page wants to stress.
fn style(name: &str) -> Option<fn(String) -> Inline> {
    let code: fn(String) -> Inline = Inline::Code;
    let emph: fn(String) -> Inline = |text| Inline::Emph(vec![Inline::text(text)]);
    let strong: fn(String) -> Inline = |text| Inline::Strong(vec![Inline::text(text)]);
    let text: fn(String) -> Inline = Inline::text;
    match name {
        // Typed, or seen in a program: code.
        "Nm" | "Cm" | "Ic" | "Li" | "Pa" | "Va" | "Dv" | "Er" | "Ev" | "Cd" | "Fd" | "In"
        | "Ft" | "Fa" | "Vt" | "Ad" | "Ms" | "Mt" | "Tn" => Some(code),
        // A placeholder the reader substitutes: emphasis.
        "Ar" | "Em" => Some(emph),
        "Sy" => Some(strong),
        // Plain words: `.No` is explicit normal text, `.An` an author's name.
        "No" | "An" => Some(text),
        _ => None,
    }
}

/// Whether a word is a macro name, and so ends a run of words.
///
/// mdoc macro names are two or three characters, capitalised or `%`-prefixed.
/// Anything else in argument position is a word.
fn is_macro(word: &str) -> bool {
    if word.starts_with('%') && word.len() == 2 {
        return true;
    }
    let mut chars = word.chars();
    let first = chars.next();
    (2..=3).contains(&word.len())
        && first.is_some_and(|c| c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_alphabetic())
        && (style(word).is_some()
            || os_name(word).is_some()
            || enclosure(word).is_some()
            || matches!(
                word,
                "Xr" | "Fl"
                    | "Fn"
                    | "Fo"
                    | "Fc"
                    | "Oc"
                    | "Ac"
                    | "Bc"
                    | "Pc"
                    | "Dc"
                    | "Sc"
                    | "Brc"
                    | "Ns"
                    | "Ap"
                    | "Pf"
                    | "Nd"
                    | "It"
                    | "El"
                    | "Bl"
                    | "Pp"
                    | "Sh"
                    | "Ss"
                    | "Ex"
                    | "Rv"
                    | "St"
            ))
}

/// Take the words up to the next macro.
fn take_words(args: &[String]) -> (Vec<String>, &[String]) {
    let end = args
        .iter()
        .position(|word| is_macro(word))
        .unwrap_or(args.len());
    (
        args[..end].iter().map(|word| unescape(word)).collect(),
        &args[end..],
    )
}

/// Whether rendered content begins with punctuation that closes what came
/// before it, and so must not be pushed away by a space.
fn opens_with_punctuation(inlines: &[Inline]) -> bool {
    matches!(inlines.first(), Some(Inline::Text(text))
        if text.starts_with([',', '.', ';', ':', ')', ']', '?', '!']))
}

/// Split trailing punctuation off a word, so it renders outside the style.
fn split_punctuation(word: &str) -> (String, Vec<Inline>) {
    let trimmed = word.trim_end_matches([',', '.', ';', ':', ')', ']', '?', '!']);
    if trimmed.len() == word.len() || trimmed.trim().is_empty() {
        return (word.to_owned(), Vec::new());
    }
    // Any space before the punctuation was an argument separator, not part of
    // the thing being styled.
    (
        trimmed.trim_end().to_owned(),
        vec![Inline::text(word[trimmed.len()..].to_owned())],
    )
}

/// Split a macro line into arguments, keeping `"quoted groups"` whole.
fn split_args(rest: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    for character in rest.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started || !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => current.push(c),
        }
    }
    if started || !current.is_empty() {
        args.push(current);
    }
    args
}

/// Cut a line at its `\"` comment.
fn strip_comment(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\\' {
            match chars.peek() {
                Some('"') => break,
                Some(next) => {
                    out.push(character);
                    out.push(*next);
                    chars.next();
                }
                None => out.push(character),
            }
            continue;
        }
        out.push(character);
    }
    out
}

/// Resolve the roff escapes mdoc pages still use for spacing and symbols.
fn unescape(word: &str) -> String {
    let mut out = String::new();
    let mut chars = word.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match chars.next() {
            // `\-` is a literal hyphen, `\&` a zero-width spacer, `\ ` a
            // non-breaking space.
            Some('-') => out.push('-'),
            Some('&') => {}
            Some(' ') => out.push(' '),
            Some('e') => out.push('\\'),
            Some('(') => {
                let name: String = chars.by_ref().take(2).collect();
                out.push_str(&super::special(&name));
            }
            Some('f') => {
                // A font change, which mdoc pages use only rarely; the
                // semantic macros carry the meaning instead.
                match chars.peek() {
                    Some('(') => {
                        chars.next();
                        chars.by_ref().take(2).for_each(drop);
                    }
                    Some('[') => {
                        for next in chars.by_ref() {
                            if next == ']' {
                                break;
                            }
                        }
                    }
                    _ => {
                        chars.next();
                    }
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// The plain text of rendered inlines, for the lines NAME and SYNOPSIS are
/// read from.
fn plain(inlines: &[Inline]) -> String {
    crate::ir::to_plain_text(inlines).trim().to_owned()
}

/// The operating system a macro names, if it names one.
fn os_name(name: &str) -> Option<&'static str> {
    match name {
        "Ux" => Some("UNIX"),
        "Bx" => Some("BSD"),
        "Nx" => Some("NetBSD"),
        "Fx" => Some("FreeBSD"),
        "Ox" => Some("OpenBSD"),
        "Dx" => Some("DragonFly"),
        "At" => Some("AT&T UNIX"),
        _ => None,
    }
}
