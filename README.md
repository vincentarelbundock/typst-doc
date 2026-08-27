# typst-doc

Render R, Python, Typst, and Unix manual-page documentation as
[Typst](https://typst.app).

Documentation: <https://vincentarelbundock.github.io/typst-doc>

```console
$ typst-doc man/mean_ci.Rd
$ typst-doc stats.py > stats.typ
$ typst-doc src/slides.typ > manual.typ
$ typst-doc /usr/share/man/man1/ls.1
$ typst-doc man/ -o reference/
```

- Four input languages: R `.Rd`, Python numpydoc docstrings,
  Typst `///` doc comments (tidy dialect), and man(7)/mdoc(7) pages.
- One entry per documented entity, from a file or a whole directory tree.
- Cross-references resolve within a run, from where they are written.
- Typst packages are named by the path a user actually imports them under.
- Output is a single document on stdout, or one `.typ` per topic plus an
  `index.typ` that builds the whole reference manual.
- Each entry is a data block plus a template, imports nothing, and compiles on
  its own. Restyle a manual with `--template`, not with styling flags.

## Install

Prebuilt binaries for the latest release, via shell script:

```console
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/vincentarelbundock/typst-doc/releases/latest/download/typst-doc-installer.sh | sh
```

Or via PowerShell:

```console
powershell -ExecutionPolicy Bypass -c "irm https://github.com/vincentarelbundock/typst-doc/releases/latest/download/typst-doc-installer.ps1 | iex"
```

Binaries for every platform are attached to each
[release](https://github.com/vincentarelbundock/typst-doc/releases/latest).

Failing that, build from source:

```console
cargo install --git https://github.com/vincentarelbundock/typst-doc
```

## Credits

Derived in part from [rd2qmd](https://github.com/eitsupi/rd2qmd) (MIT). See
[NOTICE.md](NOTICE.md). MIT licensed; see [LICENSE.md](LICENSE.md).
