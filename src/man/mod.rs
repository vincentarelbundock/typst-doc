//! The man(7) reader: roff manual-page source to a [`Topic`].
//!
//! The dialect is man(7) — the `.TH`/`.SH`/`.TP` macro package every Linux
//! page is written in — plus the low-level roff escapes those pages use for
//! fonts (`\fB`), special characters (`\(bu`), and comments (`\"`). BSD's
//! semantic mdoc(7) macros are a different language and are not read here.
//!
//! Unlike Rd and Python docstrings, a man page carries no machine-readable
//! structure below the section level: `.TP` is "hanging indent", not
//! "parameter", and `.SH OPTIONS` is a convention rather than a keyword. The
//! reader therefore works in two passes. The first is purely syntactic — roff
//! lines become [`Block`]s, with `.TP` runs collapsing into [`Block::Terms`].
//! The second reads the section *titles* and routes each section to the field
//! it belongs in, turning the terms of an OPTIONS section into [`Param`]s and
//! leaving anything unrecognised as a [`Section`] in source order.
//!
//! Two roff facts shape the line loop and are easy to get wrong:
//!
//! - Font state persists across lines, so `\fB` on one line bolds the next
//!   one too, until `\fR` or `\fP`. It is a parser field, not a local.
//! - `.TP` puts the *tag* on the following line, whatever that line happens
//!   to be — text, `.B`, or `.BR`. The tag is therefore captured by a pending
//!   flag rather than read from the macro's own arguments.

use crate::ir::{Block, Example, Inline, LinkDest, Param, Section, Term, Topic, to_plain_text};

/// Why a file could not be read as a man page.
#[derive(Debug, thiserror::Error)]
pub enum ManError {
    /// Neither a `.TH` header nor a `NAME` section: whatever this file is, it
    /// does not name a manual entry.
    #[error("not a man page: no .TH header and no .SH NAME section")]
    NotAManPage,
    /// A `.so` stub: a file whose whole content redirects to another page.
    /// Common for aliases, and for every function of a library documented on
    /// one page.
    #[error("a redirect to {target}, not a page of its own")]
    Redirect { target: String },
    /// BSD's semantic macro package, which is a different language: `.Dd`,
    /// `.Sh`, `.Nm` rather than `.TH`, `.SH`, `.B`.
    #[error("an mdoc(7) page; this reader accepts man(7)")]
    Mdoc,
}

/// Parse man(7) source and convert it to a single [`Topic`].
///
/// One page is one topic, so this returns a `Topic` rather than a `Vec` the
/// way the Python and Typst readers do.
pub fn parse(source: &str) -> Result<Topic, ManError> {
    if let Some(target) = redirect(source) {
        return Err(ManError::Redirect { target });
    }
    let document = Parser::new().run(source);
    match assemble(document) {
        Err(ManError::NotAManPage) if is_mdoc(source) => Err(ManError::Mdoc),
        other => other,
    }
}

/// The target of a `.so` stub, if the file is one.
///
/// Such a file has no content of its own: `man` reads the named page in its
/// place. Resolving it needs the manual tree the file came from, which a
/// reader taking a string does not have.
fn redirect(source: &str) -> Option<String> {
    let mut target = None;
    for line in source.lines() {
        let line = strip_comment(line);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match line.strip_prefix(".so ") {
            Some(rest) if target.is_none() => target = Some(rest.trim().to_owned()),
            // Any other content means the file stands on its own.
            _ => return None,
        }
    }
    target
}

/// Whether the source is mdoc(7) rather than man(7): its own macro package,
/// with its own vocabulary.
fn is_mdoc(source: &str) -> bool {
    source
        .lines()
        .any(|line| line.starts_with(".Dd ") || line.starts_with(".Dt ") || line == ".Dd")
}

// -- pass 1: roff lines to sections of blocks -----------------------------

/// One `.SH` section, with its prose already converted.
struct RawSection {
    title: String,
    blocks: Vec<Block>,
    /// Verbatim text, kept only for SYNOPSIS: a synopsis is a code listing,
    /// and reflowing it into paragraphs would destroy the line structure that
    /// is its whole content.
    lines: Vec<String>,
}

/// The `.TH` header: `.TH name section date source manual`.
#[derive(Default)]
struct Header {
    name: String,
    section: String,
}

struct Parser {
    header: Option<Header>,
    sections: Vec<RawSection>,
    current: RawSection,
    sink: Sink,
    font: Font,
    /// Set by a bare `.B`/`.I`, which fonts the following line.
    next_font: Option<Font>,
    /// `.TP` and tagged `.IP` take their tag from the next line.
    awaiting_tag: bool,
    verbatim: Option<Verbatim>,
    /// Open `.UR`/`.MT` link: inline content routes into it until `.UE`/`.ME`.
    link: Option<(LinkDest, Vec<Inline>)>,
    /// `\c` at the end of a line suppresses the space that would otherwise
    /// join it to the next one.
    joined: bool,
    /// Whether the next line of verbatim text starts a new line rather than
    /// continuing the current one, as roff's filling would.
    break_line: bool,
}

/// A run of lines held verbatim rather than filled into paragraphs.
struct Verbatim {
    lines: Vec<String>,
    /// Which macro opened it, and so which one closes it.
    end: &'static str,
}

