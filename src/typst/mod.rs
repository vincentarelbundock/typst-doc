//! The Typst writer: [`Topic`] to Typst markup.
//!
//! Structure and escaping ported from rd2qmd (`crates/rd2qmd-mdast/src/typst/`),
//! MIT licensed, Copyright (c) 2026 rd2md authors. See NOTICE.md.

pub mod escape;
mod html;

use crate::ir::{Align, Block, Example, Inline, LinkDest, Param, Target, Term, Topic};
use escape::{escape_text_at, indent_continuation, typst_string, typst_string_array};

/// The rendering half of a generated document, inlined below the data block.
///
/// Inlined rather than imported, because an entry must compile on its own with
/// no file beside it. The cost is that every entry carries a copy; the benefit
/// is that a manual is a directory of self-contained documents.
pub const DEFAULT_TEMPLATE: &str = include_str!("template.typ");

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// The template inlined after each topic's data, or [`DEFAULT_TEMPLATE`]
    /// when absent.
    pub template: Option<String>,
    /// Every topic name in the run that addresses exactly one heading, mapped
    /// to the label that heading carries. A shared name is absent here and
    /// resolves, if at all, through the referring topic's [`Entry::scope`].
    /// A target missing from both degrades to plain code, since a link to a
    /// label the document never defines, or defines twice, is a Typst compile
    /// error rather than just a dead end.
    pub labels: std::collections::HashMap<String, String>,
}

impl Options {
    /// The template to inline: the caller's, or the built-in default.
    pub fn template(&self) -> &str {
        self.template.as_deref().unwrap_or(DEFAULT_TEMPLATE)
    }
}

/// What a topic cannot know about itself, because it depends on the other
/// topics in the same run.
///
/// Both fields answer one question, which of the same-named topics is this,
/// for the two audiences that ask it: [`label`](Entry::label) for Typst, which
/// resolves references, and [`source`](Entry::source) for the reader, who would
/// otherwise face two entries with identical names and signatures.
#[derive(Debug, Clone, Copy, Default)]
pub struct Entry<'a> {
    /// The label the topic's heading carries. Defaults to the topic's name,
    /// which is the right answer whenever that name is unique in the run.
    pub label: Option<&'a str>,
    /// The file the topic was read from, shown under its signature. Set only
    /// when another topic shares the name, so an unambiguous entry carries no
    /// provenance noise.
    pub source: Option<&'a str>,
    /// Labels that only this topic's own scope can resolve, taking precedence
    /// over [`Options::labels`]. A reference is written from somewhere, and a
    /// shared name means the nearest one, the way an import does.
    pub scope: Option<&'a std::collections::HashMap<String, String>>,
}

/// Render a topic as a standalone Typst document.
///
/// The document is in two halves. The first binds the topic's content to a
/// fixed set of `doc-` variables; the second is the template, which renders
/// them. Only the second half decides how anything looks, so restyling a
/// manual means replacing it, never adding an option here.
pub fn topic_to_typst(topic: &Topic, entry: &Entry, options: &Options) -> String {
    let writer = Writer::new(entry, options);
    let body = format!("{}\n{}", writer.data_block(topic), options.template());

    // The MiTeX import is emitted only by documents that actually contain
    // math, so the common case has no external dependency.
    if topic_contains_math(topic) {
        format!("#import \"@preview/mitex:0.2.7\": mi, mitex\n\n{body}")
    } else {
        body
    }
}

struct Writer<'a> {
    out: String,
    at_line_start: bool,
    /// The first character of the inline that follows the one being written,
    /// which decides whether emphasis can use markers or needs a call.
    next: Option<char>,
    entry: &'a Entry<'a>,
    options: &'a Options,
}

impl<'a> Writer<'a> {
    fn new(entry: &'a Entry<'a>, options: &'a Options) -> Self {
        Self {
            out: String::new(),
            at_line_start: true,
            next: None,
            entry,
            options,
        }
    }

    // -- data -------------------------------------------------------------

