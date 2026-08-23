//! The R reader: Rd source to [`Topic`].
//!
//! Section dispatch and the inline tag mapping follow rd2qmd
//! (`crates/rd2qmd-core/`), MIT licensed, Copyright (c) 2026 rd2md authors.
//! See NOTICE.md.

use rd_ast::{RdDocument, RdNode, RdTag};

use crate::ir::{Align, Block, Example, Inline, LinkDest, Param, Section, Term, Topic};

/// Parse Rd source and convert it to a [`Topic`].
pub fn parse(source: &str) -> Result<Topic, rd_source::ParseError> {
    let document = rd_source::parse(source.as_bytes())?.into_parts().0;
    Ok(from_document(&document))
}

/// Convert an already-parsed Rd document to a [`Topic`].
pub fn from_document(document: &RdDocument) -> Topic {
    let mut topic = Topic::default();

    for node in document.nodes() {
        let Some(tagged) = node.as_tagged() else {
            continue;
        };
        let children = tagged.children();
        match tagged.tag() {
            RdTag::Name => topic.name = plain_text(children).trim().to_owned(),
            RdTag::Title => topic.title = inlines(children),
            RdTag::Alias => topic.aliases.push(plain_text(children).trim().to_owned()),
            RdTag::Keyword => topic.keywords.push(plain_text(children).trim().to_owned()),
            RdTag::Concept => topic.keywords.push(plain_text(children).trim().to_owned()),
            RdTag::Usage => {
                let code = plain_text(children);
                let code = code.trim();
                if !code.is_empty() {
                    topic.signature = Some(code.to_owned());
                }
            }
            RdTag::Description => topic.description = blocks(children),
            RdTag::Details => topic.details = blocks(children),
            RdTag::Value => topic.value = blocks(children),
            RdTag::Note => topic.note = blocks(children),
            RdTag::Author => topic.author = blocks(children),
            RdTag::References => topic.references = blocks(children),
            RdTag::SeeAlso => topic.seealso = blocks(children),
            RdTag::Arguments => topic.params = params(children),
            RdTag::Examples => topic.examples = examples(children),
            RdTag::Section => {
                // `\section{title}{body}`: the title is the option-like first
                // group, the body the second.
                let groups = groups(children);
                if groups.len() >= 2 {
                    topic.sections.push(Section {
                        title: inlines(&groups[0]),
                        body: blocks(&groups[1]),
                    });
                }
            }
            RdTag::Format | RdTag::Source => {
                topic.sections.push(Section {
                    title: vec![Inline::text(match tagged.tag() {
                        RdTag::Format => "Format",
                        _ => "Source",
                    })],
                    body: blocks(children),
                });
            }
            _ => {}
        }
    }

    topic
}

/// Split a node list into its positional argument groups.
fn groups(nodes: &[RdNode]) -> Vec<Vec<RdNode>> {
    nodes
        .iter()
        .filter_map(|node| node.as_group().map(|g| g.children().to_vec()))
        .collect()
}

/// `\arguments{ \item{name}{description} ... }`
fn params(nodes: &[RdNode]) -> Vec<Param> {
    let mut out = Vec::new();
    for node in nodes {
        let Some(tagged) = node.as_tagged() else {
            continue;
        };
        if tagged.tag() != &RdTag::Item {
            continue;
        }
        let parts = groups(tagged.children());
        if parts.len() < 2 {
            continue;
        }
        let names = plain_text(&parts[0])
            .split(',')
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .collect();
        out.push(Param {
            names,
            ty: None,
            default: None,
            optional: false,
            body: blocks(&parts[1]),
        });
    }
    out
}

fn examples(nodes: &[RdNode]) -> Vec<Example> {
    let mut out = Vec::new();
    let mut runnable = String::new();

    for node in nodes {
        match node {
            RdNode::Tagged(tagged)
                if matches!(
                    tagged.tag(),
                    RdTag::DontRun | RdTag::DontTest | RdTag::DontShow | RdTag::TestOnly
                ) =>
            {
                let code = plain_text(tagged.children());
                if !code.trim().is_empty() {
                    // Flush what precedes so example order is preserved.
                    push_example(&mut out, &mut runnable, true);
                    out.push(Example {
                        code: code.trim().to_owned(),
                        run: false,
                    });
                }
            }
            other => runnable.push_str(&plain_text(std::slice::from_ref(other))),
        }
    }
    push_example(&mut out, &mut runnable, true);
    out
}

