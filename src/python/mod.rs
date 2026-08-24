//! The Python reader: `.py` source to [`Topic`]s.
//!
//! Two layers, matching the two crates involved: `rustpython-parser` locates
//! documented entities and their signatures, and `pydocstring` parses each
//! docstring's section structure (Google and NumPy styles).
//!
//! Docstring prose is plain text, so this reader populates the [`Block`] layer
//! far more sparsely than the R reader does. That asymmetry is expected.

use pydocstring::model::{Block as PyBlock, Docstring, SectionKind};
use rustpython_parser::ast::Ranged;
use rustpython_parser::ast::{self, Stmt};
use rustpython_parser::{Parse, ParseError};

use crate::ir::{Block, Example, Inline, Param, Section, Topic};

/// Parse Python source and convert every documented entity to a [`Topic`].
///
/// Module, functions, and classes each become one topic, in source order.
pub fn parse(source: &str, path: &str) -> Result<Vec<Topic>, ParseError> {
    let suite = ast::Suite::parse(source, path)?;
    let mut topics = Vec::new();

    let module_name = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".py")
        .to_owned();

    if let Some(doc) = leading_docstring(&suite) {
        topics.push(from_docstring(&module_name, None, &doc));
    }

    collect(&suite, &module_name, source, &mut topics);
    Ok(topics)
}

fn collect(body: &[Stmt], prefix: &str, source: &str, out: &mut Vec<Topic>) {
    for stmt in body {
        match stmt {
            Stmt::FunctionDef(def) => {
                let name = qualify(prefix, def.name.as_str());
                if let Some(doc) = leading_docstring(&def.body) {
                    out.push(from_docstring(
                        &name,
                        Some(signature(def.name.as_str(), &def.args, source)),
                        &doc,
                    ));
                }
            }
            Stmt::AsyncFunctionDef(def) => {
                let name = qualify(prefix, def.name.as_str());
                if let Some(doc) = leading_docstring(&def.body) {
                    out.push(from_docstring(
                        &name,
                        Some(format!(
                            "async {}",
                            signature(def.name.as_str(), &def.args, source)
                        )),
                        &doc,
                    ));
                }
            }
            Stmt::ClassDef(def) => {
                let name = qualify(prefix, def.name.as_str());
                if let Some(doc) = leading_docstring(&def.body) {
                    out.push(from_docstring(&name, None, &doc));
                }
                // Methods are documented as topics of their own.
                collect(&def.body, &name, source, out);
            }
            _ => {}
        }
    }
}

fn qualify(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}.{name}")
    }
}

/// The docstring of a module, function, or class: a bare string expression in
/// first statement position.
fn leading_docstring(body: &[Stmt]) -> Option<String> {
    let Stmt::Expr(expr) = body.first()? else {
        return None;
    };
    let ast::Expr::Constant(constant) = expr.value.as_ref() else {
        return None;
    };
    match &constant.value {
        ast::Constant::Str(value) => Some(value.clone()),
        _ => None,
    }
}

/// Render a `def` line from the parsed argument list.
fn signature(name: &str, args: &ast::Arguments, source: &str) -> String {
    let defaults = defaults_of(args, source);
    let mut parts: Vec<String> = Vec::new();

    for arg in &args.posonlyargs {
        parts.push(render_arg(&arg.def, source, &defaults));
    }
    if !args.posonlyargs.is_empty() {
        parts.push("/".to_owned());
    }
    for arg in &args.args {
        parts.push(render_arg(&arg.def, source, &defaults));
    }
    if let Some(vararg) = &args.vararg {
        parts.push(format!("*{}", render_arg(vararg, source, &defaults)));
    } else if !args.kwonlyargs.is_empty() {
        parts.push("*".to_owned());
    }
    for arg in &args.kwonlyargs {
        parts.push(render_arg(&arg.def, source, &defaults));
    }
    if let Some(kwarg) = &args.kwarg {
        parts.push(format!("**{}", render_arg(kwarg, source, &defaults)));
    }

    let one_line = format!("def {name}({})", parts.join(", "));
    // The `/` and `*` separators are not parameters of their own.
    let arity = parts
        .iter()
        .filter(|part| part.as_str() != "*" && part.as_str() != "/")
        .count();
    if one_line.len() <= 78 && arity <= 3 {
        return one_line;
    }
    // Long or many-parameter signatures break like the R and Typst readers':
    // one parameter per line, two-space indent.
    format!("def {name}(\n  {}\n)", parts.join(",\n  "))
}

