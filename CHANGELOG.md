# Changelog

All notable changes to typst-doc are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

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