impl Parser {
    fn new() -> Self {
        Self {
            header: None,
            sections: Vec::new(),
            current: RawSection {
                title: String::new(),
                blocks: Vec::new(),
                lines: Vec::new(),
            },
            sink: Sink::new(),
            font: Font::Roman,
            next_font: None,
            awaiting_tag: false,
            verbatim: None,
            link: None,
            joined: false,
            break_line: true,
        }
    }

    fn run(mut self, source: &str) -> Document {
        let mut lines = source.lines();
        while let Some(line) = lines.next() {
            match Line::read(line) {
                Line::Blank => self.macro_line("PP", &[]),
                Line::Text(text) => self.text_line(&text),
                Line::Control { name, args } => {
                    // Macro and ignore definitions swallow their bodies.
                    if matches!(name.as_str(), "de" | "de1" | "am" | "ig") {
                        for line in lines.by_ref() {
                            if line.trim_end() == ".." || line.starts_with(".. ") {
                                break;
                            }
                        }
                        continue;
                    }
                    self.macro_line(&name, &args);
                }
            }
        }
        self.close_section();
        Document {
            header: self.header,
            sections: self.sections,
        }
    }

    // -- lines ------------------------------------------------------------

    fn text_line(&mut self, text: &str) {
        if let Some(verbatim) = &mut self.verbatim {
            verbatim.lines.push(plain(text));
            return;
        }
        let mut font = self.next_font.take().unwrap_or(self.font);
        let (inlines, joined) = decode(text, &mut font);
        // A font set by an escape persists; one set by a bare `.B` does not.
        if self.next_font.is_none() {
            self.font = font;
        }
        self.emit(inlines, joined);
    }

    /// Route one line's worth of inline content to wherever it belongs: a
    /// pending `.TP` tag, an open link, or the current paragraph.
    fn emit(&mut self, inlines: Vec<Inline>, joined: bool) {
        if inlines.is_empty() {
            return;
        }
        if !self.current.title.is_empty() {
            // The verbatim text follows roff's own filling: consecutive lines
            // are one line until a break, so `.B ls` and the argument line
            // under it stay the single synopsis line groff would print.
            let text = to_plain_text(&inlines);
            match self.current.lines.last_mut() {
                Some(line) if !self.break_line => {
                    if !self.joined {
                        line.push(' ');
                    }
                    line.push_str(&text);
                }
                _ => self.current.lines.push(text),
            }
            self.break_line = false;
        }
        if self.awaiting_tag {
            self.awaiting_tag = false;
            self.sink.start_term(trim_inlines(inlines));
            self.joined = false;
            return;
        }
        let target = match &mut self.link {
            Some((_, children)) => children,
            None => self.sink.para_mut(),
        };
        append(target, inlines, !target.is_empty() && !self.joined);
        self.joined = joined;
    }

