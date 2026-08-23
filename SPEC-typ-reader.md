# SPEC: Typst doc-comment reader for typst-doc

Add a third reader to typst-doc: `.typ` source files with `///` doc comments,
producing `ir::Topic` values rendered by the existing Typst writer. The dialect
is compatible with what the `tidy` Typst package (v0.4.3) consumes today, with
three deliberate, documented divergences.

Read these files before writing code — they are the contract:

- `src/ir/{topic,block,inline}.rs` — the IR every reader targets.
- `src/r/mod.rs` and `src/python/mod.rs` — the two existing readers; match
  their structure, comment density, and naming.
- `src/typst/mod.rs` — the writer.
- `README.md` — the architecture ("one crate, `ir` depends on nothing").

## Background

typst-doc renders R (`.Rd`) and Python (`.py`) API documentation as Typst.
The pipeline: reader → `ir::Topic` → `typst` writer → Typst markup. `Topic`
is semantic (name, title, signature, params, value, examples, sections);
`Block`/`Inline` are the prose inside a section.

The new source language is Typst itself: packages like
`vincentarelbundock/mosaic` document functions with `///` comments in the
convention established by the `tidy` package. Example (abridged from mosaic):

```typ
/// Creates one logical slide command.
///
/// A logical slide is one unit of content, which may render as several
/// physical frames once incremental steps are applied.
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
  /// Whether the slide contributes to numbering.
  /// -> auto | bool
  numbered: auto,
  ..bodies
) = { ... }
```

Key property of this dialect, and the reason the reader is shaped the way it
is: **the doc body is already Typst markup.** It must pass through to the
output verbatim, never escaped and never re-parsed into the prose IR — any
round-trip through `Block::Paragraph`/`Inline::Text` would escape markup the
author wrote deliberately. `Block::Raw` (which exists) is the primary carrier,
not the escape hatch it is for the R reader.

## The dialect

### Doc blocks

A doc block is a maximal run of consecutive lines whose content, after leading
whitespace, starts with `///`. Strip the `///` and at most one following
space from each line. A blank line or any non-`///` line ends the block.

A doc block immediately preceding a `#let` (or `let` inside code mode)
documents that definition. A doc block inside a closure's parameter list,
immediately preceding a parameter, documents that parameter.

### Type annotations — divergence 1

If the **final non-empty line** of a doc block starts (after trimming) with
`->`, that line is the type annotation: the remainder is split on `|`, each
part trimmed. It applies to the definition (return type) or the parameter
(parameter type). A `->` anywhere else is prose.

tidy splits on the *last `->` occurring anywhere in the block*, so prose like
"maps keys -> values" silently truncates the description. Do not reproduce
that. Line-anchoring is what the convention plainly intends and what real
corpora (mosaic) actually write.

### Sections — divergence 2 (an extension, not a break)

Level-1 Typst headings (`= Title`) at the top level of a definition's doc body
partition it into sections. Matching is case-insensitive on the trimmed
heading text:

| heading                          | Topic field       |
|----------------------------------|-------------------|
| `value`, `returns`, `return`     | `value`           |
| `details`                        | `details`         |
| `note`                           | `note`            |
| `see also`, `seealso`            | `seealso`         |
| `references`                     | `references`      |
| `author`                         | `author`          |
| anything else                    | `sections` (custom; title passes through raw) |

Content before the first heading: the first paragraph (up to the first blank
line) is the `title`; the rest is `description`. Each region becomes a single
`Block::Raw` preserving internal blank lines — do not split into paragraphs.

