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

#link("reference.html")[*API reference*]: the `typst-doc` manual page,
rendered by `typst-doc` itself.

= Install

Prebuilt binaries are attached to every
#link("https://github.com/vincentarelbundock/typst-doc/releases/latest")[release]:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/vincentarelbundock/typst-doc/releases/latest/download/typst-doc-installer.sh | sh
```

On Windows:

```sh
powershell -ExecutionPolicy Bypass -c "irm https://github.com/vincentarelbundock/typst-doc/releases/latest/download/typst-doc-installer.ps1 | iex"
```

Or build from source:

```sh
cargo install --git https://github.com/vincentarelbundock/typst-doc
```

Rendering the generated documents needs the
#link("https://github.com/typst/typst#installation")[Typst CLI] on your
`PATH`. Nothing else: `typst-doc` shells out to no other tool.

= Usage

Every input is a file or a directory, and every documented entity becomes one
manual entry. A directory contributes its recognised files in name order.

```sh
# One R help file, printed to standard output
typst-doc man/mean_ci.Rd

# A whole directory, joined into one document
typst-doc man/ > reference.typ

# Or one file per topic, plus an index.typ including them all
typst-doc R/ -o reference/
typst compile reference/index.typ
```

Cross-references resolve within a run: a link, or an author-written `@name` in
a Typst doc comment, becomes a real link to the entry it names. Where two
topics share a name the nearest wins, the same file before the same directory;
a name this run does not define, or one that stays ambiguous from where it was
written, renders as plain code and is reported. Internal topics are skipped
unless `--include-internal` is passed.

= R

`.Rd` files, the format `R CMD check` validates and `?help` displays. Parsing
is done by
#link("https://github.com/eitsupi/r-documentation-rs")[`rd-source` and `rd-ast`],
#link("https://github.com/eitsupi")[eitsupi]'s Rust crates, which tokenise an
`.Rd` file and hand back its macro tree.

```sh
typst-doc man/ -o reference/
```

Rd is a typed markup language, so the reader has the most to work with:
`\arguments`, `\value`, `\seealso`, `\examples`, and the rest map directly onto
manual sections. `\eqn`/`\deqn` are LaTeX, so they go through
#link("https://typst.app/universe/package/mitex")[MiTeX]. `\out{}` becomes
`html.elem`, guarded by `target()` so the same file still compiles to PDF. R
code becomes a highlighted raw block, never an executable one, and
`\keyword{internal}`, the signal pkgdown filters on, marks a topic internal.

= Python

Modules and packages, with
#link("https://numpydoc.readthedocs.io")[numpydoc]-style docstrings.

```sh
typst-doc mypkg/ -o reference/
```

A docstring is the signal that an entity is documentation-worthy: undocumented
definitions are not listed. Parameters, Returns, See Also, and Examples route
to the matching sections, long signatures are broken across lines, and names
starting with `_` are private, though dunders like `__init__` stay public.

A package is read the way Python imports it: every topic is named under the
packages above it, so `fit` in `mypkg/core.py` is `mypkg.core.fit`, and an
`__init__.py` documents its own package. How far the dotted name reaches is
decided by the highest `__init__.py` above the file, so a PEP 420 namespace
subpackage belongs to the path without one of its own, while a loose module in
a plain directory keeps its bare stem.

= Typst

Packages documented with `///` comments, in the convention established by the
#link("https://typst.app/universe/package/tidy")[tidy] package.