    fn macro_line(&mut self, name: &str, args: &[String]) {
        // Inside `.nf`/`.EX`/`.TS`, only the closing macro and a handful of
        // spacing requests mean anything; everything else is text.
        if let Some(verbatim) = &self.verbatim {
            if name == verbatim.end {
                let Verbatim { lines, end } = self.verbatim.take().expect("checked");
                self.push_code(lines, end == "EE");
                return;
            }
            let line = match name {
                "PP" | "P" | "LP" | "sp" | "br" => Some(String::new()),
                "B" | "I" | "R" | "SM" | "SB" | "BR" | "RB" | "BI" | "IB" | "IR" | "RI" => {
                    Some(to_plain_text(&alternating(name, args, &mut Font::Roman)))
                }
                _ => None,
            };
            if let (Some(line), Some(verbatim)) = (line, self.verbatim.as_mut()) {
                verbatim.lines.push(line);
            }
            return;
        }

        // Everything except the macros that emit inline content ends the
        // current line of verbatim text.
        if !matches!(
            name,
            "B" | "I"
                | "R"
                | "SM"
                | "SB"
                | "BR"
                | "RB"
                | "BI"
                | "IB"
                | "IR"
                | "RI"
                | "UR"
                | "UE"
                | "MT"
                | "ME"
                | "MR"
                | "OP"
        ) {
            self.break_line = true;
        }

        match name {
            "TH" => {
                self.header = Some(Header {
                    name: args.first().map(|arg| plain(arg)).unwrap_or_default(),
                    section: args.get(1).map(|arg| plain(arg)).unwrap_or_default(),
                });
            }
            "SH" => {
                self.close_section();
                self.current.title = plain(&args.join(" "));
                self.font = Font::Roman;
            }
            "SS" => {
                self.sink.para_break();
                let (inlines, _) = decode(&args.join(" "), &mut Font::Roman);
                self.sink.push_block(Block::Heading {
                    level: 2,
                    content: inlines,
                });
            }
            "PP" | "P" | "LP" | "HP" | "sp" | "Pp" => self.sink.para_break(),
            "br" => {
                if !self.sink.para_mut().is_empty() {
                    self.sink.para_mut().push(Inline::LineBreak);
                    self.joined = true;
                }
            }
            "TP" | "TQ" => {
                self.sink.para_break_in_terms();
                self.awaiting_tag = true;
            }
            "IP" => match args.split_first() {
                None => self.sink.para_break(),
                Some((tag, _)) => match bullet(tag) {
                    Some(ordered) => self.sink.start_item(ordered),
                    None => {
                        let (inlines, _) = decode(tag, &mut Font::Roman);
                        self.sink.start_term(trim_inlines(inlines));
                    }
                },
            },
            "RS" => self.sink.start_indent(),
            "RE" => self.sink.end_indent(),
            "nf" => {
                self.verbatim = Some(Verbatim {
                    lines: Vec::new(),
                    end: "fi",
                })
            }
            "EX" => {
                self.verbatim = Some(Verbatim {
                    lines: Vec::new(),
                    end: "EE",
                })
            }
            "TS" => {
                self.verbatim = Some(Verbatim {
                    lines: Vec::new(),
                    end: "TE",
                })
            }
            "B" | "I" | "R" | "SM" | "SB" | "BR" | "RB" | "BI" | "IB" | "IR" | "RI" => {
                if args.is_empty() {
                    // A bare `.B` fonts the following line.
                    self.next_font = Some(match name {
                        "I" | "IR" | "IB" => Font::Italic,
                        "R" => Font::Roman,
                        _ => Font::Bold,
                    });
                    return;
                }
                let mut font = self.font;
                let inlines = alternating(name, args, &mut font);
                self.emit(inlines, false);
            }
            "UR" | "MT" => {
                let target = args.first().cloned().unwrap_or_default();
                let dest = if name == "UR" {
                    LinkDest::Url(target)
                } else {
                    LinkDest::Email(target)
                };
                self.link = Some((dest, Vec::new()));
            }
            "UE" | "ME" => {
                // A trailing argument is punctuation that follows the link
                // with no space; anything after that is ordinary prose, and
                // gets one.
                let punctuated = !args.is_empty();
                if let Some((dest, children)) = self.link.take() {
                    let link = Inline::Link {
                        dest,
                        children: trim_inlines(children),
                    };
                    self.emit(vec![link], punctuated);
                }
                if let Some(punct) = args.first() {
                    self.emit(vec![Inline::text(plain(punct))], false);
                }
            }
            // groff's cross-reference macro: `.MR ls 1 .`
            "MR" => {
                let mut inlines = vec![Inline::Link {
                    dest: LinkDest::Topic {
                        package: None,
                        topic: args.first().cloned().unwrap_or_default(),
                    },
                    children: Vec::new(),
                }];
                if let Some(section) = args.get(1) {
                    inlines.push(Inline::text(format!("({section})")));
                }
                if let Some(punct) = args.get(2) {
                    inlines.push(Inline::text(plain(punct)));
                }
                self.emit(inlines, false);
            }
            // Synopsis macros: `.SY cmd` ... `.OP -f file` ... `.YS`.
            "SY" => self.emit(
                vec![Inline::Strong(vec![Inline::text(
                    args.first().map(|arg| plain(arg)).unwrap_or_default(),
                )])],
                false,
            ),
            "OP" => {
                let text = args
                    .iter()
                    .map(|arg| plain(arg))
                    .collect::<Vec<_>>()
                    .join(" ");
                self.emit(vec![Inline::text(format!("[{text}]"))], false);
            }
            "YS" => self.sink.para_break(),
            // Formatting requests with no meaning outside a fixed-width page.
            _ => {}
        }
    }

    fn push_code(&mut self, lines: Vec<String>, example: bool) {
        let value = lines.join("\n");
        if value.trim().is_empty() {
            return;
        }
        // A synopsis written as `.nf` is still a synopsis: its lines belong to
        // the section's verbatim text as much as a filled line does.
        self.current.lines.extend(lines.iter().cloned());
        self.break_line = true;
        self.sink.para_break();
        // `.EX` marks an example; `.nf` is only "do not fill", which is also
        // how tables and diagrams are written, so it gets no language.
        let lang = example.then(|| "sh".to_owned());
        self.sink.push_block(Block::Code {
            lang,
            value: value.trim_matches('\n').to_owned(),
        });
    }

    fn close_section(&mut self) {
        let blocks = std::mem::replace(&mut self.sink, Sink::new()).finish();
        let mut section = std::mem::replace(
            &mut self.current,
            RawSection {
                title: String::new(),
                blocks: Vec::new(),
                lines: Vec::new(),
            },
        );
        section.blocks = blocks;
        if section.title.is_empty() && section.blocks.is_empty() {
            return;
        }
        self.sections.push(section);
    }
}

struct Document {
    header: Option<Header>,
    sections: Vec<RawSection>,
}

// -- roff lines -----------------------------------------------------------

enum Line {
    Blank,
    Text(String),
    Control { name: String, args: Vec<String> },
}

impl Line {
    fn read(line: &str) -> Self {
        let line = strip_comment(line);
        // `'` is the no-break control character; it introduces a request just
        // as `.` does.
        let Some(rest) = line.strip_prefix('.').or_else(|| line.strip_prefix('\'')) else {
            return if line.trim().is_empty() {
                Self::Blank
            } else {
                Self::Text(line)
            };
        };
        let rest = rest.trim_start();
        let (name, args) = match rest.find(|c: char| c.is_whitespace()) {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, ""),
        };
        Self::Control {
            name: name.to_owned(),
            args: split_args(args),
        }
    }
}

/// Cut a line at its `\"` comment, respecting escaped backslashes.
fn strip_comment(line: &str) -> String {
    let bytes: Vec<char> = line.chars().collect();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == '\\' {
            match bytes.get(index + 1) {
                Some('"') | Some('#') => return bytes[..index].iter().collect(),
                _ => index += 2,
            }
            continue;
        }
        index += 1;
    }
    line.trim_end().to_owned()
}

