#import "/.calepin/calepin.typ" as calepin
#calepin.setup(eval: false)

#set document(title: [typst-doc])
#metadata((
  summary: "typst-doc renders R, Python, Typst, and Unix manual-page documentation as Typst markup.",
  tags: ("overview", "getting started"),
)) <website-metadata>

// The page title is the hero, so `#title()` would only repeat it.
#let hero() = calepin.elements.target(
  html: () => html.elem("section", attrs: (
    class: "hero",
    style: "text-align: center; margin-block: 2.5rem 3rem;",
  ))[
    #html.elem("h1", attrs: (
      style: "font-family: var(--pico-font-family-monospace, monospace); "
        + "font-size: clamp(2.75rem, 9vw, 5rem); line-height: 1.05; "
        + "margin: 0;",
    ))[typst-doc]
    #html.elem("p", attrs: (
      style: "font-size: clamp(1.1rem, 3.2vw, 1.6rem); font-weight: 600; "
        + "margin: 0.75rem 0 0;",
    ))[Convert Man Pages to Typst]
    #html.elem("p", attrs: (
      style: "margin: 0.4rem 0 0; letter-spacing: 0.08em; "
        + "text-transform: uppercase; font-size: 0.85rem; opacity: 0.75;",
    ))[Python · R · Typst · CLI]
  ],
  paged: () => align(center)[
    #text(size: 3em, font: "DejaVu Sans Mono")[typst-doc]
    #text(size: 1.4em, weight: "bold")[\ Convert Man Pages to Typst]
    #text(size: 0.9em)[\ Python · R · Typst · CLI]
  ],
)

#hero()

Render R, Python, Typst, and Unix manual-page documentation as
#link("https://typst.app")[Typst]. One reader per input language, one writer,
one intermediate representation in between.

#link("reference.html")[*API reference*] — the `typst-doc` manual page,
rendered by `typst-doc` itself.

= Install

From #link("https://crates.io")[crates.io] once released, or straight from the
repository:

```sh
cargo install --git https://github.com/vincentarelbundock/typst-doc
```

From a local checkout:

```sh
git clone https://github.com/vincentarelbundock/typst-doc
cd typst-doc
make install
```

Rendering the generated documents needs the
#link("https://github.com/typst/typst#installation")[Typst CLI] on your
`PATH`. Nothing else: `typst-doc` shells out to no other tool.

= Usage

Every input is a file or a directory, and every documented entity becomes one
manual entry. Entries are joined into a single document in the order given; a
directory contributes its recognised files in name order.

```sh
# One R help file, printed to standard output
typst-doc man/mean_ci.Rd

# A Python module, with parameters as a term list instead of a table
typst-doc stats.py --params terms -o stats.typ

# A whole help directory, joined into one manual
typst-doc man/ -o reference.typ

# One file per topic, plus an index.typ that includes them all
typst-doc R/ --split -o reference/

# Then render it
typst compile reference.typ
```

Cross-references resolve within a run: a link whose target is converted in the
same invocation becomes a real link to that entry's heading, and any other
target renders as plain code, so every generated document compiles on its own.
Internal topics are skipped unless `--include-internal` is passed.

= R

`.Rd` files, the format `R CMD check` validates and `?help` displays.

```sh
typst-doc man/ -o reference.typ
```

Rd is a typed markup language, so the reader has the most to work with:
`\arguments`, `\value`, `\seealso`, `\examples`, and the rest map directly
onto manual sections. `\eqn`/`\deqn` are LaTeX, which Typst math does not
read, so they go through
#link("https://typst.app/universe/package/mitex")[MiTeX]; the import is
emitted only by documents that contain math. `\out{}` becomes `html.elem`,
guarded by `target()` so the same file still compiles to PDF. R code becomes a
highlighted raw block, never an executable one. `\keyword{internal}` — the
signal pkgdown filters on — marks a topic internal.

= Python

Modules and packages, with
#link("https://numpydoc.readthedocs.io")[numpydoc]-style docstrings.

```sh
typst-doc mypkg/ --params terms -o reference.typ
```

A docstring is the signal that an entity is documentation-worthy: undocumented
definitions are not listed. Parameters, Returns, See Also, and Examples route
to the matching sections. Names starting with `_` are private, though dunders
like `__init__` stay public. Long signatures are broken across lines.

= Typst

Packages documented with `///` comments, in the convention established by the
#link("https://typst.app/universe/package/tidy")[tidy] package.

```sh
typst-doc src/lib.typ --split -o reference/
```

````typ
/// Creates one logical slide command.
///
/// = Examples
///
/// ```typ
/// #mosaic.slide[Hello]
/// ```
///
/// -> content
#let slide(
  /// Which layout resolves this slide.
  /// -> auto | str | dictionary
  layout: auto,
  ..bodies
) = { }
````

Structure comes from `typst-syntax`, Typst's own parser, rather than regexes,
so defaults keep the author's formatting. Existing tidy comments parse
unchanged; the dialect diverges in three deliberate ways — type annotations
are line-anchored to a final `->` line, level-1 headings such as `= Value` and
`= Details` route to the matching manual section, and only documented
definitions become entries.

= Man pages

roff in the man(7) macro package: the `.TH`, `.SH`, `.TP`, `.B` vocabulary
every Linux page is written in. Inputs are selected by section number, or the
`.man` extension.

```sh
zcat /usr/share/man/man1/ls.1.gz > ls.1
typst-doc ls.1 -o ls.typ
```

A man page carries no machine-readable structure below the section level, so
the reader infers it: roff lines become blocks, a run of `.TP` items becomes a
term list, and the section _titles_ then route each part to the field it
belongs in — `NAME` to the topic name, `SYNOPSIS` to the signature, anything
ending in `OPTIONS` to the parameters, `RETURN VALUE` and `EXIT STATUS` to the
value. Unrecognised sections keep their own heading rather than being guessed
at. `ls(1)` in `SEE ALSO` is a cross-reference, exactly like an Rd `\link`.
`.so` stubs and mdoc(7) pages are not pages, and scanning skips them with a
warning.

= Output validity

The writer emits strings, so `typst-syntax` doubles as a validator. Every test
asserts that generated markup parses with no errors, and

```sh
cargo run --example validate -- tests/corpus
```

checks a whole directory of `.Rd`, `.py`, `.typ`, or man page files. CI runs
it on every push.