    /// Bind everything the template may render to a fixed set of variables.
    ///
    /// Every variable is emitted for every topic, empty where the topic has
    /// nothing: a template that reads `doc-examples` must not fail on the one
    /// entry that happens to have none.
    fn data_block(&self, topic: &Topic) -> String {
        let label = self.entry.label.unwrap_or(&topic.name);
        let mut out = String::new();
        out.push_str(&format!("#let doc-name = {}\n", typst_string(&topic.name)));
        // A label literal, so the reference machinery still sees `<name>` in
        // the source. Names Typst cannot spell as a label carry none.
        out.push_str(&format!(
            "#let doc-label = {}\n",
            if is_label_safe(label) {
                format!("<{label}>")
            } else {
                "none".to_owned()
            }
        ));
        out.push_str(&format!(
            "#let doc-title = [{}]\n",
            self.render_isolated_inline(&topic.title)
        ));
        out.push_str(&format!(
            "#let doc-aliases = {}\n",
            typst_string_array(&topic.aliases)
        ));
        out.push_str(&format!(
            "#let doc-source = {}\n",
            optional_string(self.entry.source)
        ));
        out.push_str(&format!(
            "#let doc-signature = {}\n",
            match &topic.signature {
                Some(signature) => raw_literal(topic.lang.as_deref(), signature),
                None => "none".to_owned(),
            }
        ));
        out.push_str(&format!(
            "#let doc-params = {}\n",
            self.params_array(&topic.params)
        ));
        out.push_str(&format!(
            "#let doc-raises = {}\n",
            self.params_array(&topic.raises)
        ));
        out.push_str(&format!(
            "#let doc-examples = {}\n",
            self.examples_array(topic.lang.as_deref(), &topic.examples)
        ));
        out.push_str(&format!(
            "#let doc-sections = {}\n",
            self.sections_array(topic)
        ));
        out
    }

    fn params_array(&self, params: &[Param]) -> String {
        if params.is_empty() {
            return "()".to_owned();
        }
        let mut out = String::from("(\n");
        for param in params {
            out.push_str(&format!(
                "  (names: {}, type: {}, default: {}, optional: {}, body: [{}]),\n",
                typst_string_array(&param.names),
                optional_string(param.ty.as_deref()),
                optional_string(param.default.as_deref()),
                param.optional,
                indent_continuation(&self.render_isolated(&param.body), 4)
            ));
        }
        out.push(')');
        out
    }

    fn examples_array(&self, lang: Option<&str>, examples: &[Example]) -> String {
        if examples.is_empty() {
            return "()".to_owned();
        }
        let mut out = String::from("(\n");
        for example in examples {
            // `run` is what `\dontrun{}` meant, kept as data: whether an
            // example is executable is the template's to act on, or ignore.
            out.push_str(&format!(
                "  (run: {}, code: {}),\n",
                example.run,
                raw_literal(lang, &example.code)
            ));
        }
        out.push(')');
        out
    }

    /// Every section the topic has, in order, each tagged with what it holds.
    ///
    /// Order is data rather than control flow, so a template renders the whole
    /// entry in one loop and reorders or retitles by manipulating this list.
    /// Empty sections are absent, which is why the loop needs no guards.
    fn sections_array(&self, topic: &Topic) -> String {
        let mut entries: Vec<String> = Vec::new();
        let prose = |id: &str, title: &str, body: &[Block]| {
            if body.is_empty() {
                return None;
            }
            Some(format!(
                "  (id: \"{id}\", title: [{title}], kind: \"prose\", body: [\n{}\n  ]),\n",
                self.render_isolated(body)
            ))
        };

        entries.extend(prose("description", "Description", &topic.description));
        if !topic.params.is_empty() {
            entries.push(
                "  (id: \"arguments\", title: [Arguments], kind: \"params\", items: doc-params),\n"
                    .to_owned(),
            );
        }
        entries.extend(prose("details", "Details", &topic.details));
        entries.extend(prose("value", "Value", &topic.value));
        if !topic.raises.is_empty() {
            entries.push(
                "  (id: \"raises\", title: [Raises], kind: \"params\", items: doc-raises),\n"
                    .to_owned(),
            );
        }
        for section in &topic.sections {
            entries.push(format!(
                "  (id: \"custom\", title: [{}], kind: \"prose\", body: [\n{}\n  ]),\n",
                self.render_isolated_inline(&section.title),
                self.render_isolated(&section.body)
            ));
        }
        entries.extend(prose("note", "Note", &topic.note));
        if !topic.examples.is_empty() {
            entries.push(
                "  (id: \"examples\", title: [Examples], kind: \"examples\", items: doc-examples),\n"
                    .to_owned(),
            );
        }
        entries.extend(prose("seealso", "See Also", &topic.seealso));
        entries.extend(prose("references", "References", &topic.references));
        entries.extend(prose("author", "Author", &topic.author));

        if entries.is_empty() {
            return "()".to_owned();
        }
        format!("(\n{})", entries.concat())
    }

