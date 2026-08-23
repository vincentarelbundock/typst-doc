//! The Typst writer: [`Topic`] to Typst markup.
//!
//! Structure and escaping ported from rd2qmd (`crates/rd2qmd-mdast/src/typst/`),
//! MIT licensed, Copyright (c) 2026 rd2md authors. See NOTICE.md.

pub mod escape;
mod html;

use crate::ir::{Align, Block, Example, Inline, LinkDest, Param, Section, Target, Term, Topic};
use escape::{escape_text_at, indent_continuation, typst_string, typst_string_array};

/// How to render the parameter list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParamsFormat {
    /// A two-column `#table`.
    #[default]
    Table,
    /// A `#terms` list.
    Terms,
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub params_format: ParamsFormat,
    /// Heading level the topic title is rendered at.
    pub base_level: u8,
}

impl Options {
    fn level(&self, relative: u8) -> u8 {
        self.base_level
            .max(1)
            .saturating_add(relative.saturating_sub(1))
    }
}

/// Render a topic as a standalone Typst document body.
pub fn topic_to_typst(topic: &Topic, options: &Options) -> String {
    let mut writer = Writer::new(options);
    writer.write_topic(topic);
    let body = writer.finish();

    // The MiTeX import is emitted only by documents that actually contain
    // math, so the common case has no external dependency.
    if topic_contains_math(topic) {
        format!("#import \"@preview/mitex:0.2.5\": mi, mitex\n\n{body}")
    } else {
        body
    }
}

struct Writer<'a> {
    out: String,
    at_line_start: bool,
    options: &'a Options,
}

impl<'a> Writer<'a> {
    fn new(options: &'a Options) -> Self {
        Self {
            out: String::new(),
            at_line_start: true,
            options,
        }
    }

    fn finish(mut self) -> String {
        while self.out.ends_with('\n') {
            self.out.pop();
        }
        self.out.push('\n');
        self.out
    }

    // -- topic ------------------------------------------------------------

    fn write_topic(&mut self, topic: &Topic) {
        self.write_heading(self.options.level(1), &topic.title, Some(&topic.name));

        if let Some(signature) = &topic.signature {
            self.write_code_block(Some("r"), signature);
        }

        self.write_section(topic, 2, "Description", &topic.description);

        if !topic.params.is_empty() {
            self.write_labelled_heading(2, "Arguments");
            self.write_params(&topic.params);
        }

        self.write_section(topic, 2, "Details", &topic.details);
        self.write_section(topic, 2, "Value", &topic.value);

        if !topic.raises.is_empty() {
            self.write_labelled_heading(2, "Raises");
            self.write_params(&topic.raises);
        }

        for section in &topic.sections {
            self.write_custom_section(section);
        }

        self.write_section(topic, 2, "Note", &topic.note);

        if !topic.examples.is_empty() {
            self.write_labelled_heading(2, "Examples");
            for example in &topic.examples {
                self.write_example(example);
            }
        }

        self.write_section(topic, 2, "See Also", &topic.seealso);
        self.write_section(topic, 2, "References", &topic.references);
        self.write_section(topic, 2, "Author", &topic.author);
    }

    fn write_section(&mut self, _topic: &Topic, level: u8, title: &str, body: &[Block]) {
        if body.is_empty() {
            return;
        }
        self.write_labelled_heading(level, title);
        self.write_blocks(body);
    }

    fn write_custom_section(&mut self, section: &Section) {
        self.ensure_blank_line();
        self.write_heading(self.options.level(2), &section.title, None);
        self.write_blocks(&section.body);
    }

    fn write_labelled_heading(&mut self, level: u8, title: &str) {
        self.write_heading(self.options.level(level), &[Inline::text(title)], None);
    }

    fn write_heading(&mut self, level: u8, content: &[Inline], label: Option<&str>) {
        if content.is_empty() && label.is_none() {
            return;
        }
        self.ensure_blank_line();
        for _ in 0..level.max(1) {
            self.out.push('=');
        }
        self.out.push(' ');
        self.at_line_start = false;
        self.write_inlines(content);
        if let Some(label) = label {
            // Labels must be valid Typst identifiers; anything else is dropped
            // rather than emitted as a broken reference target.
            if is_label_safe(label) {
                self.out.push_str(&format!(" <{label}>"));
            }
        }
        self.newline();
        self.ensure_blank_line();
    }

    fn write_params(&mut self, params: &[Param]) {
        match self.options.params_format {
            ParamsFormat::Terms => self.write_params_terms(params),
            ParamsFormat::Table => self.write_params_table(params),
        }
    }

    fn write_params_terms(&mut self, params: &[Param]) {
        self.ensure_blank_line();
        self.out.push_str("#terms(\n");
        for param in params {
            let names = self.render_isolated_inline(&[Inline::Code(param_names(param))]);
            let body = self.render_isolated(&param.body);
            self.out.push_str(&format!(
                "  ({}, [{}]),\n",
                names,
                indent_continuation(&body, 2)
            ));
        }
        self.out.push_str(")\n");
        self.at_line_start = true;
        self.ensure_blank_line();
    }

    fn write_params_table(&mut self, params: &[Param]) {
        self.ensure_blank_line();
        self.out
            .push_str("#table(\n  columns: 2,\n  stroke: none,\n");
        for param in params {
            let names = self.render_isolated_inline(&[Inline::Code(param_names(param))]);
            let mut body = self.render_isolated(&param.body);
            if let Some(ty) = &param.ty {
                let ty = self.render_isolated_inline(&[Inline::Code(ty.clone())]);
                body = if body.is_empty() {
                    ty
                } else {
                    format!("{ty} \\\n{body}")
                };
            }
            self.out.push_str(&format!(
                "  [{}], [{}],\n",
                names,
                indent_continuation(&body, 2)
            ));
        }
        self.out.push_str(")\n");
        self.at_line_start = true;
        self.ensure_blank_line();
    }

    fn write_example(&mut self, example: &Example) {
        if !example.run {
            // Not executable, but still worth showing. A comment marks why.
            self.ensure_blank_line();
            self.out.push_str("// not run\n");
            self.at_line_start = true;
        }
        self.write_code_block(Some("r"), &example.code);
    }

    // -- blocks -----------------------------------------------------------

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
                self.write_heading(self.options.level(level.saturating_add(1)), content, None);
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
                "  ([{}], [{}]),\n",
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
        for inline in inlines {
            self.write_inline(inline);
        }
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

    fn wrap(&mut self, marker: &str, children: &[Inline]) {
        self.out.push_str(marker);
        self.at_line_start = false;
        self.write_inlines(children);
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
                    None if is_label_safe(topic) => {
                        self.out.push_str(&format!("@{topic}"));
                        self.at_line_start = false;
                        return;
                    }
                    None => {
                        self.write_inline(&Inline::Code(topic.clone()));
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

    // -- layout helpers ---------------------------------------------------

    /// Render blocks to a standalone string, isolated from the current
    /// output buffer's line state.
    fn render_isolated(&self, blocks: &[Block]) -> String {
        let mut writer = Writer::new(self.options);
        writer.write_blocks(blocks);
        writer.out.trim().to_owned()
    }

    /// Render inlines to a standalone string. Inline siblings must stay on
    /// one line, so this never inserts block separation.
    fn render_isolated_inline(&self, inlines: &[Inline]) -> String {
        let mut writer = Writer::new(self.options);
        writer.at_line_start = false;
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

fn param_names(param: &Param) -> String {
    param.names.join(", ")
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