```sh
typst-doc src/lib.typ -o reference/
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

When the input is a package (a `typst.toml` at it or above it), the entry point
is read, and each definition is named by the path a user imports it under:
`image` when `lib.typ` re-exports it plainly, `layout.image` when behind a
module alias. A Typst module cannot bind two things to one name, so those names
are unique by construction, and what the entry point never mentions is not
public: unexported definitions are skipped as internal, or kept with
`--include-internal`.

Existing tidy comments parse unchanged. The dialect diverges in three
deliberate ways: type annotations are line-anchored to a final `->` line,
level-1 headings such as `= Value` and `= Details` route to the matching manual
section, and only documented definitions become entries. Defaults are sliced
from the source, so the author's formatting survives.

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
term list, and the section _titles_ route each part to the field it belongs in.
`NAME` gives the topic name, `SYNOPSIS` the signature, anything ending in
`OPTIONS` the parameters, `RETURN VALUE` and `EXIT STATUS` the value.
Unrecognised sections keep their own heading rather than being guessed at, and
`ls(1)` in `SEE ALSO` is a cross-reference, exactly like an Rd `\link`.

Both macro packages are read: presentational man(7), whose structure has to be
inferred, and mdoc(7), the BSD and macOS default, which names what each thing
is and is taken at its word. A `.so` stub is a redirect rather than a page, and
scanning skips one with a warning.

= Templates

Every generated document is in two halves: a data block, and a template that
renders it.

````typ
#let doc-title = [Confidence interval for a mean]
#let doc-signature = ```r
mean_ci(x, level = 0.95)
```
#let doc-params = (
  (names: ("x",), type: "numeric", default: none, optional: false,
   body: [A numeric vector.]),
)
#let doc-sections = (
  (id: "description", title: [Description], kind: "prose", body: [...]),
  (id: "arguments", title: [Arguments], kind: "params", items: doc-params),
)

// ... the template, rendering those bindings ...
````

The first half is what the entry _says_; the second is the only thing that
decides how it _looks_. So there are no styling options: to change how a manual
is typeset, replace the template.

```sh
typst-doc man/ -o reference/          # writes template-default.typ too
typst-doc man/ --template mine.typ -o reference/
```

The template is inlined rather than imported, so every entry compiles on its
own with no file beside it. Each one therefore carries its own copy, which is a
fair price for a directory of self-contained documents.

== The data

Every binding is defined by every entry, empty where the topic has nothing, so
a template that reads `doc-examples` cannot break on the one entry that has
none.

#table(
  columns: 3,
  stroke: none,
  align: (left, left, left),
  [*Binding*], [*Type*], [*Holds*],
  [`doc-name`], [`str`], [the topic's identifier],
  [`doc-label`], [`label` or `none`], [the heading's cross-reference target],
  [`doc-title`], [`content`], [the one-line title],
  [`doc-aliases`], [`array`], [strings: other names the topic answers to],
  [`doc-source`], [`str` or `none`], [the file, when a name is shared],
  [`doc-signature`], [`content` or `none`], [how the entity is called],
  [`doc-params`], [`array`],
  [dictionaries: `names`, `type`, `default`, `optional`, `body`],
  [`doc-raises`], [`array`], [dictionaries, shaped like `doc-params`],
  [`doc-examples`], [`array`], [dictionaries: `run`, `code`],
  [`doc-sections`], [`array`], [dictionaries: the whole entry, in order],
)

The code bindings are raw blocks, so a template that wants to rebuild one has
`doc-signature.lang` and `.text` to hand, and the same on each example's
`code`.

Write against `doc-sections`. It is the entry in order, each section tagged
with what it holds (`"prose"` with a `body`, or `"params"` or `"examples"` with
`items`), and sections the topic does not have are simply absent. One loop
renders everything, with no emptiness guards:

```typ
#for section in doc-sections {
  heading(level: 2, section.title)
  if section.kind == "prose" { section.body }
  else if section.kind == "params" { my-params(section.items) }
  else if section.kind == "examples" { my-examples(section.items) }
}
```

Because the order is data rather than control flow, reordering sections,
retitling them, or translating their headings is list manipulation, not an edit
to the loop.

== Writing one

`--output` writes two more files beside the manual, both generated, so neither
can drift from what the binary actually does:

- `template-default.typ`: the default template itself. Passing it back with
  `--template` reproduces the default output exactly.
- `example-data.typ`: one entry's data block, exercising every field.

Concatenated, the two are a working document, which is how a template is
previewed with no manual to hand:

```sh
cat reference/example-data.typ mytemplate.typ | typst compile - preview.pdf
```

Both are rewritten on every run, so copy `template-default.typ` under another
name before editing it. Names beginning with `doc-` are reserved for the data
contract; anything else a template defines is its own. A template is code, so a
typo in a binding name surfaces when Typst compiles the manual, not when
`typst-doc` writes it.

A generated entry imports exactly one thing, and only when it contains LaTeX
math: #link("https://typst.app/universe/package/mitex")[MiTeX], because Typst
math cannot read LaTeX. Nothing else is ever imported.

= Output validity

The writer emits strings, so `typst-syntax`, Typst's own parser, doubles as a
validator: every test asserts that generated markup parses with no errors. The
same check runs over a whole directory of `.Rd`, `.py`, `.typ`, or man page
files, and over the fixture corpus in CI on every push:

```sh
cargo run --example validate -- tests/corpus
```