    // -- blocks -----------------------------------------------------------

    /// A heading inside a section's prose. Manual sections themselves are
    /// headings the template makes, not markup this writer emits.
    fn write_heading(&mut self, level: u8, content: &[Inline]) {
        if content.is_empty() {
            return;
        }
        self.ensure_blank_line();
        for _ in 0..level.max(1) {
            self.out.push('=');
        }
        self.out.push(' ');
        self.at_line_start = false;
        self.write_inlines(content);
        self.newline();
        self.ensure_blank_line();
    }

    fn write_blocks(&mut self, blocks: &[Block]) {
        for block in blocks {
            self.write_block(block);
        }
    }

    fn write_block(&mut self, block: &Block) {
        match block {
            Block::Paragraph(children) => {
                self.ensure_blank_line();
                self.write_inlines(children);
                self.newline();
            }
            Block::Heading { level, content } => {
                self.write_heading(level.saturating_add(1), content);
            }
            Block::Code { lang, value } => self.write_code_block(lang.as_deref(), value),
            Block::List { ordered, items } => self.write_list(*ordered, items),
            Block::Terms(terms) => self.write_terms(terms),
            Block::Table { align, rows } => self.write_table(align, rows),
            Block::DisplayMath(latex) => {
                self.ensure_blank_line();
                self.out
                    .push_str(&format!("#mitex(`{}`)\n", latex.replace('`', "\\`")));
                self.at_line_start = true;
            }
            Block::Group(children) => self.write_blocks(children),
            Block::Targeted {
                target,
                then,
                otherwise,
            } => self.write_targeted_blocks(*target, then, otherwise),
            Block::Html(html) => self.write_html(html),
            Block::Raw(value) => {
                self.ensure_blank_line();
                self.out.push_str(value);
                self.at_line_start = value.ends_with('\n');
                self.newline();
            }
        }
    }

    fn write_code_block(&mut self, lang: Option<&str>, value: &str) {
        self.ensure_blank_line();
        // A plain raw block, never an executable one: plain Typst renders a
        // highlighted listing, and executing engines can opt in themselves.
        let fence = longest_backtick_run(value).max(2) + 1;
        let ticks = "`".repeat(fence);
        self.out.push_str(&ticks);
        if let Some(lang) = lang {
            self.out.push_str(lang);
        }
        self.out.push('\n');
        self.out.push_str(value.trim_end());
        self.out.push('\n');
        self.out.push_str(&ticks);
        self.newline();
        self.at_line_start = true;
        self.ensure_blank_line();
    }

    fn write_list(&mut self, ordered: bool, items: &[Vec<Block>]) {
        self.ensure_blank_line();
        let marker = if ordered { "+" } else { "-" };
        for item in items {
            let body = self.render_isolated(item);
            self.out.push_str(marker);
            self.out.push(' ');
            self.out.push_str(&indent_continuation(&body, 2));
            self.out.push('\n');
            self.at_line_start = true;
        }
        self.ensure_blank_line();
    }

    fn write_terms(&mut self, terms: &[Term]) {
        self.ensure_blank_line();
        self.out.push_str("#terms(\n");
        for term in terms {
            let head = self.render_isolated_inline(&term.term);
            let body = self.render_isolated(&term.body);
            self.out.push_str(&format!(
                "  terms.item([{}], [{}]),\n",
                head,
                indent_continuation(&body, 2)
            ));
        }
        self.out.push_str(")\n");
        self.at_line_start = true;
        self.ensure_blank_line();
    }