/// Render one argument, annotation included.
///
/// Annotations are recovered by slicing the original source over the node's
/// `TextRange` rather than re-rendering the expression tree. That keeps the
/// author's own formatting for arbitrary annotations (`list[int]`,
/// `Callable[..., T] | None`) and costs no dependency; an unparser would
/// normalise them into something the author never wrote.
fn render_arg(arg: &ast::Arg, source: &str, defaults: &DefaultMap) -> String {
    let mut out = arg.arg.as_str().to_owned();

    if let Some(annotation) = &arg.annotation
        && let Some(text) = slice(source, annotation.range())
    {
        out.push_str(": ");
        out.push_str(text);
    }

    if let Some(default) = defaults.get(arg.arg.as_str()) {
        out.push_str(if arg.annotation.is_some() { " = " } else { "=" });
        out.push_str(default);
    }

    out
}

/// Argument name to its rendered default value.
type DefaultMap = std::collections::HashMap<String, String>;

/// Collect the source text of every default expression, by argument name.
fn defaults_of(args: &ast::Arguments, source: &str) -> DefaultMap {
    let mut map = DefaultMap::new();

    let positional = args.posonlyargs.iter().chain(args.args.iter());
    for arg in positional {
        if let Some(default) = &arg.default
            && let Some(text) = slice(source, default.range())
        {
            map.insert(arg.def.arg.as_str().to_owned(), text.to_owned());
        }
    }
    for arg in &args.kwonlyargs {
        if let Some(default) = &arg.default
            && let Some(text) = slice(source, default.range())
        {
            map.insert(arg.def.arg.as_str().to_owned(), text.to_owned());
        }
    }

    map
}

/// Slice the original source over a node's range.
///
/// Ranges are byte offsets into the same source that was parsed, so a failed
/// slice means a bug rather than bad input; it degrades to omitting the
/// annotation rather than panicking.
fn slice(source: &str, range: rustpython_parser::text_size::TextRange) -> Option<&str> {
    source.get(usize::from(range.start())..usize::from(range.end()))
}

/// Map a parsed docstring onto the topic model.
fn from_docstring(name: &str, signature: Option<String>, source: &str) -> Topic {
    let parsed: Docstring = pydocstring::parse::parse(source).to_model();
    let mut topic = Topic::new(name);
    topic.signature = signature;
    topic.lang = Some("python".to_owned());

    if let Some(summary) = &parsed.summary {
        topic.title = inlines(&unwrap_lines(summary.trim()));
    }
    if let Some(extended) = &parsed.extended_summary {
        topic.description = prose(extended);
    }

    for section in &parsed.sections {
        match &section.kind {
            SectionKind::Parameters
            | SectionKind::KeywordParameters
            | SectionKind::OtherParameters
            | SectionKind::Receives => topic.params.extend(params(&section.blocks)),
            SectionKind::Returns | SectionKind::Yields => {
                topic.value.extend(returns(&section.blocks))
            }
            SectionKind::Raises | SectionKind::Warns => {
                topic.raises.extend(exceptions(&section.blocks))
            }
            SectionKind::SeeAlso => topic.seealso.extend(free_blocks(&section.blocks)),
            SectionKind::References => topic.references.extend(free_blocks(&section.blocks)),
            SectionKind::Attributes | SectionKind::Methods => topic.sections.push(Section {
                title: vec![Inline::text(match section.kind {
                    SectionKind::Attributes => "Attributes",
                    _ => "Methods",
                })],
                body: free_blocks(&section.blocks),
            }),
            SectionKind::FreeText(kind) => {
                let title = format!("{kind:?}");
                if title.eq_ignore_ascii_case("examples") {
                    for block in &section.blocks {
                        if let PyBlock::Paragraph(text) = block {
                            topic.examples.push(Example {
                                code: strip_prompts(text.as_str()),
                                run: true,
                            });
                        }
                    }
                } else {
                    topic.sections.push(Section {
                        title: vec![Inline::text(title)],
                        body: free_blocks(&section.blocks),
                    });
                }
            }
            _ => topic.sections.push(Section {
                title: vec![Inline::text(format!("{:?}", section.kind))],
                body: free_blocks(&section.blocks),
            }),
        }
    }

    topic
}

fn params(blocks: &[PyBlock]) -> Vec<Param> {
    blocks
        .iter()
        .filter_map(|block| match block {
            PyBlock::Parameter(parameter) => Some(Param {
                names: parameter.names.clone(),
                ty: parameter.type_annotation.clone(),
                default: parameter.default_value.clone(),
                optional: parameter.is_optional,
                body: parameter
                    .description
                    .as_deref()
                    .map(prose)
                    .unwrap_or_default(),
            }),
            _ => None,
        })
        .collect()
}

fn returns(blocks: &[PyBlock]) -> Vec<Block> {
    let mut out = Vec::new();
    for block in blocks {
        match block {
            PyBlock::Return(value) => {
                let mut parts = Vec::new();
                if let Some(ty) = &value.type_annotation {
                    parts.push(Inline::Code(ty.clone()));
                }
                if let Some(description) = &value.description {
                    if !parts.is_empty() {
                        parts.push(Inline::text(" — "));
                    }
                    parts.extend(inlines(&unwrap_lines(description.trim())));
                }
                if !parts.is_empty() {
                    out.push(Block::Paragraph(parts));
                }
            }
            PyBlock::Paragraph(text) => out.extend(prose(text)),
            _ => {}
        }
    }
    out
}

