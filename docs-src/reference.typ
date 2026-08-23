#import "/.calepin/calepin.typ" as calepin
#calepin.setup(eval: false)

#set document(title: [Reference])
#metadata((
  summary: "The typst-doc manual page, rendered as Typst by typst-doc itself.",
)) <website-metadata>

#title()

Generated from `man/typst-doc.1` by `typst-doc` itself:
`typst-doc man/typst-doc.1 --base-level 2 -o docs-src/reference.typ`.

== Render R, Python, Typst, and man page documentation as Typst <typst-doc>

```sh
typst-doc [-o|--output] [--params] [--base-level] [--include-internal]
          [--split] [-h|--help] [-V|--version] <INPUTS>
```

=== Description

typst-doc reads documentation from source files and writes Typst markup\. Each input is a file or a directory; every documented entity becomes one manual entry, and all entries are joined into a single document in the order given\. A directory contributes its recognised files in name order\.

Four input languages are recognised, by extension: R documentation \(\.Rd\), Python modules and packages \(\.py\), Typst source documented with \/\// comments \(\.typ\), and Unix manual pages written in the man\(7\) macro package \(a section number such as \.1 or \.3, or \.man\)\.

Cross-references resolve within a run: a topic link whose target is converted in the same invocation becomes a real link to that entry's heading, and any other target renders as plain code, so every generated document compiles on its own\.

Returns 0 on success\. An input that cannot be read, or whose extension is not recognised, is an error; unrecognised files found while scanning a directory are skipped instead\.

=== Arguments

#table(
  columns: 2,
  stroke: none,
  [`-o, --output <OUTPUT>`], [Output file\. Defaults to stdout],
  [`--params <PARAMS> [default: table]`], [How to render the parameter list: table or terms],
  [`--base-level <BASE_LEVEL> [default: 1]`], [Heading level for the topic title],
  [`--include-internal`], [Include internal topics: \`\\keyword{internal}\` in R \(the signal pkgdown filters on\), and \`\_\`-prefixed names in Python\. Skipped by default\. Typst \`\_\` definitions are always private],
  [`--split`], [Write one \`\<topic\>\.typ\` file per topic into the \-\-output directory \(created if missing\) instead of joining everything into one document\. Topics sharing a name are disambiguated by their source path, with a warning],
  [`-h, --help`], [Print help \(see a summary with '-h'\)],
  [`-V, --version`], [Print version],
  [`<INPUTS>`], [Input \`\.Rd\`, \`\.py\`, \`\.typ\`, or man page \(\`\.1\`, \`\.3\`, \`\.man\`\) files, or directories of them\. Topics are joined into one document, in the order given; a directory contributes its recognised files in name order],
)

=== Examples

Convert one R help file and print the Typst to standard output:

```sh
typst-doc man/mean_ci.Rd
```

Convert a Python module, rendering parameters as a term list:

```sh
typst-doc stats.py --params terms -o stats.typ
```

Convert a whole R package's help directory into one manual:

```sh
typst-doc man/ -o reference.typ
```

Split a package that documents many functions into one file per topic:

```sh
typst-doc src/lib.typ --split -o reference/
```

=== See Also

`typst`\(1\), `man`\(7\), `groff_man`\(7\)
