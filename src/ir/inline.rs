//! Inline (span-level) content.

/// Where a link points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkDest {
    /// An absolute URL.
    Url(String),
    /// An email address, without the `mailto:` scheme.
    Email(String),
    /// A DOI, without the resolver prefix.
    Doi(String),
    /// Another documentation topic, optionally in another package or module.
    Topic {
        package: Option<String>,
        topic: String,
    },
}

/// Span-level content. Readers populate what their source can express; the
/// writer renders every variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    /// Literal text. Escaping is the writer's job, never the reader's.
    Text(String),
    /// Inline code: `\code{}`, or a docstring's ``literal``.
    Code(String),
    /// Verbatim text that must not be reflowed.
    Verb(String),
    Emph(Vec<Inline>),
    Strong(Vec<Inline>),
    Link {
        dest: LinkDest,
        /// Display text. Empty means "render the destination itself".
        children: Vec<Inline>,
    },
    /// Inline math, as LaTeX source: `\eqn{}`.
    Math(String),
    /// An explicit line break within a paragraph.
    LineBreak,
}

impl Inline {
    /// Convenience for the common case of a plain-text run.
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }
}

/// Flatten inline content to plain text, dropping markup.
///
/// Used where a target admits no markup at all, such as a `#table` column
/// specifier or a label.
pub fn to_plain_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    push_plain_text(inlines, &mut out);
    out
}

fn push_plain_text(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(value) | Inline::Code(value) | Inline::Verb(value) => out.push_str(value),
            Inline::Math(value) => out.push_str(value),
            Inline::Emph(children) | Inline::Strong(children) => push_plain_text(children, out),
            Inline::Link { dest, children } => {
                if children.is_empty() {
                    match dest {
                        LinkDest::Url(value) | LinkDest::Email(value) | LinkDest::Doi(value) => {
                            out.push_str(value)
                        }
                        LinkDest::Topic { topic, .. } => out.push_str(topic),
                    }
                } else {
                    push_plain_text(children, out);
                }
            }
            Inline::LineBreak => out.push(' '),
        }
    }
}
