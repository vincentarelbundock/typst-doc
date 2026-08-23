//! The intermediate representation every reader targets and the writer renders.
//!
//! Two layers, deliberately separated:
//!
//! - [`Topic`] is the *semantic* layer: which section a piece of prose belongs
//!   to. Rd and Python docstrings agree closely here.
//! - [`Block`] and [`Inline`] are the *prose* layer: the rich text inside a
//!   section. Rd is far more expressive here than a typical docstring, so this
//!   layer is shaped by Rd's vocabulary and readers under-populate it rather
//!   than the reverse.
//!
//! This module depends on no reader and no writer, and must stay that way.

pub mod block;
pub mod inline;
pub mod topic;

pub use block::{Align, Block, Term};
pub use inline::{Inline, LinkDest, Target, to_plain_text};
pub use topic::{Example, Param, Section, Topic};
