# Changelog

All notable changes to typst-doc are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

## 0.0.2

### Added

- **Every generated document is now data plus a template.** An entry binds its
  content to a fixed set of `doc-` variables (`doc-title`, `doc-params`,
  `doc-sections`, and the rest, each defined for every topic and empty where
  the topic has nothing), and a template below renders them. The template is
  inlined, not imported, so an entry still compiles on its own.
- **`--template FILE` replaces the rendering half.** With `--output`,
  `template-default.typ` and `example-data.typ` are written beside the manual;
  concatenated, they are a working document to preview a template against.
  Passing `template-default.typ` back reproduces the default output exactly.

### Removed

- **`--params`.** Table-versus-terms is now a few lines in a template, which is
  where every other styling decision lives too. This was the whole point of the
  change: presentation options belong in Typst, not in the CLI.

## 0.0.1

First release. Reads documentation from four sources and writes Typst markup:
R documentation (`.Rd`), Python modules and packages, Typst source documented
with `///` comments, and Unix manual pages written in man(7) or mdoc(7).

- **Output shape follows the output target.** Without `--output`, every entry
  is joined into one document on standard output. With it, each entry becomes
  its own `<topic>.typ` in that directory, alongside an `index.typ` that
  outlines and includes them all.
- **Directories are read the way each format nests**, descending into
  subdirectories in name order and skipping dot-prefixed entries. Python
  topics are named under the packages above them, so `fit` in `mypkg/core.py`
  is `mypkg.core.fit`.
- **Cross-references resolve from where they are written.** A link, an alias,
  or an author-written `@name` becomes a real link to the entry it names,
  preferring the nearest definition when a name is shared. A target the run
  does not define is left alone and reported.
- **A Typst package is read through its entry point.** `typst.toml` names it,
  and it names the API: each definition is documented under the path a user
  imports it by (`image`, `layout.image`), and what it does not export is
  treated as internal.
- **Every generated document parses as Typst**, checked in CI over a fixture
  corpus by `cargo run --example validate`.