`= Examples` maps to a **custom section**, not `Topic::examples`: the examples
field models bare runnable code (Rd's `\examples`), while a Typst doc body
freely interleaves prose and fenced code. Rendering it as a section titled
"Examples" with verbatim body gives the same visual result without forcing
prose into a code-only shape. Under tidy these headings simply render as
headings, so the extension degrades gracefully.

### Documented definitions only — divergence 3

Only definitions carrying a doc block become Topics (tidy lists undocumented
public functions too). This mirrors the Python reader, where a docstring is
the signal that an entity is documentation-worthy. Definitions whose name
starts with `_` are always skipped, documented or not.

### Out of scope (pass through or ignore, do not implement)

- `@name` cross-references: left verbatim in the body. They resolve iff the
  target Topic is rendered in the same document (headings carry `<name>`
  labels); a dangling ref is the author's concern, and parse-validation does
  not resolve refs, so tests are unaffected.
- ```` ```example ```` / ```` ```examplec ```` executed blocks: pass through
  as ordinary fenced code.
- `>>> ` doctest lines: pass through (tidy 0.4.3 has this path commented out).
- Curried definitions (`#let f = g.with(...)`): treat as variables.
- Module assembly / outlines: the CLI converts one file; joining is the
  caller's job.
- Variables (`#let x = value`, no parameter list): produce a Topic with
  `signature: None` and no params. The type annotation, if any, goes in the
  first line of the description as `` `type` `` — or skip rendering it; keep
  whatever is simplest and note it.

## Parsing strategy — this is the point of the exercise

**Definition and parameter structure MUST come from the `typst-syntax` CST,
not from regexes.** Promote `typst-syntax` from `[dev-dependencies]` to
`[dependencies]` (same version). tidy finds `#let` with
`regex("#?let (\w[\w\d\-_]*)\s*(\(?)")`, which false-positives inside strings
and cannot see the argument list's structure; the entire justification for
this reader is doing that part properly.

Approach:

1. `typst_syntax::parse(source)` (markup mode). The CST is lossless: comment
   trivia appears as nodes (`SyntaxKind::LineComment`).
2. Walk for let bindings. For each, collect the run of `LineComment` siblings
   immediately preceding it (only those whose text starts with `///`).
3. For a function binding, walk its closure's parameter list. Each parameter's
   doc block is the `LineComment` run preceding it inside the params node.
   Parameter name, and default value if present, come from the CST.
4. **Slice source text over spans** for default values (and any expression
   text you need) — exactly as `src/python/mod.rs` does with `TextRange`.
   Never re-render expressions; slicing preserves the author's formatting.

Before implementing, write a small throwaway example (see `examples/dump.rs`
for the pattern) that parses a fixture and prints the CST, to verify where
`///` comments actually sit relative to let bindings and parameters. If
comment trivia placement makes attachment genuinely awkward, a hybrid is
acceptable: locate `///` runs by line scanning, but definitions, parameter
lists, names, and defaults still come from the CST. Document the choice in a
module comment.

## Mapping to the IR

- `Topic::name`: the binding name. `aliases`, `keywords`: empty.
- `Topic::title`: first paragraph of the body, as a single **`Inline::Raw`**
  (new variant, see below).
- `Topic::signature`: reconstructed, not sliced (the raw source span would
  include the interleaved `///` blocks):
  `name(param, param: default, ..rest) -> type`. Defaults are source-sliced.
  If the result exceeds ~78 chars or has more than 3 parameters, break one
  parameter per line with two-space indent (match how R usage blocks look).
  Parameter *types* do not appear in the signature — they render in the
  Arguments table via `Param::ty`, as the Python reader does.
- `Topic::params`: one `Param` per documented-or-not parameter, in source
  order. `names`: one name (or `..name` for sink). `ty`: types joined
  `" | "`. `default`: source-sliced. `body`: single `Block::Raw`.
- Section fields per the table above; bodies are `Vec<Block>` containing one
  `Block::Raw` each.

## IR and writer changes (small, required)

1. **`Inline::Raw(String)`** — verbatim Typst, written unescaped. Wire it
   into: the writer's `write_inline` (push verbatim, `at_line_start = false`),
   `to_plain_text` (push as-is), `inlines_contain_math` (false). Rationale
   comment: titles from Typst doc bodies are already markup; `Inline::Text`
   would escape them.
2. **`Topic::lang: Option<String>`** — the source language for code fences.
   The writer currently hardcodes `Some("r")` for both the signature block and
   examples (`src/typst/mod.rs`, two call sites) — wrong for Python today.
   Writer uses `topic.lang.as_deref()`; R reader sets `Some("r")`, Python
   `Some("python")`, this reader `Some("typ")`. Update the two existing
   readers and any test expectations this changes.

Both matches on `Block`/`Inline` in the writer are deliberately exhaustive —
extend them, never add a `_` arm.

## Wiring

- New module `src/typ/mod.rs`, exported from `lib.rs` alongside `r` and
  `python`. Entry point: `pub fn parse(source: &str) -> Vec<Topic>` (parsing
  is total: a file with no documented definitions yields an empty Vec; CST
  errors do not abort — typst-syntax always produces a tree).
- CLI (`src/main.rs`): extension `typ` routes to the new reader. Multiple
  topics per file already works (Python does it).
- `examples/validate.rs`: add the `typ` extension to the match.
- README: add the reader to the pipeline diagram and add a short "Typst doc
  comments" section documenting the dialect — the three divergences from tidy
  explicitly. The dialect spec is part of the deliverable.

## Tests (in `tests/fixtures.rs`, same style as existing ones)

Every generated document must pass `assert_valid_typst` (already defined
there: parses with `typst_syntax` and asserts no errors).

1. A distilled mosaic-style fixture (function with doc block, fenced `typ`
   example, several documented params with types and defaults, a `..sink`,
   `-> content` return): assert name, title, signature string, param names /
   types / defaults, and that the body's markup (e.g. a `*bold*` phrase and
   the code fence) appears **verbatim** in the output — not escaped.
2. `->` in prose ("maps keys -> values" mid-body) survives intact; only a
   final `-> int` line becomes the type. This is the tidy-divergence test.
3. `= Examples` and `= See also` headings route to a custom section and
   `seealso` respectively; an unrecognized heading becomes a custom section.
4. `_private` definitions and undocumented definitions produce no Topic.
5. A `let` inside a string literal produces no Topic (the regex-parser trap;
   this is why the CST is required).
6. A variable binding (documented, no params) produces a signature-less Topic.
7. `Topic::lang` renders the signature fence as ```` ```typ ````, and the
   existing Python test's fence as ```` ```python ````.

Run the full suite; `cargo clippy --all-targets` and `cargo fmt` must be
clean. If `/tmp/marginaleffects-head-3c6249b3/r/man` exists, run
`cargo run --example validate` over it to confirm no R-side regression from
the `Topic::lang` change; if the path is gone, skip and say so.

## Non-negotiables

- `ir` keeps zero reader/writer dependencies.
- Doc bodies are never escaped and never round-tripped through
  `Paragraph`/`Text`.
- No new syntax beyond the three divergences; mosaic's existing comments must
  parse unchanged.
- Match the existing code's comment style: comments explain *why*, not what.