    fn write_table(&mut self, align: &[Align], rows: &[Vec<Vec<Inline>>]) {
        self.ensure_blank_line();
        let columns = align
            .len()
            .max(rows.iter().map(Vec::len).max().unwrap_or(0));
        self.out
            .push_str(&format!("#table(\n  columns: {columns},\n"));
        if !align.is_empty() {
            let names: Vec<_> = align
                .iter()
                .map(|a| match a {
                    Align::Left => "left",
                    Align::Center => "center",
                    Align::Right => "right",
                })
                .collect();
            self.out
                .push_str(&format!("  align: ({},),\n", names.join(", ")));
        }
        for row in rows {
            let cells: Vec<_> = row
                .iter()
                .map(|cell| format!("[{}]", self.render_isolated_inline(cell)))
                .collect();
            self.out.push_str(&format!("  {},\n", cells.join(", ")));
        }
        self.out.push_str(")\n");
        self.at_line_start = true;
        self.ensure_blank_line();
    }

    /// A branch that applies to one output target only.
    ///
    /// Both branches survive into the document: one Typst source compiles to
    /// both PDF and HTML, so the choice belongs to `target()` at compile time
    /// rather than to this writer.
    fn write_targeted_blocks(&mut self, target: Target, then: &[Block], otherwise: &[Block]) {
        if then.is_empty() && otherwise.is_empty() {
            return;
        }
        self.ensure_blank_line();
        let then = indent_continuation(&self.render_isolated(then), 2);
        self.out.push_str(&format!(
            "#context {{\n  if {} [{}]",
            target.condition(),
            then
        ));
        if !otherwise.is_empty() {
            let otherwise = indent_continuation(&self.render_isolated(otherwise), 2);
            self.out.push_str(&format!(" else [{otherwise}]"));
        }
        self.out.push_str("\n}\n");
        self.at_line_start = true;
        self.ensure_blank_line();
    }

    /// Raw HTML from `\out{}`. Handled in [`html`].
    fn write_html(&mut self, html: &str) {
        self.ensure_blank_line();
        self.write_html_fragment(html);
        self.newline();
        self.ensure_blank_line();
    }

    // -- inlines ----------------------------------------------------------

    fn write_inlines(&mut self, inlines: &[Inline]) {
        for (index, inline) in inlines.iter().enumerate() {
            // Emphasis markers need a word boundary on both sides, so the
            // character that follows decides how emphasis is written.
            self.next = inlines.get(index + 1).and_then(leading_char);
            self.write_inline(inline);
        }
        self.next = None;
    }

    fn write_inline(&mut self, inline: &Inline) {
        match inline {
            Inline::Text(value) => {
                let escaped = escape_text_at(value, self.at_line_start);
                self.out.push_str(&escaped);
                // Whitespace does not end the start-of-line state: Typst reads
                // an indented `-` as a list marker too, and inline text often
                // arrives split across several text runs.
                if !escaped.trim().is_empty() || escaped.contains('\n') {
                    self.at_line_start = escaped.ends_with('\n');
                }
            }
            Inline::Raw(value) => {
                // Already Typst markup, authored as such; escaping would
                // mangle what the author wrote deliberately.
                self.out.push_str(value);
                self.at_line_start = false;
            }
            Inline::Code(value) | Inline::Verb(value) => {
                let fence = longest_backtick_run(value) + 1;
                let ticks = "`".repeat(fence);
                self.out.push_str(&ticks);
                self.out.push_str(value);
                self.out.push_str(&ticks);
                self.at_line_start = false;
            }
            Inline::Emph(children) => self.wrap("_", children),
            Inline::Strong(children) => self.wrap("*", children),
            Inline::Math(latex) => {
                self.out
                    .push_str(&format!("#mi(`{}`)", latex.replace('`', "\\`")));
                self.at_line_start = false;
            }
            Inline::LineBreak => {
                self.out.push_str(" \\\n");
                self.at_line_start = true;
            }
            Inline::Sexpr(code) => {
                // Visibly unevaluated: `\Sexpr` needs a live R session.
                self.write_inline(&Inline::Code(format!("\\Sexpr{{{code}}}")));
            }
            Inline::Targeted {
                target,
                then,
                otherwise,
            } => {
                let rendered_then = self.render_isolated_inline(then);
                self.out.push_str(&format!(
                    "#context {{ if {} [{}]",
                    target.condition(),
                    rendered_then
                ));
                if !otherwise.is_empty() {
                    let rendered = self.render_isolated_inline(otherwise);
                    self.out.push_str(&format!(" else [{rendered}]"));
                }
                self.out.push_str(" }");
                self.at_line_start = false;
            }
            Inline::Link { dest, children } => self.write_link(dest, children),
        }
    }

