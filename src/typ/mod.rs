//! The Typst reader: `///` doc comments in `.typ` source to [`Topic`]s.
//!
//! The dialect is the one the `tidy` package established, with three
//! deliberate divergences documented in README.md: type annotations are
//! line-anchored, level-1 headings route to the matching manual section, and
//! only documented definitions become topics.
//!
//! Definition and parameter structure comes from the `typst-syntax` CST, not
//! from regexes: the CST is lossless, so `///` runs appear as `LineComment`
//! nodes that are siblings of the `let` binding or parameter they precede,
//! and a `let` inside a string literal is just a string. One wrinkle drives
//! the walker's shape: a run before the *first* statement of a code block
//! sits in the `CodeBlock` node, outside the inner `Code` node that holds the
//! binding. The pending run therefore flows *into* child containers instead
//! of being dropped at their boundary; any other non-trivia node ends it.
//!
//! The doc body is already Typst markup and must reach the output verbatim,
//! so prose becomes [`Block::Raw`] and titles [`Inline::Raw`] — never
//! `Paragraph`/`Text`, which would escape markup the author wrote
//! deliberately.

use typst_syntax::{LinkedNode, SyntaxKind};

pub mod package;

use crate::ir::{Block, Inline, Param, Section, Topic};

/// Parse Typst source and convert every documented `let` binding to a
/// [`Topic`], in source order.
///
/// Parsing is total: `typst-syntax` always produces a tree, and a file with
/// no documented definitions yields an empty Vec.
pub fn parse(source: &str) -> Vec<Topic> {
    let root = typst_syntax::parse(source);
    let mut topics = Vec::new();
    collect(
        &LinkedNode::new(&root),
        source,
        &mut Vec::new(),
        &mut topics,
    );
    topics
}

/// Walk the tree, attaching each `///` run to the binding it immediately
/// precedes.
fn collect(node: &LinkedNode, source: &str, pending: &mut Vec<String>, out: &mut Vec<Topic>) {
    for child in node.children() {
        match child.kind() {
            SyntaxKind::LineComment => match doc_line(child.leaf_text()) {
                Some(line) => pending.push(line.to_owned()),
                None => pending.clear(),
            },
            // A single newline keeps a run alive; a blank line ends it.
            // Markup mode spells the blank line `Parbreak`, code mode a
            // `Space` containing two newlines.
            SyntaxKind::Space => {
                if child.leaf_text().bytes().filter(|b| *b == b'\n').count() >= 2 {
                    pending.clear();
                }
            }
            SyntaxKind::Parbreak => pending.clear(),
            // The `#` of markup mode sits between a run and its binding.
            SyntaxKind::Hash => {}
            SyntaxKind::LetBinding => {
                let doc = std::mem::take(pending);
                if let Some(topic) = topic_of(&child, source, &doc) {
                    out.push(topic);
                }
                // Definitions nested in the body start runs of their own.
                collect(&child, source, &mut Vec::new(), out);
            }
            _ => {
                let mut inner = std::mem::take(pending);
                collect(&child, source, &mut inner, out);
            }
        }
    }
}