/// Split macro arguments, honouring roff's `"quoted argument"` form.
fn split_args(rest: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut chars = rest.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let mut arg = String::new();
        if c == '"' {
            chars.next();
            while let Some(c) = chars.next() {
                if c == '"' {
                    // `""` inside a quoted argument is a literal quote.
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        arg.push('"');
                        continue;
                    }
                    break;
                }
                arg.push(c);
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                arg.push(c);
                chars.next();
            }
        }
        args.push(arg);
    }
    args
}

/// The bullet of an `.IP` tag, if it is one: `Some(ordered)`.
fn bullet(tag: &str) -> Option<bool> {
    let plain = plain(tag);
    let tag = plain.trim();
    if matches!(tag, "\u{2022}" | "*" | "-" | "\u{2013}" | "o") {
        return Some(false);
    }
    let digits = tag.trim_end_matches(['.', ')']);
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then_some(true)
}

// -- fonts and escapes ----------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Font {
    Roman,
    Bold,
    Italic,
    BoldItalic,
}

impl Font {
    fn wrap(self, text: String) -> Inline {
        match self {
            Self::Roman => Inline::Text(text),
            Self::Bold => Inline::Strong(vec![Inline::Text(text)]),
            Self::Italic => Inline::Emph(vec![Inline::Text(text)]),
            Self::BoldItalic => Inline::Strong(vec![Inline::Emph(vec![Inline::Text(text)])]),
        }
    }

    fn of(name: &str) -> Option<Self> {
        match name {
            "B" | "3" | "CB" => Some(Self::Bold),
            "I" | "2" | "CI" => Some(Self::Italic),
            "BI" => Some(Self::BoldItalic),
            "R" | "1" | "P" | "" | "C" | "CR" => Some(Self::Roman),
            _ => None,
        }
    }
}

/// Convert one text line, tracking the font across the call so that a `\fB`
/// left open at end of line still applies to the next one.
///
/// The returned flag is `\c`: the line continues into the next with no space.
fn decode(text: &str, font: &mut Font) -> (Vec<Inline>, bool) {
    let mut out = Vec::new();
    let mut run = String::new();
    let mut joined = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '\\' {
            run.push(c);
            continue;
        }
        match chars.next() {
            None => break,
            Some('f') => {
                let name = escape_name(&mut chars);
                if let Some(next) = Font::of(&name) {
                    if !run.is_empty() {
                        out.push(font.wrap(std::mem::take(&mut run)));
                    }
                    *font = next;
                }
            }
            // Special characters, and interpolated strings, which in practice
            // name the same glyphs.
            Some('(') => {
                let name: String = chars.by_ref().take(2).collect();
                run.push_str(&special(&name));
            }
            Some('[') => {
                let mut name = String::new();
                for c in chars.by_ref() {
                    if c == ']' {
                        break;
                    }
                    name.push(c);
                }
                run.push_str(&special(&name));
            }
            Some('*') => {
                let name = escape_name(&mut chars);
                run.push_str(&special(&name));
            }
            // Sizes, motions, widths, registers: presentation with no
            // counterpart in a reflowed document.
            // Sizes, motions, widths, registers, and device control such as
            // help2man's `\X'tty: link ...'`: presentation with no
            // counterpart in a reflowed document.
            Some('s') | Some('n') | Some('h') | Some('v') | Some('l') | Some('L') | Some('w')
            | Some('x') | Some('k') | Some('b') | Some('d') | Some('u') | Some('r') | Some('$')
            | Some('X') | Some('Y') | Some('Z') | Some('D') | Some('F') | Some('H') | Some('S')
            | Some('N') | Some('M') | Some('m') | Some('g') | Some('o') | Some('p') | Some('z')
            | Some('A') | Some('B') | Some('V') => {
                skip_argument(&mut chars);
            }
            Some('c') => joined = true,
            Some('e') => run.push('\\'),
            Some('-') => run.push('-'),
            Some('\'') => run.push('\u{00B4}'),
            Some('`') => run.push('`'),
            Some('~') | Some(' ') | Some('0') => run.push(' '),
            // Zero-width and hyphenation control: no output at all.
            Some('&') | Some('%') | Some(')') | Some(':') | Some('{') | Some('}') | Some('|')
            | Some('^') | Some('/') | Some(',') | Some('\n') => {}
            Some(other) => run.push(other),
        }
    }
    if !run.is_empty() {
        out.push(font.wrap(run));
    }
    (out, joined)
}

/// The name of an escape: `x` for `\fx`, `xx` for `\f(xx`, `name` for
/// `\f[name]`. The introducing character has already been consumed.
fn escape_name(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    match chars.peek() {
        Some('(') => {
            chars.next();
            chars.by_ref().take(2).collect()
        }
        Some('[') => {
            chars.next();
            let mut name = String::new();
            for c in chars.by_ref() {
                if c == ']' {
                    break;
                }
                name.push(c);
            }
            name
        }
        Some(_) => chars.next().into_iter().collect(),
        None => String::new(),
    }
}

/// Skip an escape's argument: either a delimited `\h'...'` form or a single
/// character.
fn skip_argument(chars: &mut std::iter::Peekable<std::str::Chars>) {
    match chars.peek().copied() {
        Some('\'') => {
            chars.next();
            for c in chars.by_ref() {
                if c == '\'' {
                    break;
                }
            }
        }
        Some('(') | Some('[') => {
            escape_name(chars);
        }
        Some(_) => {
            chars.next();
        }
        None => {}
    }
}

