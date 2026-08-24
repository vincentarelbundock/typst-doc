//! The command-line interface, defined once so that both the binary and the
//! man page generator (`cargo run --example mangen`) read from it.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

use crate::typst::ParamsFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Params {
    Table,
    Terms,
}

impl From<Params> for ParamsFormat {
    fn from(value: Params) -> Self {
        match value {
            Params::Table => Self::Table,
            Params::Terms => Self::Terms,
        }
    }
}

/// `long_about` is written for the man page DESCRIPTION, so `-h` keeps the
/// one-line `about` and only `--help` in full shows it.
#[derive(Debug, Parser)]
#[command(
    name = "typst-doc",
    version,
    about = "Render R, Python, Typst, and man page documentation as Typst",
    long_about = "\
typst-doc reads documentation from source files and writes Typst markup. Each \
input is a file or a directory; every documented entity becomes one manual \
entry. A directory contributes its recognised files in name order.

Where the entries go is decided by the output target. With --output, each one \
is written to its own <topic>.typ file in that directory, alongside an \
index.typ that outlines and includes them all; without it, they are joined \
into a single document on standard output, in the order given.

Two topics can share a name — the same function documented in two modules, \
say. Their files and heading labels then take the shortest part of the source \
path that tells them apart, each entry shows the file it came from, and a \
reference to the shared name is left as plain code, since it addresses no \
single entry.

Four input languages are recognised, by extension: R documentation (.Rd), \
Python modules and packages (.py), Typst source documented with /// comments \
(.typ), and Unix manual pages written in the man(7) macro package (a section \
number such as .1 or .3, or .man).

Each topic title is a level-1 heading. To nest the output under a title of \
your own, set the offset where you include it: `#set heading(offset: 1)`.

Cross-references resolve within a run: a topic link whose target is converted \
in the same invocation becomes a real link to that entry's heading, and any \
other target renders as plain code, so every generated document compiles on \
its own.

Returns 0 on success. An input that cannot be read, or whose extension is not \
recognised, is an error; unrecognised files found while scanning a directory \
are skipped instead."
)]
pub struct Cli {
    /// Input `.Rd`, `.py`, `.typ`, or man page (`.1`, `.3`, `.man`) files,
    /// or directories of them. Topics are joined into one document, in the
    /// order given; a directory contributes its recognised files in name
    /// order.
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,

    /// Directory to write the manual into, created if missing: one
    /// `<topic>.typ` file per topic, plus an `index.typ` that outlines and
    /// includes them all. Without it, the whole manual goes to stdout as a
    /// single document.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// How to render the parameter list: table or terms.
    #[arg(long, value_enum, default_value_t = Params::Table, hide_possible_values = true)]
    pub params: Params,

    /// Include internal topics: `\keyword{internal}` in R (the signal
    /// pkgdown filters on), and `_`-prefixed names in Python. Skipped by
    /// default. Typst `_` definitions are always private.
    #[arg(long)]
    pub include_internal: bool,
}
