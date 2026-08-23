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
entry, and all entries are joined into a single document in the order given. A \
directory contributes its recognised files in name order.

Four input languages are recognised, by extension: R documentation (.Rd), \
Python modules and packages (.py), Typst source documented with /// comments \
(.typ), and Unix manual pages written in the man(7) macro package (a section \
number such as .1 or .3, or .man).

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

    /// Output file. Defaults to stdout.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// How to render the parameter list: table or terms.
    #[arg(long, value_enum, default_value_t = Params::Table, hide_possible_values = true)]
    pub params: Params,

    /// Heading level for the topic title.
    #[arg(long, default_value_t = 1)]
    pub base_level: u8,

    /// Include internal topics: `\keyword{internal}` in R (the signal
    /// pkgdown filters on), and `_`-prefixed names in Python. Skipped by
    /// default. Typst `_` definitions are always private.
    #[arg(long)]
    pub include_internal: bool,

    /// Write one `<topic>.typ` file per topic into the --output directory
    /// (created if missing) instead of joining everything into one document.
    /// Topics sharing a name are disambiguated by their source path, with a
    /// warning.
    #[arg(long, requires = "output")]
    pub split: bool,
}
