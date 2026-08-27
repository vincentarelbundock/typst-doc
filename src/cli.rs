//! The command-line interface, defined once so that both the binary and the
//! man page generator (`cargo run --example mangen`) read from it.

use std::path::PathBuf;

use clap::Parser;

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
entry. A directory contributes every recognised file beneath it, descending \
into subdirectories, each in name order; entries whose name begins with a dot \
are skipped, so a .venv or .git in the tree is not mistaken for source.

Where the entries go is decided by the output target. With --output, each one \
is written to its own <topic>.typ file in that directory, alongside an \
index.typ that outlines and includes them all; without it, they are joined \
into a single document on standard output, in the order given.

A Typst package is read through its entry point: each definition is named by \
the path it is imported under, and what the entry point does not export is \
treated as internal. Elsewhere, two topics can share a name: the same \
function documented in two modules, say. Their files and heading labels then \
take the shortest part of the source \
path that tells them apart, each entry shows the file it came from, and a \
reference to the shared name is left as plain code, since it addresses no \
single entry.

Four input languages are recognised, by extension: R documentation (.Rd), \
Python modules and packages (.py), Typst source documented with /// comments \
(.typ), and Unix manual pages in either macro package, man(7) or mdoc(7) \
(a section number such as .1 or .3, or .man).

Each generated document is in two halves. The first binds the topic's content \
to a fixed set of doc- variables (doc-title, doc-params, doc-sections, and \
the rest), every one of them defined for every topic, empty where the topic \
has nothing. The second is the template, which renders them, and is the only \
half that decides how an entry looks. It is inlined rather than imported, so \
an entry compiles on its own with nothing beside it. Pass --template FILE to \
supply your own; with --output, the default is written to \
template-default.typ, and one entry's data to example-data.typ, as a place to \
start.

Each topic title is a level-1 heading. To nest the output under a title of \
your own, set the offset where you include it: `#set heading(offset: 1)`.

Cross-references resolve within a run, from where they are written: a link, or\
an author-written @name, becomes a real link to the entry it names, choosing\
the nearest definition when a name is shared: the same file first, then the\
same directory. A target this run does not define, or one still ambiguous from\
where it was written, renders as plain code, with a warning.

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

    /// A Typst file whose contents replace the default template: the half of
    /// each generated document that renders the data block above it. The
    /// default is written to `template-default.typ` alongside the manual
    /// whenever `--output` is given, as a starting point.
    #[arg(long, value_name = "FILE")]
    pub template: Option<PathBuf>,

    /// Include internal topics: `\keyword{internal}` in R (the signal
    /// pkgdown filters on), and `_`-prefixed names in Python. Skipped by
    /// default. Typst `_` definitions are always private.
    #[arg(long)]
    pub include_internal: bool,
}
