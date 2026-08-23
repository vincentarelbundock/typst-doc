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
    /// An unevaluated `\Sexpr{}`: R code that needs a live R session.
    ///
    /// Rendered visibly rather than dropped or silently unwrapped, so the
    /// reader can tell the difference between documented content and code
    /// that never ran.
    Sexpr(String),
    /// A branch that applies only to one output target: `\if`, `\ifelse`.
    Targeted {
        target: Target,
        then: Vec<Inline>,
        otherwise: Vec<Inline>,
    },
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
            Inline::Sexpr(code) => out.push_str(code),
            Inline::Targeted {
                then, otherwise, ..
            } => {
                // Plain text has no target, so the print branch wins.
                push_plain_text(
                    if otherwise.is_empty() {
                        then
                    } else {
                        otherwise
                    },
                    out,
                )
            }
        }
    }
}

/// Which output target a conditional branch applies to.
///
/// Rd conditionals name an output *format* (`html`, `latex`, `text`), but a
/// single Typst document compiles to both PDF and HTML. So rather than
/// resolving the condition at conversion time and discarding a branch, both
/// branches survive into the document and Typst's own `target()` picks between
/// them at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Applies when compiling to HTML.
    Html,
    /// Applies when compiling to anything else, PDF included.
    Print,
}

impl Target {
    /// The Typst condition that is true for this target.
    pub fn condition(self) -> &'static str {
        match self {
            Self::Html => "target() == \"html\"",
            Self::Print => "target() != \"html\"",
        }
    }
}