    /// Write emphasis or strong content.
    ///
    /// Typst's `*`/`_` markers only delimit at a word boundary, so `_n_th`
    /// never closes its emphasis and the document fails to parse. Where a
    /// boundary is missing, or the content itself starts or ends with a
    /// space, which the markers also reject, the function form carries the
    /// same meaning with no such constraint.
    fn wrap(&mut self, marker: &str, children: &[Inline]) {
        let inner = self.render_isolated_inline(children);
        // A slash beside a `*` is the other hazard: `/*` opens a block comment
        // and `*/` closes one, so `*/etc*` is not a bold path.
        let unsafe_boundary = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '/');
        if inner.is_empty()
            || unsafe_boundary(self.out.chars().last())
            || unsafe_boundary(self.next)
            || inner.starts_with(char::is_whitespace)
            || inner.ends_with(char::is_whitespace)
            || inner.starts_with('/')
            || inner.ends_with('/')
        {
            let function = if marker == "*" { "strong" } else { "emph" };
            self.out.push_str(&format!("#{function}[{inner}]"));
            self.at_line_start = false;
            return;
        }

        self.out.push_str(marker);
        self.out.push_str(&inner);
        self.out.push_str(marker);
        self.at_line_start = false;
    }

    fn write_link(&mut self, dest: &LinkDest, children: &[Inline]) {
        let (target, fallback) = match dest {
            LinkDest::Url(url) => (typst_string(url), url.clone()),
            LinkDest::Email(address) => {
                (typst_string(&format!("mailto:{address}")), address.clone())
            }
            LinkDest::Doi(doi) => (
                typst_string(&format!("https://doi.org/{doi}")),
                format!("doi:{doi}"),
            ),
            LinkDest::Topic { package, topic } => {
                // Cross-package references have no in-document target, so they
                // render as text rather than a dangling link.
                match package {
                    Some(package) => {
                        self.write_inline(&Inline::Code(format!("{package}::{topic}")));
                        return;
                    }
                    // `@name` would be shorter, but referencing an unnumbered
                    // heading is a Typst error; a `#link` to the label is not.
                    // The label is looked up rather than assumed to be the
                    // name, since a name two topics share addresses neither.
                    None => {
                        match self.label_of(topic) {
                            Some(label) => {
                                self.out
                                    .push_str(&format!("#link(label({}))[", typst_string(label)));
                                self.at_line_start = false;
                                self.write_inline(&Inline::Code(topic.clone()));
                                self.out.push(']');
                            }
                            None => self.write_inline(&Inline::Code(topic.clone())),
                        }
                        return;
                    }
                }
            }
        };

        self.out.push_str(&format!("#link({target})["));
        self.at_line_start = false;
        if children.is_empty() {
            self.write_inline(&Inline::Code(fallback));
        } else {
            self.write_inlines(children);
        }
        self.out.push(']');
        self.at_line_start = false;
    }

    /// The label a link to `name` should address, nearest scope first.
    fn label_of(&self, name: &str) -> Option<&String> {
        self.entry
            .scope
            .and_then(|scope| scope.get(name))
            .or_else(|| self.options.labels.get(name))
            .filter(|label| is_label_safe(label))
    }

    // -- layout helpers ---------------------------------------------------

    /// Render blocks to a standalone string, isolated from the current
    /// output buffer's line state.
    fn render_isolated(&self, blocks: &[Block]) -> String {
        let mut writer = Writer::new(self.entry, self.options);
        writer.write_blocks(blocks);
        writer.out.trim().to_owned()
    }

    /// Render inlines to a standalone string. Inline siblings must stay on
    /// one line, so this never inserts block separation.
    fn render_isolated_inline(&self, inlines: &[Inline]) -> String {
        let mut writer = Writer::new(self.entry, self.options);
        // Every caller drops the result into a fresh `[..]`, where markup
        // starts anew: a leading `/`, `-`, or `=` there is a list marker, not
        // text, exactly as at the start of a line.
        writer.at_line_start = true;
        writer.write_inlines(inlines);
        writer.out.trim().to_owned()
    }

    fn newline(&mut self) {
        if !self.at_line_start {
            self.out.push('\n');
            self.at_line_start = true;
        }
    }

    fn ensure_blank_line(&mut self) {
        self.newline();
        if !self.out.is_empty() && !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
    }
}