/// A roff special character or predefined string, as text.
///
/// Unknown names render as themselves rather than vanishing: a wrong glyph is
/// visible and fixable, a silently dropped one is not.
fn special(name: &str) -> String {
    let known = match name {
        "bu" => "\u{2022}",
        "em" | "-" => "\u{2014}",
        "en" => "\u{2013}",
        "hy" => "-",
        "aq" | "Aq" => "'",
        "dq" | "\"" => "\"",
        "lq" | "Lq" => "\u{201C}",
        "rq" | "Rq" => "\u{201D}",
        "oq" => "\u{2018}",
        "cq" => "\u{2019}",
        "ga" => "`",
        "rs" => "\\",
        "sl" => "/",
        "ba" | "bv" => "|",
        "ti" => "~",
        "ha" => "^",
        "at" => "@",
        "sh" => "#",
        "Do" => "$",
        "pc" => "\u{00B7}",
        "co" | "Co" => "\u{00A9}",
        "rg" | "Rg" => "\u{00AE}",
        "tm" | "Tm" => "\u{2122}",
        "de" | "De" => "\u{00B0}",
        "mu" => "\u{00D7}",
        "di" => "\u{00F7}",
        "+-" | "Pm" => "\u{00B1}",
        "<=" => "\u{2264}",
        ">=" => "\u{2265}",
        "!=" => "\u{2260}",
        "==" => "\u{2261}",
        "~~" | "ap" => "\u{2248}",
        "->" => "\u{2192}",
        "<-" => "\u{2190}",
        "ua" => "\u{2191}",
        "da" => "\u{2193}",
        "la" => "\u{27E8}",
        "ra" => "\u{27E9}",
        "lB" => "[",
        "rB" => "]",
        "lC" => "{",
        "rC" => "}",
        "or" => "|",
        "es" => "\u{2205}",
        "if" => "\u{221E}",
        "*a" => "\u{03B1}",
        "*b" => "\u{03B2}",
        "*p" => "\u{03C0}",
        "*m" => "\u{03BC}",
        "R" => "\u{00AE}",
        "C" => "\u{00A9}",
        // `\[u00E9]` and friends name a code point directly.
        other => {
            if let Some(hex) = other.strip_prefix('u')
                && let Some(c) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            {
                return c.to_string();
            }
            return other.to_owned();
        }
    };
    known.to_owned()
}

/// The alternating-font macros: `.BR ls (1)` is bold `ls` immediately
/// followed by roman `(1)`, with no space between arguments.
fn alternating(name: &str, args: &[String], font: &mut Font) -> Vec<Inline> {
    let fonts: Vec<Font> = match name {
        "B" | "SB" => vec![Font::Bold],
        "I" => vec![Font::Italic],
        "R" | "SM" => vec![Font::Roman],
        "BR" => vec![Font::Bold, Font::Roman],
        "RB" => vec![Font::Roman, Font::Bold],
        "BI" => vec![Font::Bold, Font::Italic],
        "IB" => vec![Font::Italic, Font::Bold],
        "IR" => vec![Font::Italic, Font::Roman],
        "RI" => vec![Font::Roman, Font::Italic],
        _ => vec![Font::Roman],
    };

    let mut out = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        // A single-font macro joins its arguments with spaces; an alternating
        // one concatenates them.
        let text = if fonts.len() == 1 && index > 0 {
            format!(" {arg}")
        } else {
            arg.clone()
        };
        let mut inner = fonts[index % fonts.len()];
        let (inlines, _) = decode(&text, &mut inner);
        out.extend(inlines);
    }
    *font = Font::Roman;
    out
}

/// A line's text with all markup resolved: what the page would print.
fn plain(text: &str) -> String {
    let (inlines, _) = decode(text, &mut Font::Roman);
    to_plain_text(&inlines)
}

// -- block accumulation ---------------------------------------------------

/// A container being filled: the root of a section, an `.RS` indent, or the
/// body of the list item currently open.
struct Frame {
    kind: FrameKind,
    blocks: Vec<Block>,
    para: Vec<Inline>,
}

enum FrameKind {
    Root,
    Indent,
    /// A run of `.TP` items. `term` is the tag of the item being filled.
    Terms {
        items: Vec<Term>,
        term: Vec<Inline>,
    },
    List {
        items: Vec<Vec<Block>>,
        ordered: bool,
    },
}

/// Turns a stream of roff events into blocks.
struct Sink {
    stack: Vec<Frame>,
}

impl Sink {
    fn new() -> Self {
        Self {
            stack: vec![Frame {
                kind: FrameKind::Root,
                blocks: Vec::new(),
                para: Vec::new(),
            }],
        }
    }

    fn top(&mut self) -> &mut Frame {
        self.stack.last_mut().expect("the root frame never pops")
    }

    fn para_mut(&mut self) -> &mut Vec<Inline> {
        &mut self.top().para
    }

    fn push_block(&mut self, block: Block) {
        self.flush_para();
        self.top().blocks.push(block);
    }

    fn flush_para(&mut self) {
        let para = std::mem::take(&mut self.top().para);
        let para = trim_inlines(para);
        if !para.is_empty() {
            self.top().blocks.push(Block::Paragraph(para));
        }
    }

    /// `.PP`: ends the paragraph and any open list, but not an `.RS` indent.
    fn para_break(&mut self) {
        self.flush_para();
        while matches!(
            self.top().kind,
            FrameKind::Terms { .. } | FrameKind::List { .. }
        ) {
            self.close_frame();
        }
    }

