//! The reader-agnostic documentation model.
//!
//! One [`Topic`] is one documented entity: an Rd file, or a Python function,
//! class, or module. Readers fill in what their source expresses and leave the
//! rest empty; no field is required beyond `name`.

use super::block::Block;
use super::inline::Inline;

/// One documented parameter.
///
/// `names` is a list because Rd's `\item{x, y}{..}` and numpydoc's `x, y : int`
/// both document several parameters with one description.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Param {
    pub names: Vec<String>,
    /// A type annotation. Always `None` from Rd; usually `Some` from numpydoc.
    pub ty: Option<String>,
    pub default: Option<String>,
    pub optional: bool,
    pub body: Vec<Block>,
}

/// A worked example.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Example {
    pub code: String,
    /// Whether the source marked this as not to be run: `\dontrun{}`.
    pub run: bool,
}

/// A section with no dedicated field: `\section{}`, or an unrecognised
/// docstring heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: Vec<Inline>,
    pub body: Vec<Block>,
}

/// A single documented entity.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Topic {
    /// The primary identifier: `\name`, or the qualified Python name.
    pub name: String,
    pub title: Vec<Inline>,
    /// Alternative names this topic is reachable under: `\alias`.
    pub aliases: Vec<String>,
    pub keywords: Vec<String>,
    /// How the entity is called: `\usage`, or a Python signature.
    pub signature: Option<String>,
    pub description: Vec<Block>,
    pub details: Vec<Block>,
    /// `\arguments`, or the Parameters section.
    pub params: Vec<Param>,
    /// `\value`, or the Returns section.
    pub value: Vec<Block>,
    /// Exceptions the entity may raise. Empty from Rd.
    pub raises: Vec<Param>,
    pub examples: Vec<Example>,
    pub seealso: Vec<Block>,
    pub references: Vec<Block>,
    pub note: Vec<Block>,
    pub author: Vec<Block>,
    /// Sections in source order that have no dedicated field above.
    pub sections: Vec<Section>,
}

impl Topic {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }
}
