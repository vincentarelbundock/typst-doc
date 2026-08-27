# Third-party notices

## rd2qmd

Parts of this project derive from [rd2qmd](https://github.com/eitsupi/rd2qmd),
used under the MIT License.

> MIT License
>
> Copyright (c) 2026 rd2md authors
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in
> all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
> FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
> IN THE SOFTWARE.

Derived files, each of which carries an attribution header:

- `src/typst/escape.rs`: Typst markup and string-literal escaping, ported
  from `crates/rd2qmd-mdast/src/typst/mod.rs`.
- `src/typst/html.rs`: `\out{}` HTML-fragment handling (`html.elem` under a
  `target()` guard, verbatim fallback), ported from
  `crates/rd2qmd-mdast/src/typst/html.rs`.
- `src/typst/mod.rs`: writer structure and the Rd-to-Typst construct mapping
  (MiTeX for equations, `html.elem` for raw HTML, `#table`/`#terms` for
  parameter lists), ported from `crates/rd2qmd-mdast/src/typst/`.
- `src/r/mod.rs`: Rd section dispatch and the inline tag mapping, following
  `crates/rd2qmd-core/`.

## Dependencies

`rd-ast` and `rd-source` (MIT, <https://github.com/eitsupi/r-documentation-rs>)
provide Rd parsing. `pydocstring` and `rustpython-parser` provide Python
docstring and source parsing. `typst-syntax` is used as a development-only
validator. See `Cargo.toml` for versions.