    /// The paragraph break implied by a following `.TP`, which ends the
    /// current item's body but keeps the list open.
    fn para_break_in_terms(&mut self) {
        self.flush_para();
    }

    fn start_term(&mut self, term: Vec<Inline>) {
        self.flush_para();
        let frame = self.stack.last_mut().expect("the root frame never pops");
        if let FrameKind::Terms { items, term: open } = &mut frame.kind {
            let previous = std::mem::replace(open, term);
            items.push(Term {
                term: previous,
                body: std::mem::take(&mut frame.blocks),
            });
            return;
        }
        while matches!(self.top().kind, FrameKind::List { .. }) {
            self.close_frame();
        }
        self.stack.push(Frame {
            kind: FrameKind::Terms {
                items: Vec::new(),
                term,
            },
            blocks: Vec::new(),
            para: Vec::new(),
        });
    }

    fn start_item(&mut self, ordered: bool) {
        self.flush_para();
        let frame = self.stack.last_mut().expect("the root frame never pops");
        if let FrameKind::List { items, .. } = &mut frame.kind {
            items.push(std::mem::take(&mut frame.blocks));
            return;
        }
        while matches!(self.top().kind, FrameKind::Terms { .. }) {
            self.close_frame();
        }
        self.stack.push(Frame {
            kind: FrameKind::List {
                items: Vec::new(),
                ordered,
            },
            blocks: Vec::new(),
            para: Vec::new(),
        });
    }

    fn start_indent(&mut self) {
        self.flush_para();
        self.stack.push(Frame {
            kind: FrameKind::Indent,
            blocks: Vec::new(),
            para: Vec::new(),
        });
    }

    /// `.RE` closes everything opened since the matching `.RS`, including any
    /// list still open inside it.
    fn end_indent(&mut self) {
        self.flush_para();
        while self.stack.len() > 1 {
            let indent = matches!(self.top().kind, FrameKind::Indent);
            self.close_frame();
            if indent {
                break;
            }
        }
    }

    /// Pop the top frame and fold it into its parent as a block.
    fn close_frame(&mut self) {
        self.flush_para();
        if self.stack.len() == 1 {
            return;
        }
        let frame = self.stack.pop().expect("checked");
        let block = match frame.kind {
            FrameKind::Root | FrameKind::Indent => Block::Group(frame.blocks),
            FrameKind::Terms { mut items, term } => {
                items.push(Term {
                    term,
                    body: frame.blocks,
                });
                Block::Terms(items)
            }
            FrameKind::List { mut items, ordered } => {
                items.push(frame.blocks);
                Block::List { ordered, items }
            }
        };
        if !is_empty_block(&block) {
            self.top().blocks.push(block);
        }
    }

    fn finish(mut self) -> Vec<Block> {
        while self.stack.len() > 1 {
            self.close_frame();
        }
        self.flush_para();
        std::mem::take(&mut self.top().blocks)
    }
}

fn is_empty_block(block: &Block) -> bool {
    match block {
        Block::Group(blocks) => blocks.is_empty(),
        Block::List { items, .. } => items.iter().all(Vec::is_empty),
        Block::Terms(items) => items
            .iter()
            .all(|item| item.term.is_empty() && item.body.is_empty()),
        _ => false,
    }
}

/// Append one line's inlines to a run, optionally separated by the space
/// roff's filling would insert.
///
/// A font left open across a line break produces two runs of the same kind on
/// either side of that space; merging them keeps `\fBtwo\nwords\fR` a single
/// bold phrase instead of two adjacent ones.
fn append(target: &mut Vec<Inline>, inlines: Vec<Inline>, space: bool) {
    let mut inlines = inlines.into_iter();
    let Some(first) = inlines.next() else {
        return;
    };
    let mergeable = matches!(
        (target.last(), &first),
        (Some(Inline::Strong(_)), Inline::Strong(_)) | (Some(Inline::Emph(_)), Inline::Emph(_))
    );
    if mergeable {
        let children = match target.last_mut() {
            Some(Inline::Strong(children) | Inline::Emph(children)) => children,
            _ => unreachable!("checked above"),
        };
        if space {
            children.push(Inline::text(" "));
        }
        match first {
            Inline::Strong(more) | Inline::Emph(more) => children.extend(more),
            _ => unreachable!("checked above"),
        }
    } else {
        if space {
            target.push(Inline::text(" "));
        }
        target.push(first);
    }
    target.extend(inlines);
}

/// Drop leading and trailing whitespace from an inline run.
fn trim_inlines(mut inlines: Vec<Inline>) -> Vec<Inline> {
    while inlines.first().is_some_and(is_blank_text) {
        inlines.remove(0);
    }
    while inlines.last().is_some_and(is_blank_text) {
        inlines.pop();
    }
    if let Some(Inline::Text(first)) = inlines.first_mut() {
        *first = first.trim_start().to_owned();
    }
    if let Some(Inline::Text(last)) = inlines.last_mut() {
        *last = last.trim_end().to_owned();
    }
    inlines
}

fn is_blank_text(inline: &Inline) -> bool {
    match inline {
        Inline::Text(text) => text.trim().is_empty(),
        Inline::LineBreak => true,
        _ => false,
    }
}

// -- pass 2: sections to a topic ------------------------------------------

