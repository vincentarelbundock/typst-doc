//! Render R and Python API documentation as Typst.
//!
//! The pipeline has three stages and one shared vocabulary:
//!
//! ```text
//! .Rd  --[r]------>  ir::Topic  --[typst]-->  Typst markup
//! .py  --[python]->
//! ```
//!
//! [`ir`] is the contract between them: it depends on no reader and no writer,
//! and every reader targets it. Adding a reader means adding a module that
//! produces [`ir::Topic`], and nothing in [`typst`] changes.

pub mod ir;
pub mod python;
pub mod r;
pub mod typst;

pub use ir::Topic;
pub use typst::{Options, ParamsFormat, topic_to_typst};
