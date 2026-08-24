# typst-doc

Render R, Python, Typst, and Unix manual-page documentation as
[Typst](https://typst.app).

```console
$ typst-doc man/mean_ci.Rd
$ typst-doc stats.py --params terms > stats.typ
$ typst-doc src/slides.typ > manual.typ
$ typst-doc /usr/share/man/man1/ls.1
$ typst-doc man/ -o reference/
```

Each input is a file or a directory; every documented entity becomes one
manual entry (a directory contributes every recognised file beneath it,
descending into subdirectories, each in name order; dot-prefixed entries such
as `.venv` are skipped).
Where the entries go is decided by the output target. With `--output`, each
one is written to its own `<topic>.typ` file in that directory, along with an
`index.typ` whose table of contents `#include`s every entry, so `typst compile
index.typ` builds the whole reference manual. Without it, the entries are
joined into a single document on standard output, in the order given.

Cross-references resolve within a run, from where they are written: a link, or
an author-written `@name` in a Typst doc comment, becomes a real link to the
entry it names. Where two topics share a name, the nearest definition wins —
the same file first, then the same directory — so a package with an `image` in
both `component/` and `layout/` still links correctly from either side. A
target this run does not define is left alone, since the document including
these entries may define it, and reported as a warning; one still ambiguous
from where it was written renders as plain code. Internal
topics are skipped unless `--include-internal` is passed: `\keyword{internal}`
in R (the signal pkgdown filters on), and `_`-prefixed names in Python
(dunders like `__init__` stay public). Typst `_` definitions are always
private. Man pages have no such signal, and `_exit(2)` is public API.

## Design

Three stages and one shared vocabulary:

```text
.Rd   --[r]------>  ir::Topic  --[typst]-->  Typst markup
.py   --[python]->
.typ  --[typ]---->
.1    --[man]---->
```

`ir` is the contract. It depends on no reader and no writer, and every reader
targets it. Adding a reader means adding a module that produces an `ir::Topic`;
nothing in the writer changes.

The IR has two layers, because documentation formats agree far more about
*structure* than about *prose*:

- `ir::Topic` is semantic — which section a piece of prose belongs to. Rd's
  `\arguments`/`\value`/`\seealso` and numpydoc's Parameters/Returns/See Also
  are the same document, differently spelled.
- `ir::Block` and `ir::Inline` are the rich text inside a section. Rd is a
  typed markup language and is much more expressive here than a docstring, so
  this layer is shaped by Rd's vocabulary. The Python reader under-populates it
  rather than the IR being narrowed to the intersection.

The man reader is the one that has to *infer* the semantic layer rather than
read it: roff says "hanging indent", never "parameter". That inference lives
in the reader, behind the same `ir::Topic` the others produce, so the writer
cannot tell the difference.

Everything is one crate. The module boundaries carry the design; crate
boundaries would only add versioning and publishing overhead with a single
consumer.

## Typst-specific mappings

Three constructs have no direct Typst equivalent:

- **Equations.** Rd equations are LaTeX, which Typst math does not read, so
  `\eqn`/`\deqn` go through [MiTeX](https://typst.app/universe/package/mitex).
  The import is emitted only by documents that contain math.
- **Raw HTML.** `\out{}` is re-expressed with `html.elem`, guarded by
  `target()` so the same file still compiles to PDF.
- **Code.** R code becomes a plain highlighted raw block, never an executable
  one.

## Typst doc comments

The third input language is Typst itself: packages documented with `///`
comments in the convention established by the
[tidy](https://typst.app/universe/package/tidy) package. The reader is
specified in [SPEC-typ-reader.md](SPEC-typ-reader.md); this section defines
the dialect it accepts, which is tidy's with three deliberate divergences.
Existing tidy-style comments (e.g. mosaic's) parse unchanged.

A doc block is a run of consecutive `///` lines. Placed immediately above a
`#let`, it documents that definition; placed inside a closure's parameter
list, immediately above a parameter, it documents that parameter — the
adjacency is the point, since parameter names are never repeated in prose and
so can never drift out of sync. The body of a block is ordinary Typst markup
and passes through to the output verbatim, never escaped.

```typ
/// Creates one logical slide command.
///
/// A logical slide is one unit of content.
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
) = { ... }
```

The divergences from tidy:

1. **Type annotations are line-anchored.** If the final non-empty line of a
   block starts with `->`, it is the type annotation (`|`-separated). A `->`
   anywhere else is prose. tidy splits on the last `->` occurring *anywhere*,
   so a description like "maps keys -> values" silently loses its tail into a
   type; this dialect does not reproduce that.
2. **Level-1 headings are semantic sections.** `= Value` (or `= Returns`),
   `= Details`, `= Note`, `= See also`, `= References`, and `= Author` in a
   definition's doc body route their content to the matching manual section,
   so a Typst function renders with the same structure as an R or Python one.
   Unrecognised headings become custom sections; content before the first
   heading is the title (first paragraph) and description. Under tidy these
   headings simply render as headings, so the extension degrades gracefully.
3. **Only documented definitions become manual entries.** A `///` block is
   the signal that an entity is documentation-worthy, mirroring the Python
   reader's docstring rule; tidy also lists undocumented public functions.
   Names starting with `_` are always private.

Definition and parameter structure — names, defaults, the argument list —
comes from parsing the source with `typst-syntax`, Typst's own parser, rather
than the regexes tidy uses; defaults are sliced from the original source so
the author's formatting survives. `@name` cross-references pass through
verbatim and resolve when the target is rendered in the same document, since
every entry's heading carries a `<name>` label.

## Unix manual pages

The fourth input language is roff, in the man(7) macro package: the `.TH`,
`.SH`, `.TP`, `.B` vocabulary every Linux page is written in, plus the
low-level escapes those pages use for fonts (`\fB`), special characters
(`\(bu`), and comments (`\"`). Input files are selected by section number —
`ls.1`, `printf.3`, `sshd_config.5` — or the `.man` extension. Pages are
often installed compressed; decompress first (`zcat ls.1.gz > ls.1`).

A man page carries no machine-readable structure below the section level, so
the reader works in two passes. The first is syntactic: roff lines become
blocks, and a run of `.TP` items becomes a term list. The second reads the
section *titles* and routes each to the field it belongs in:

| Section | Field |
| --- | --- |
| `NAME` | topic name, aliases, and title |
| `SYNOPSIS`, `SYNTAX`, `USAGE` | signature |
| `DESCRIPTION`, `OVERVIEW` | description |
| anything ending in `OPTIONS` | parameters |
| `RETURN VALUE`, `EXIT STATUS` | value |
| `EXAMPLES` | examples, when the section is code alone |
| `SEE ALSO` | see also |
| `NOTES`, `AUTHORS`, `REFERENCES`, `STANDARDS` | the matching section |

Everything else keeps its own heading, in source order. The routing is
deliberately loose: these titles are a convention, not a vocabulary, and a
section that is not recognised is rendered rather than guessed at.

Three further judgements:

- **The page name comes from `NAME`, not `.TH`.** `.TH LS 1` shouts by
  convention; the entity is `ls`, and that is what other pages
  cross-reference. Where the two agree apart from case, `NAME` wins.
- **`ls(1)` in `SEE ALSO` is a cross-reference**, resolving to a link when
  that page is converted in the same run, exactly like an Rd `\link`.
- **Two kinds of file in a manual directory are not pages**: `.so` stubs,
  which redirect to another entry, and mdoc(7) pages, which are BSD's
  separate macro package and not read here. Scanning a directory skips them
  with a warning; naming one explicitly reports what it is.

The section number picks the fence language: 2 and 3 document C functions,
everything else a command, so signatures and examples are highlighted as `c`
or `sh`.

## Output validity

The writer emits strings, so `typst-syntax` — Typst's own parser, already a
dependency of the Typst reader — doubles as a validator. Every test asserts
that generated markup parses with no errors, and
`cargo run --example validate -- <dir>` checks a whole directory of `.Rd`,
`.py`, `.typ`, or man page files. CI runs the tests, clippy, rustfmt, and the validator
over the fixture corpus in `tests/corpus/` on every push.

## Credits

Derived in part from [rd2qmd](https://github.com/eitsupi/rd2qmd) (MIT). See
[NOTICE.md](NOTICE.md).