/// A Typst string literal, or `none` where there is nothing to say.
fn optional_string(value: Option<&str>) -> String {
    value.map_or_else(|| "none".to_owned(), typst_string)
}

/// A raw block literal, usable in code position: `` ```lang … ``` ``.
///
/// The fence is longer than any backtick run inside, so code containing
/// backticks cannot close it early.
fn raw_literal(lang: Option<&str>, value: &str) -> String {
    let ticks = "`".repeat(longest_backtick_run(value).max(2) + 1);
    format!(
        "{ticks}{}\n{}\n{ticks}",
        lang.unwrap_or(""),
        value.trim_end()
    )
}

/// Whether a string is usable as a Typst label without escaping.
fn is_label_safe(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

fn longest_backtick_run(value: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for character in value.chars() {
        if character == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

fn topic_contains_math(topic: &Topic) -> bool {
    let blocks = [
        &topic.description,
        &topic.details,
        &topic.value,
        &topic.seealso,
        &topic.references,
        &topic.note,
        &topic.author,
    ];
    blocks.iter().any(|b| blocks_contain_math(b))
        || topic.params.iter().any(|p| blocks_contain_math(&p.body))
        || topic.raises.iter().any(|p| blocks_contain_math(&p.body))
        || topic.sections.iter().any(|s| blocks_contain_math(&s.body))
        || inlines_contain_math(&topic.title)
}

fn blocks_contain_math(blocks: &[Block]) -> bool {
    blocks.iter().any(|block| match block {
        Block::DisplayMath(_) => true,
        Block::Paragraph(children)
        | Block::Heading {
            content: children, ..
        } => inlines_contain_math(children),
        Block::List { items, .. } => items.iter().any(|item| blocks_contain_math(item)),
        Block::Terms(terms) => terms
            .iter()
            .any(|t| inlines_contain_math(&t.term) || blocks_contain_math(&t.body)),
        Block::Table { rows, .. } => rows
            .iter()
            .any(|row| row.iter().any(|cell| inlines_contain_math(cell))),
        Block::Group(children) => blocks_contain_math(children),
        Block::Targeted {
            then, otherwise, ..
        } => blocks_contain_math(then) || blocks_contain_math(otherwise),
        Block::Code { .. } | Block::Html(_) | Block::Raw(_) => false,
    })
}

fn inlines_contain_math(inlines: &[Inline]) -> bool {
    inlines.iter().any(|inline| match inline {
        Inline::Math(_) => true,
        Inline::Emph(children) | Inline::Strong(children) => inlines_contain_math(children),
        Inline::Link { children, .. } => inlines_contain_math(children),
        Inline::Targeted {
            then, otherwise, ..
        } => inlines_contain_math(then) || inlines_contain_math(otherwise),
        _ => false,
    })
}

#[allow(dead_code)]
fn unused_string_array(values: &[String]) -> String {
    typst_string_array(values)
}

/// The first character an inline will render as, where knowing it matters:
/// emphasis markers need a non-word character on either side.
fn leading_char(inline: &Inline) -> Option<char> {
    match inline {
        Inline::Text(value) | Inline::Raw(value) => value.chars().next(),
        Inline::Verb(_) | Inline::Code(_) | Inline::Sexpr(_) => Some('`'),
        Inline::Emph(children) | Inline::Strong(children) => {
            children.first().and_then(leading_char)
        }
        Inline::Math(_) | Inline::Link { .. } | Inline::Targeted { .. } => Some('#'),
        Inline::LineBreak => Some(' '),
    }
}