fn push_example(out: &mut Vec<Example>, buffer: &mut String, run: bool) {
    let code = buffer.trim();
    if !code.is_empty() {
        out.push(Example {
            code: code.to_owned(),
            run,
        });
    }
    buffer.clear();
}

// -- blocks ---------------------------------------------------------------

/// Convert a node list to blocks, grouping inline runs into paragraphs on
/// blank lines.
fn blocks(nodes: &[RdNode]) -> Vec<Block> {
    let mut out = Vec::new();
    let mut pending: Vec<Inline> = Vec::new();

    for node in nodes {
        if let Some(block) = as_block(node) {
            flush_paragraph(&mut out, &mut pending);
            out.push(block);
            continue;
        }

        if let RdNode::Text(text) = node {
            // A blank line ends the paragraph.
            let mut parts = text.split("\n\n").peekable();
            while let Some(part) = parts.next() {
                let collapsed = collapse(part);
                // A separator between two inline nodes is worth one space; the
                // same whitespace at the start of a paragraph is not.
                if !collapsed.is_empty() && !(collapsed.trim().is_empty() && pending.is_empty()) {
                    pending.push(Inline::Text(collapsed));
                }
                if parts.peek().is_some() {
                    flush_paragraph(&mut out, &mut pending);
                }
            }
            continue;
        }

        pending.extend(inlines(std::slice::from_ref(node)));
    }

    flush_paragraph(&mut out, &mut pending);
    out
}

fn flush_paragraph(out: &mut Vec<Block>, pending: &mut Vec<Inline>) {
    while matches!(pending.last(), Some(Inline::Text(t)) if t.trim().is_empty()) {
        pending.pop();
    }
    if pending.is_empty() {
        return;
    }
    out.push(Block::Paragraph(std::mem::take(pending)));
}