fn exceptions(blocks: &[PyBlock]) -> Vec<Param> {
    blocks
        .iter()
        .filter_map(|block| match block {
            PyBlock::Exception(entry) => Some(Param {
                names: vec![entry.type_name.clone()],
                ty: None,
                default: None,
                optional: false,
                body: entry.description.as_deref().map(prose).unwrap_or_default(),
            }),
            _ => None,
        })
        .collect()
}

fn free_blocks(blocks: &[PyBlock]) -> Vec<Block> {
    let mut out = Vec::new();
    for block in blocks {
        match block {
            PyBlock::Paragraph(text) => out.extend(prose(text)),
            PyBlock::SeeAlso(entry) => {
                out.push(Block::Paragraph(vec![Inline::Code(entry.names.join(", "))]))
            }
            PyBlock::Reference(reference) => out.push(Block::Paragraph(vec![Inline::text(
                format!("{reference:?}"),
            )])),
            PyBlock::Attribute(attribute) => out.push(Block::Paragraph(vec![Inline::Code(
                attribute.names.join(", "),
            )])),
            PyBlock::Method(method) => {
                out.push(Block::Paragraph(vec![Inline::Code(method.name.clone())]))
            }
            _ => {}
        }
    }
    out
}

/// Split plain docstring prose into paragraphs on blank lines.
fn prose(text: &str) -> Vec<Block> {
    text.split("\n\n")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| Block::Paragraph(inlines(&unwrap_lines(part))))
        .collect()
}

/// Split docstring prose into inline runs, recognising the code markup that
/// Python projects write by hand.
///
/// `pydocstring` reports prose verbatim, so a docstring's inline markup arrives
/// here as literal punctuation and would otherwise be escaped into the output.
/// Three spellings are common enough to be worth reading, and all three mean
/// the same thing to a reader: a piece of code.
///
/// - `` ``literal`` `` — reStructuredText inline literal.
/// - ``:role:`target` `` — an interpreted-text role such as `:func:`; the role
///   is dropped and its target kept, since Typst has nothing to link it to.
/// - `` `code` `` — a single-backtick span, Markdown's code and reST's default
///   role. Both intend code, so both render as code.
///
/// Anything unterminated is left as text rather than swallowing the rest of the
/// paragraph.
fn inlines(text: &str) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    let mut pending = String::new();
    let mut rest = text;

    let flush = |pending: &mut String, out: &mut Vec<Inline>| {
        if !pending.is_empty() {
            out.push(Inline::text(std::mem::take(pending)));
        }
    };

    while let Some(at) = rest.find(['`', ':']) {
        let (before, from) = rest.split_at(at);
        pending.push_str(before);

        // `:role:`target`` — only when a backquote actually follows the role.
        if let Some(after_colon) = from.strip_prefix(':') {
            match role_target(after_colon) {
                Some((target, tail)) => {
                    flush(&mut pending, &mut out);
                    out.push(Inline::Code(target.to_owned()));
                    rest = tail;
                }
                None => {
                    pending.push(':');
                    rest = after_colon;
                }
            }
            continue;
        }

        let (fence, body) = match from.strip_prefix("``") {
            Some(body) => ("``", body),
            None => ("`", &from[1..]),
        };
        match body.find(fence) {
            Some(end) => {
                flush(&mut pending, &mut out);
                out.push(Inline::Code(body[..end].trim().to_owned()));
                rest = &body[end + fence.len()..];
            }
            // Unterminated: keep the backticks as ordinary text.
            None => {
                pending.push_str(fence);
                rest = body;
            }
        }
    }

    pending.push_str(rest);
    flush(&mut pending, &mut out);
    out
}

/// Match `role:`target`` at the start of `text`, after the leading colon.
///
/// Returns the target and the remainder of the input.
fn role_target(text: &str) -> Option<(&str, &str)> {
    let (role, after) = text.split_once(':')?;
    if role.is_empty()
        || !role
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '+' || c == '_' || c == '.')
    {
        return None;
    }
    let body = after.strip_prefix('`')?;
    let end = body.find('`')?;
    Some((body[..end].trim(), &body[end + 1..]))
}

fn unwrap_lines(text: &str) -> String {
    text.lines().map(str::trim).collect::<Vec<_>>().join(" ")
}

/// Strip `>>> ` and `... ` doctest prompts, keeping the code and dropping
/// expected output.
fn strip_prompts(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if !lines
        .iter()
        .any(|line| line.trim_start().starts_with(">>>"))
    {
        return text.trim().to_owned();
    }
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            trimmed
                .strip_prefix(">>> ")
                .or_else(|| trimmed.strip_prefix("... "))
                .or_else(|| trimmed.strip_prefix(">>>").filter(|r| r.is_empty()))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}