fn assemble(document: Document) -> Result<Topic, ManError> {
    let Document { header, sections } = document;

    let name_section = sections
        .iter()
        .find(|section| kind(&section.title) == Kind::Name);
    if header.is_none() && name_section.is_none() {
        return Err(ManError::NotAManPage);
    }

    let (names, summary) = match name_section {
        Some(section) => split_name(&section.lines.join(" ")),
        None => (Vec::new(), String::new()),
    };
    let header = header.unwrap_or_default();

    // `.TH LS 1` shouts the page name by convention, but the entity is `ls`
    // and that is what other pages cross-reference. The NAME section spells
    // it the way the software does, so it wins when the two agree apart from
    // case.
    let name = match names
        .iter()
        .find(|name| name.eq_ignore_ascii_case(&header.name))
    {
        Some(name) => name.clone(),
        None if header.name.is_empty() => names.first().cloned().unwrap_or_default(),
        None => header.name.clone(),
    };
    let mut topic = Topic::new(name);
    // Section 2 and 3 pages document C functions; everything else documents
    // commands, and shell highlighting is the closer fit.
    topic.lang = Some(match header.section.chars().next() {
        Some('2') | Some('3') => "c".to_owned(),
        _ => "sh".to_owned(),
    });
    topic.aliases = names
        .into_iter()
        .filter(|alias| *alias != topic.name)
        .collect();
    topic.title = vec![Inline::text(if summary.is_empty() {
        topic.name.clone()
    } else {
        summary
    })];

    for section in sections {
        let blocks = section.blocks;
        match kind(&section.title) {
            Kind::Name => {}
            Kind::Synopsis => {
                let signature: Vec<String> =
                    section.lines.iter().map(|line| wrap_usage(line)).collect();
                let signature = signature.join("\n");
                if !signature.trim().is_empty() {
                    topic.signature = Some(signature.trim().to_owned());
                }
            }
            Kind::Description => topic.description.extend(blocks),
            Kind::Options => {
                let (params, rest) = split_params(blocks);
                topic.params.extend(params);
                if !rest.is_empty() {
                    topic.sections.push(Section {
                        title: vec![Inline::text(title_case(&section.title))],
                        body: rest,
                    });
                }
            }
            Kind::Value => topic.value.extend(blocks),
            Kind::Examples => match all_code(&blocks) {
                Some(examples) => topic.examples.extend(examples),
                // Examples interleaved with prose do not fit `Topic::examples`,
                // which holds code alone; keeping the section loses nothing.
                None => topic.sections.push(Section {
                    title: vec![Inline::text(title_case(&section.title))],
                    body: blocks,
                }),
            },
            Kind::SeeAlso => topic.seealso.extend(link_blocks(blocks)),
            Kind::Note => topic.note.extend(blocks),
            Kind::Author => topic.author.extend(blocks),
            Kind::References => topic.references.extend(blocks),
            Kind::Other => topic.sections.push(Section {
                title: vec![Inline::text(title_case(&section.title))],
                body: blocks,
            }),
        }
    }

    if topic.name.is_empty() {
        return Err(ManError::NotAManPage);
    }
    Ok(topic)
}

#[derive(PartialEq, Eq)]
enum Kind {
    Name,
    Synopsis,
    Description,
    Options,
    Value,
    Examples,
    SeeAlso,
    Note,
    Author,
    References,
    Other,
}

/// Which field a section title routes to.
///
/// Man page section titles are a convention, not a vocabulary: matching is
/// therefore loose, and anything unrecognised keeps its own heading rather
/// than being forced into a field.
fn kind(title: &str) -> Kind {
    let title = title.trim().to_ascii_uppercase();
    match title.as_str() {
        "NAME" => Kind::Name,
        "SYNOPSIS" | "SYNTAX" | "USAGE" => Kind::Synopsis,
        "DESCRIPTION" | "OVERVIEW" => Kind::Description,
        "RETURN VALUE" | "RETURN VALUES" | "EXIT STATUS" => Kind::Value,
        "EXAMPLE" | "EXAMPLES" => Kind::Examples,
        "SEE ALSO" => Kind::SeeAlso,
        "NOTE" | "NOTES" | "CAVEAT" | "CAVEATS" => Kind::Note,
        "AUTHOR" | "AUTHORS" | "REPORTING BUGS" => Kind::Author,
        "REFERENCE" | "REFERENCES" | "STANDARDS" => Kind::References,
        other if other.ends_with("OPTION") || other.ends_with("OPTIONS") => Kind::Options,
        _ => Kind::Other,
    }
}

/// `ls, dir \- list directory contents` splits into names and a summary.
fn split_name(text: &str) -> (Vec<String>, String) {
    let (names, summary) = match text.split_once(" - ") {
        Some(split) => split,
        None => match text.split_once(" \u{2014} ") {
            Some(split) => split,
            None => (text, ""),
        },
    };
    let names = names
        .split(',')
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .collect();
    (names, summary.trim().to_owned())
}

/// The terms of an OPTIONS section become parameters; anything else in it
/// stays a block.
fn split_params(blocks: Vec<Block>) -> (Vec<Param>, Vec<Block>) {
    let mut params = Vec::new();
    let mut rest = Vec::new();
    for block in blocks {
        match block {
            Block::Terms(terms) => params.extend(terms.into_iter().map(param_of)),
            Block::Group(inner) => {
                let (inner_params, inner_rest) = split_params(inner);
                params.extend(inner_params);
                rest.extend(inner_rest);
            }
            other => rest.push(other),
        }
    }
    (params, rest)
}

