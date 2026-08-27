//! Block-level content.

use super::inline::Inline;

/// Column alignment in a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// One entry in a term list: `\describe{}` in Rd, a definition list elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    pub term: Vec<Inline>,
    pub body: Vec<Block>,
}

/// Block-level content.
///
/// The split from [`Inline`] is deliberate: it lets the writer decide line
/// breaking from the type alone, rather than re-deriving block-ness at render
/// time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading {
        /// 1-based, relative to the topic's own heading level.
        level: u8,
        content: Vec<Inline>,
    },
    /// A code listing. `lang` selects syntax highlighting.
    Code {
        lang: Option<String>,
        value: String,
    },
    List {
        ordered: bool,
        items: Vec<Vec<Block>>,
    },
    /// A term/definition list, rendered as Typst `#terms`.
    Terms(Vec<Term>),
    Table {
        align: Vec<Align>,
        /// Row-major; each cell is inline content.
        rows: Vec<Vec<Vec<Inline>>>,
    },
    /// Display math, as LaTeX source: `\deqn{}`.
    DisplayMath(String),
    /// Raw HTML from `\out{}`, re-expressed as `html.elem` where possible.
    Html(String),
    /// Several blocks with no wrapper of their own: the body of a construct
    /// that groups content without styling it.
    Group(Vec<Block>),
    /// A branch that applies only to one output target: `\if`, `\ifelse`,
    /// `#ifdef`.
    Targeted {
        target: super::inline::Target,
        then: Vec<Block>,
        otherwise: Vec<Block>,
    },
    /// Verbatim Typst, passed through untouched. The escape hatch of last
    /// resort: anything routed here is outside the writer's escaping
    /// guarantees, so readers should prefer a typed variant.
    Raw(String),
}
