# man2typst

Render R, Python, and Typst API documentation as [Typst](https://typst.app).

```console
$ man2typst man/mean_ci.Rd
$ man2typst stats.py --params terms -o stats.typ
$ man2typst src/slides.typ -o manual.typ
$ man2typst man/ -o reference.typ
```

Each input is a file or a directory; every documented entity becomes one
manual entry, and all entries are joined into a single document in the order
given (a directory contributes its recognised files in name order). With
`--split`, each entry is instead written to its own `<topic>.typ` file in the
`--output` directory — useful when one source file documents many functions —
along with an `index.typ` whose table of contents `#include`s every entry, so
`typst compile index.typ` builds the whole reference manual.

Cross-references resolve within a run: a topic link whose target is converted
in the same invocation becomes a real link to that entry's heading, and any
other target renders as plain code, so every generated document compiles on
its own. Author-written `@name` references passing through Typst doc bodies
verbatim cannot be rewritten; a dangling one gets a warning instead. Internal
topics are skipped unless `--include-internal` is passed: `\keyword{internal}`
in R (the signal pkgdown filters on), and `_`-prefixed names in Python
(dunders like `__init__` stay public). Typst `_` definitions are always
private.

## Design

Three stages and one shared vocabulary:

```text
.Rd   --[r]------>  ir::Topic  --[typst]-->  Typst markup
.py   --[python]->
.typ  --[typ]---->
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

## Output validity

The writer emits strings, so `typst-syntax` — Typst's own parser, already a
dependency of the Typst reader — doubles as a validator. Every test asserts
that generated markup parses with no errors, and
`cargo run --example validate -- <dir>` checks a whole directory of `.Rd`,
`.py`, or `.typ` files. CI runs the tests, clippy, rustfmt, and the validator
over the fixture corpus in `tests/corpus/` on every push.

## Credits

Derived in part from [rd2qmd](https://github.com/eitsupi/rd2qmd) (MIT). See
[NOTICE.md](NOTICE.md).