fn param_of(term: Term) -> Param {
    let text = to_plain_text(&term.term);
    // `-a, --all` documents two spellings of one option, the way Rd's
    // `\item{x, y}` documents two arguments.
    let parts: Vec<String> = text.split(',').map(|part| part.trim().to_owned()).collect();
    let names = if parts.len() > 1 && parts.iter().all(|part| part.starts_with('-')) {
        parts
    } else {
        vec![text.trim().to_owned()]
    };
    Param {
        names,
        body: term.body,
        ..Param::default()
    }
}

/// The examples of a section that holds nothing but code.
fn all_code(blocks: &[Block]) -> Option<Vec<Example>> {
    let mut examples = Vec::new();
    for block in blocks {
        match block {
            Block::Code { value, .. } => examples.push(Example {
                code: value.clone(),
                run: true,
            }),
            Block::Group(inner) => examples.extend(all_code(inner)?),
            _ => return None,
        }
    }
    (!examples.is_empty()).then_some(examples)
}

/// Turn `ls(1)`-style cross-references into topic links, which resolve to a
/// real link when the target page is converted in the same run.
fn link_blocks(blocks: Vec<Block>) -> Vec<Block> {
    blocks
        .into_iter()
        .map(|block| match block {
            Block::Paragraph(inlines) => Block::Paragraph(link_inlines(inlines)),
            Block::Group(inner) => Block::Group(link_blocks(inner)),
            other => other,
        })
        .collect()
}

fn link_inlines(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    for inline in inlines {
        match &inline {
            // `.BR ls (1)` arrives as bold `ls` followed by `(1)`.
            Inline::Text(text) if text.trim_start().starts_with('(') => {
                let name = out.last().and_then(bare_name);
                match name {
                    Some(name) if section_prefix(text.trim_start()) => {
                        out.pop();
                        out.push(Inline::Link {
                            dest: LinkDest::Topic {
                                package: None,
                                topic: name,
                            },
                            children: Vec::new(),
                        });
                        out.push(inline);
                    }
                    _ => out.push(inline),
                }
            }
            _ => out.push(inline),
        }
    }
    out
}

/// The text of an inline that is a single unadorned page name.
fn bare_name(inline: &Inline) -> Option<String> {
    let text = match inline {
        Inline::Strong(children) | Inline::Emph(children) => to_plain_text(children),
        Inline::Text(text) => text.clone(),
        _ => return None,
    };
    let text = text.trim().to_owned();
    (!text.is_empty()
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '+')))
    .then_some(text)
}

/// Whether text opens with a manual section number: `(1)`, `(3p)`.
fn section_prefix(text: &str) -> bool {
    let Some(rest) = text.strip_prefix('(') else {
        return false;
    };
    let Some(index) = rest.find(')') else {
        return false;
    };
    let inner = &rest[..index];
    !inner.is_empty()
        && inner.chars().next().is_some_and(|c| c.is_ascii_digit())
        && inner.chars().all(|c| c.is_ascii_alphanumeric())
}

/// `SEE ALSO` reads as shouting in a proportional font; the writer's other
/// headings are title case.
fn title_case(title: &str) -> String {
    title
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    format!("{}{}", first.to_uppercase(), chars.as_str().to_lowercase())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Break a long usage line the way a hand-written SYNOPSIS is laid out: one
/// continuation per fill, hanging-indented under the command name.
///
/// Generators such as `clap_mangen` emit the whole usage on one line, which
/// renders as an unreadable ribbon in a raw block that does not soft-wrap. A
/// break is only ever inserted at a top-level space, so a bracketed
/// alternation like `[-o|--output <FILE>]` stays whole.
fn wrap_usage(line: &str) -> String {
    const WIDTH: usize = 72;
    // Beyond this a hanging indent wastes more room than it buys in clarity.
    const MAX_INDENT: usize = 16;

    let trimmed = line.trim_end();
    if trimmed.chars().count() <= WIDTH {
        return trimmed.to_owned();
    }
    let leading: String = trimmed
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect::<String>();
    let groups = usage_groups(trimmed.trim_start());
    let Some((command, rest)) = groups.split_first() else {
        return trimmed.to_owned();
    };
    // A SYNOPSIS section sometimes holds prose rather than usage. Reflowing a
    // sentence under a hanging indent helps nobody, so only a line carrying
    // option or bracket groups is treated as a usage line.
    let is_usage = rest.iter().any(|group| group.starts_with(['[', '-']));
    if rest.is_empty() || !is_usage {
        return trimmed.to_owned();
    }

    let indent = format!(
        "{leading}{}",
        " ".repeat((command.chars().count() + 1).min(MAX_INDENT))
    );
    let mut lines = vec![format!("{leading}{command}")];
    for group in rest {
        let current = lines.last_mut().expect("seeded above");
        if current.chars().count() + 1 + group.chars().count() <= WIDTH {
            current.push(' ');
            current.push_str(group);
        } else {
            lines.push(format!("{indent}{group}"));
        }
    }
    lines.join("\n")
}

/// Split a usage line at spaces that are not inside a bracketed group.
fn usage_groups(line: &str) -> Vec<String> {
    let mut groups = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;

    for c in line.chars() {
        match c {
            '[' | '(' | '{' | '<' => {
                depth += 1;
                current.push(c);
            }
            ']' | ')' | '}' | '>' => {
                depth = depth.saturating_sub(1);
                current.push(c);
            }
            c if c.is_whitespace() && depth == 0 => {
                if !current.is_empty() {
                    groups.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}