/// The content of a `///` line: the marker and at most one following space
/// stripped. `None` for an ordinary comment, which ends a doc run.
fn doc_line(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("///")?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

/// Convert one documented `let` binding to a topic.
fn topic_of(binding: &LinkedNode, source: &str, doc: &[String]) -> Option<Topic> {
    if doc.is_empty() {
        return None;
    }

    let closure = binding
        .children()
        .find(|child| child.kind() == SyntaxKind::Closure);
    // A closure without a name is a lambda bound like any other value, and a
    // destructuring pattern names no single entity; both fall out here.
    let name = ident_text(closure.as_ref().unwrap_or(binding))?;
    if name.starts_with('_') {
        return None;
    }

    let (types, body) = split_types(doc);
    let mut topic = Topic::new(name);
    topic.lang = Some("typ".to_owned());
    fill_body(&mut topic, body);

    match closure.as_ref().and_then(|closure| {
        closure
            .children()
            .find(|child| child.kind() == SyntaxKind::Params)
    }) {
        Some(params) => {
            topic.params = params_of(&params, source);
            topic.signature = Some(signature(&topic.name, &topic.params, &types));
        }
        // A variable binding: no signature to show, so the type annotation,
        // if any, leads the description as inline code instead.
        None => {
            if !types.is_empty() {
                topic
                    .description
                    .insert(0, Block::Raw(format!("`{}`", types.join(" | "))));
            }
        }
    }

    Some(topic)
}

/// The first identifier among a node's direct children.
fn ident_text(node: &LinkedNode) -> Option<String> {
    node.children()
        .find(|child| child.kind() == SyntaxKind::Ident)
        .map(|ident| ident.leaf_text().to_string())
}

/// Split the trailing type annotation off a doc run.
///
/// Divergence from tidy: only the *final non-empty line* can be the
/// annotation. tidy splits on the last `->` occurring anywhere in the block,
/// which silently truncates prose like "maps keys -> values".
fn split_types(lines: &[String]) -> (Vec<String>, &[String]) {
    let last = lines.iter().rposition(|line| !line.trim().is_empty());
    if let Some(index) = last
        && let Some(annotation) = lines[index].trim().strip_prefix("->")
    {
        let types = annotation
            .split('|')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect();
        return (types, &lines[..index]);
    }
    (Vec::new(), lines)
}

/// Partition a definition's doc body into title, description, and sections.
///
/// Level-1 headings outside code fences split the body; recognised titles
/// route to the matching topic field. Each region stays one [`Block::Raw`],
/// internal blank lines included — splitting into paragraphs would mean
/// re-parsing markup that must pass through untouched.
fn fill_body(topic: &mut Topic, lines: &[String]) {
    let mut regions: Vec<(Option<String>, Vec<String>)> = vec![(None, Vec::new())];
    let mut fence: Option<usize> = None;

    for line in lines {
        let ticks = leading_backticks(line.trim_start());
        match fence {
            Some(open) if ticks >= open => fence = None,
            None if ticks >= 3 => fence = Some(ticks),
            _ => {}
        }
        if fence.is_none()
            && ticks == 0
            && let Some(title) = line.strip_prefix("= ")
        {
            regions.push((Some(title.trim().to_owned()), Vec::new()));
            continue;
        }
        regions
            .last_mut()
            .expect("seeded above")
            .1
            .push(line.clone());
    }

    for (title, body) in regions {
        match title {
            None => {
                // The leading region: first paragraph is the title, joined to
                // one line because it lands inside a Typst heading.
                let blank = body.iter().position(|line| line.trim().is_empty());
                let (head, rest) = body.split_at(blank.unwrap_or(body.len()));
                let head = head.join(" ").trim().to_owned();
                if !head.is_empty() {
                    topic.title = vec![Inline::Raw(head)];
                }
                topic.description.extend(raw_block(rest));
            }
            Some(title) => {
                let body = raw_block(&body);
                match title.to_lowercase().as_str() {
                    "value" | "returns" | "return" => topic.value.extend(body),
                    "details" => topic.details.extend(body),
                    "note" => topic.note.extend(body),
                    "see also" | "seealso" => topic.seealso.extend(body),
                    "references" => topic.references.extend(body),
                    "author" => topic.author.extend(body),
                    // `Examples` included: a Typst doc body interleaves prose
                    // and fenced code, which `Topic::examples` (bare runnable
                    // code) cannot carry. A custom section renders the same.
                    _ => topic.sections.push(Section {
                        title: vec![Inline::Raw(title)],
                        body,
                    }),
                }
            }
        }
    }
}

/// The length of the backtick run opening a line, if any.
fn leading_backticks(line: &str) -> usize {
    line.bytes().take_while(|b| *b == b'`').count()
}

/// A region as a single verbatim block, outer blank lines trimmed and inner
/// ones kept.
fn raw_block(lines: &[String]) -> Vec<Block> {
    let first = lines.iter().position(|line| !line.trim().is_empty());
    let Some(first) = first else {
        return Vec::new();
    };
    let last = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .expect("a non-blank line exists");
    vec![Block::Raw(lines[first..=last].join("\n"))]
}

/// Convert a closure's parameter list, documented or not, in source order.
fn params_of(params: &LinkedNode, source: &str) -> Vec<Param> {
    let mut out = Vec::new();
    let mut pending: Vec<String> = Vec::new();

    for child in params.children() {
        match child.kind() {
            SyntaxKind::LineComment => match doc_line(child.leaf_text()) {
                Some(line) => pending.push(line.to_owned()),
                None => pending.clear(),
            },
            SyntaxKind::Space => {
                if child.leaf_text().bytes().filter(|b| *b == b'\n').count() >= 2 {
                    pending.clear();
                }
            }
            SyntaxKind::Ident => out.push(param(
                child.leaf_text().to_string(),
                None,
                std::mem::take(&mut pending),
            )),
            SyntaxKind::Named => {
                let Some(name) = ident_text(&child) else {
                    continue;
                };
                out.push(param(
                    name,
                    default_of(&child, source),
                    std::mem::take(&mut pending),
                ));
            }
            SyntaxKind::Spread => {
                let name = ident_text(&child).unwrap_or_default();
                out.push(param(
                    format!("..{name}"),
                    None,
                    std::mem::take(&mut pending),
                ));
            }
            SyntaxKind::Comma | SyntaxKind::LeftParen | SyntaxKind::RightParen => {}
            // A placeholder or destructuring pattern binds no documentable
            // name.
            _ => pending.clear(),
        }
    }

    out
}

fn param(name: String, default: Option<String>, doc: Vec<String>) -> Param {
    let (types, body) = split_types(&doc);
    Param {
        names: vec![name],
        ty: (!types.is_empty()).then(|| types.join(" | ")),
        default,
        optional: false,
        body: raw_block(body),
    }
}

/// The default value of a `Named` parameter: the expression after the colon,
/// sliced from the original source so the author's formatting survives.
fn default_of(named: &LinkedNode, source: &str) -> Option<String> {
    let expression = named
        .children()
        .skip_while(|child| child.kind() != SyntaxKind::Colon)
        .find(|child| {
            !matches!(
                child.kind(),
                SyntaxKind::Colon | SyntaxKind::Space | SyntaxKind::LineComment
            )
        })?;
    source.get(expression.range()).map(str::to_owned)
}

/// Reconstruct the signature; the raw source span is unusable because it
/// interleaves the `///` blocks. Parameter types stay out — they render in
/// the Arguments table — but the return type is part of how the function is
/// called.
fn signature(name: &str, params: &[Param], types: &[String]) -> String {
    let parts: Vec<String> = params
        .iter()
        .map(|param| match &param.default {
            Some(default) => format!("{}: {}", param.names[0], default),
            None => param.names[0].clone(),
        })
        .collect();
    let suffix = if types.is_empty() {
        String::new()
    } else {
        format!(" -> {}", types.join(" | "))
    };

    let one_line = format!("{name}({}){suffix}", parts.join(", "));
    if one_line.len() <= 78 && params.len() <= 3 {
        return one_line;
    }
    // Long or many-parameter signatures break like a multi-line R usage
    // block: one parameter per line, two-space indent.
    format!("{name}(\n  {}\n){suffix}", parts.join(",\n  "))
}