/// A block-level Rd construct, or `None` if the node is inline.
fn as_block(node: &RdNode) -> Option<Block> {
    let tagged = node.as_tagged()?;
    let children = tagged.children();
    match tagged.tag() {
        RdTag::Itemize => Some(Block::List {
            ordered: false,
            items: list_items(children),
        }),
        RdTag::Enumerate => Some(Block::List {
            ordered: true,
            items: list_items(children),
        }),
        RdTag::Describe => Some(Block::Terms(terms(children))),
        RdTag::Preformatted => Some(Block::Code {
            lang: None,
            value: plain_text(children).trim_end().to_owned(),
        }),
        RdTag::Deqn => Some(Block::DisplayMath(latex_of(tagged.children()))),
        RdTag::Tabular => Some(table(children)),
        RdTag::Out => Some(Block::Html(plain_text(children))),
        RdTag::Subsection => {
            let parts = groups(children);
            if parts.len() >= 2 {
                // Rendered as a heading followed by its body; the body blocks
                // are hoisted, since the IR has no nested-section block.
                Some(Block::Heading {
                    level: 2,
                    content: inlines(&parts[0]),
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Split `\itemize`/`\enumerate` children into items.
///
/// Unlike `\describe` and `\arguments`, `\item` here is a zero-arity marker:
/// the item's content follows it as siblings, up to the next marker.
fn list_items(nodes: &[RdNode]) -> Vec<Vec<Block>> {
    let mut items: Vec<Vec<RdNode>> = Vec::new();
    for node in nodes {
        let is_marker = node
            .as_tagged()
            .is_some_and(|tagged| tagged.tag() == &RdTag::Item);
        if is_marker {
            items.push(Vec::new());
        } else if let Some(current) = items.last_mut() {
            current.push(node.clone());
        }
        // Content before the first marker is not part of any item.
    }
    items.iter().map(|item| blocks(item)).collect()
}

fn terms(nodes: &[RdNode]) -> Vec<Term> {
    nodes
        .iter()
        .filter_map(|node| node.as_tagged())
        .filter(|tagged| tagged.tag() == &RdTag::Item)
        .filter_map(|tagged| {
            let parts = groups(tagged.children());
            (parts.len() >= 2).then(|| Term {
                term: inlines(&parts[0]),
                body: blocks(&parts[1]),
            })
        })
        .collect()
}

/// `\tabular{lcr}{ a \tab b \cr ... }`
///
/// Both the column spec and the cells arrive as positional groups, not as a
/// tag option.
fn table(nodes: &[RdNode]) -> Block {
    let parts = groups(nodes);
    let spec = parts.first().map(|g| plain_text(g)).unwrap_or_default();
    let cells: &[RdNode] = parts.get(1).map(Vec::as_slice).unwrap_or(nodes);

    let align = spec
        .trim()
        .chars()
        .filter_map(|c| match c {
            'l' => Some(Align::Left),
            'c' => Some(Align::Center),
            'r' => Some(Align::Right),
            _ => None,
        })
        .collect();

    let mut rows: Vec<Vec<Vec<Inline>>> = Vec::new();
    let mut row: Vec<Vec<Inline>> = Vec::new();
    let mut cell: Vec<RdNode> = Vec::new();

    for node in cells {
        match node.as_tagged().map(|t| t.tag()) {
            Some(RdTag::Tab) => {
                row.push(inlines(&cell));
                cell.clear();
            }
            Some(RdTag::Cr) => {
                row.push(inlines(&cell));
                cell.clear();
                rows.push(std::mem::take(&mut row));
            }
            _ => cell.push(node.clone()),
        }
    }
    if !cell.is_empty() {
        row.push(inlines(&cell));
    }
    if !row.is_empty() {
        rows.push(row);
    }
    // Trailing `\cr` produces a final empty row.
    rows.retain(|row| row.iter().any(|cell| !cell.is_empty()));

    Block::Table { align, rows }
}

// -- inlines --------------------------------------------------------------

fn inlines(nodes: &[RdNode]) -> Vec<Inline> {
    let mut out = Vec::new();
    for node in nodes {
        match node {
            RdNode::Text(text) => out.push(Inline::Text(collapse(text))),
            RdNode::RCode(code) | RdNode::Verb(code) => out.push(Inline::Code(code.clone())),
            RdNode::Comment(_) => {}
            RdNode::Group(group) => out.extend(inlines(group.children())),
            RdNode::Raw(_) => {}
            RdNode::Tagged(tagged) => {
                let children = tagged.children();
                match tagged.tag() {
                    RdTag::Code
                    | RdTag::Samp
                    | RdTag::Kbd
                    | RdTag::Env
                    | RdTag::Command
                    | RdTag::Option
                    | RdTag::File
                    | RdTag::Var => out.push(Inline::Code(plain_text(children))),
                    RdTag::Emph | RdTag::Cite | RdTag::Dfn => {
                        out.push(Inline::Emph(inlines(children)))
                    }
                    RdTag::Strong | RdTag::Bold => out.push(Inline::Strong(inlines(children))),
                    RdTag::Pkg | RdTag::Acronym | RdTag::Abbr => {
                        out.extend(inlines(children));
                    }
                    RdTag::CranPkg => {
                        let pkg = plain_text(children);
                        out.push(Inline::Link {
                            dest: LinkDest::Url(format!(
                                "https://CRAN.R-project.org/package={pkg}"
                            )),
                            children: vec![Inline::Code(pkg)],
                        });
                    }
                    RdTag::SQuote => out.push(Inline::Text(format!(
                        "\u{2018}{}\u{2019}",
                        plain_text(children)
                    ))),
                    RdTag::DQuote => out.push(Inline::Text(format!(
                        "\u{201c}{}\u{201d}",
                        plain_text(children)
                    ))),
                    RdTag::Url => {
                        let url = plain_text(children);
                        out.push(Inline::Link {
                            dest: LinkDest::Url(url),
                            children: Vec::new(),
                        });
                    }
                    RdTag::Email => out.push(Inline::Link {
                        dest: LinkDest::Email(plain_text(children)),
                        children: Vec::new(),
                    }),
                    RdTag::Doi => out.push(Inline::Link {
                        dest: LinkDest::Doi(plain_text(children)),
                        children: Vec::new(),
                    }),
                    RdTag::Href => {
                        // `\href{url}{text}`
                        let parts = groups(children);
                        let (url, text) = match parts.len() {
                            0 => (plain_text(children), Vec::new()),
                            1 => (plain_text(&parts[0]), Vec::new()),
                            _ => (plain_text(&parts[0]), inlines(&parts[1])),
                        };
                        out.push(Inline::Link {
                            dest: LinkDest::Url(url),
                            children: text,
                        });
                    }
                    RdTag::Link | RdTag::LinkS4Class => {
                        let topic = plain_text(children);
                        // `\link[pkg]{topic}` and `\link[pkg:dest]{topic}`
                        let package = tagged.option().map(plain_text).and_then(|opt| {
                            let opt = opt.trim().to_owned();
                            opt.split(':')
                                .next()
                                .map(str::to_owned)
                                .filter(|p| !p.is_empty())
                        });
                        out.push(Inline::Link {
                            dest: LinkDest::Topic {
                                package,
                                topic: topic.clone(),
                            },
                            children: vec![Inline::Code(topic)],
                        });
                    }
                    RdTag::Eqn => out.push(Inline::Math(latex_of(children))),
                    RdTag::Verb => out.push(Inline::Verb(plain_text(children))),
                    RdTag::Dots | RdTag::LDots => out.push(Inline::Code("...".to_owned())),
                    RdTag::Sspace => out.push(Inline::Text(" ".to_owned())),
                    RdTag::Enc => {
                        // `\enc{encoded}{ascii}`: prefer the encoded form.
                        let parts = groups(children);
                        if parts.is_empty() {
                            out.extend(inlines(children));
                        } else {
                            out.extend(inlines(&parts[0]));
                        }
                    }
                    // Unknown or structural: descend rather than drop.
                    _ => out.extend(inlines(children)),
                }
            }
            // `RdNode` is non-exhaustive: a future leaf kind is skipped.
            _ => {}
        }
    }
    out
}

/// `\eqn{latex}{ascii}` keeps the LaTeX form; the ASCII fallback is dropped.
fn latex_of(children: &[RdNode]) -> String {
    let parts = groups(children);
    if parts.is_empty() {
        plain_text(children).trim().to_owned()
    } else {
        plain_text(&parts[0]).trim().to_owned()
    }
}

/// Flatten nodes to their text content, dropping markup.
fn plain_text(nodes: &[RdNode]) -> String {
    let mut out = String::new();
    push_plain(nodes, &mut out);
    out
}

fn push_plain(nodes: &[RdNode], out: &mut String) {
    for node in nodes {
        match node {
            RdNode::Text(text) | RdNode::RCode(text) | RdNode::Verb(text) => out.push_str(text),
            RdNode::Comment(_) | RdNode::Raw(_) => {}
            RdNode::Group(group) => push_plain(group.children(), out),
            RdNode::Tagged(tagged) => push_plain(tagged.children(), out),
            // `RdNode` is non-exhaustive: a future leaf kind is skipped.
            _ => {}
        }
    }
}

/// Collapse Rd's source line wrapping into single spaces.
///
/// Rd is whitespace-insensitive within a paragraph, and Typst would otherwise
/// inherit the original hard wrapping. Leading and trailing whitespace
/// collapses to a single space rather than vanishing: inline text arrives
/// split across several nodes, and the space between `\code{f}` and the word
/// after it lives at exactly that boundary.
fn collapse(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let leading = value.starts_with(char::is_whitespace);
    let trailing = value.ends_with(char::is_whitespace);
    let core = value.split_whitespace().collect::<Vec<_>>().join(" ");

    if core.is_empty() {
        // Whitespace-only: one space, since something separated its neighbours.
        return " ".to_owned();
    }

    let mut out = String::with_capacity(core.len() + 2);
    if leading {
        out.push(' ');
    }
    out.push_str(&core);
    if trailing {
        out.push(' ');
    }
    out
}
