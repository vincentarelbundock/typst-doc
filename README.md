# man2typst

Render R and Python API documentation as [Typst](https://typst.app).

```console
$ man2typst man/mean_ci.Rd
$ man2typst stats.py --params terms -o stats.typ
```

## Design

Three stages and one shared vocabulary:

```text
.Rd  --[r]------>  ir::Topic  --[typst]-->  Typst markup
.py  --[python]->
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

## Output validity

The writer emits strings, so `typst-syntax` — Typst's own parser — is a
dev-dependency used as a validator. Every test asserts that generated markup
parses with no errors, and `cargo run --example validate -- <dir>` checks a
whole directory of `.Rd` or `.py` files.

## Credits

Derived in part from [rd2qmd](https://github.com/eitsupi/rd2qmd) (MIT). See
[NOTICE.md](NOTICE.md).
