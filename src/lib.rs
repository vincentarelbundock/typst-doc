//! Render R, Python, Typst, and Unix manual-page documentation as Typst.
//!
//! The pipeline has three stages and one shared vocabulary:
//!
//! ```text
//! .Rd   --[r]------>  ir::Topic  --[typst]-->  Typst markup
//! .py   --[python]->
//! .typ  --[typ]---->
//! .1    --[man]---->
//! ```
//!
//! [`ir`] is the contract between them: it depends on no reader and no writer,
//! and every reader targets it. Adding a reader means adding a module that
//! produces [`ir::Topic`], and nothing in [`typst`] changes.

pub mod cli;
pub mod ir;
pub mod man;
pub mod python;
pub mod r;
pub mod typ;
pub mod typst;

pub use ir::Topic;
pub use typst::{Entry, Options, ParamsFormat, topic_to_typst};
