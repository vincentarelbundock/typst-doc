#import "/.calepin/calepin.typ" as calepin
#calepin.setup(eval: false)

#set document(title: [Reference])
#metadata((
  summary: "The typst-doc manual page, rendered as Typst by typst-doc itself.",
)) <website-metadata>

#title()

Generated from `man/typst-doc.1` by `typst-doc` itself:
`typst-doc man/typst-doc.1 > docs-src/reference.typ`.

#set heading(offset: 1)

= Render R, Python, Typst, and man page documentation as Typst <typst-doc>

```sh
typst-doc [-o|--output] [--params] [--include-internal] [-h|--help]
          [-V|--version] <INPUTS>
```

== Description

typst-doc reads documentation from source files and writes Typst markup\. Each input is a file or a directory; every documented entity becomes one manual entry\. A directory contributes every recognised file beneath it, descending into subdirectories, each in name order; entries whose name begins with a dot are skipped, so a \.venv or \.git in the tree is not mistaken for source\.

Where the entries go is decided by the output target\. With \-\-output, each one is written to its own \<topic\>\.typ file in that directory, alongside an index\.typ that outlines and includes them all; without it, they are joined into a single document on standard output, in the order given\.

Two topics can share a name — the same function documented in two modules, say\. Their files and heading labels then take the shortest part of the source path that tells them apart, each entry shows the file it came from, and a reference to the shared name is left as plain code, since it addresses no single entry\.

Four input languages are recognised, by extension: R documentation \(\.Rd\), Python modules and packages \(\.py\), Typst source documented with \/\// comments \(\.typ\), and Unix manual pages written in the man\(7\) macro package \(a section number such as \.1 or \.3, or \.man\)\.

Each topic title is a level-1 heading\. To nest the output under a title of your own, set the offset where you include it: \`\#set heading\(offset: 1\)\`\.

Cross-references resolve within a run: a topic link whose target is converted in the same invocation becomes a real link to that entry's heading, and any other target renders as plain code, so every generated document compiles on its own\.

Returns 0 on success\. An input that cannot be read, or whose extension is not recognised, is an error; unrecognised files found while scanning a directory are skipped instead\.

== Arguments

#table(
  columns: 2,
  stroke: none,
  [`-o, --output <OUTPUT>`], [Directory to write the manual into, created if missing: one \`\<topic\>\.typ\` file per topic, plus an \`index\.typ\` that outlines and includes them all\. Without it, the whole manual goes to stdout as a single document],
  [`--params <PARAMS> [default: table]`], [How to render the parameter list: table or terms],
  [`--include-internal`], [Include internal topics: \`\\keyword{internal}\` in R \(the signal pkgdown filters on\), and \`\_\`-prefixed names in Python\. Skipped by default\. Typst \`\_\` definitions are always private],
  [`-h, --help`], [Print help \(see a summary with '-h'\)],
  [`-V, --version`], [Print version],
  [`<INPUTS>`], [Input \`\.Rd\`, \`\.py\`, \`\.typ\`, or man page \(\`\.1\`, \`\.3\`, \`\.man\`\) files, or directories of them\. Topics are joined into one document, in the order given; a directory contributes its recognised files in name order],
)

== Examples

Convert one R help file and print the Typst to standard output:

```sh
typst-doc man/mean_ci.Rd
```

Convert a Python module, rendering parameters as a term list:

```sh
typst-doc stats.py --params terms > stats.typ
```

Convert a whole R package's help directory into one manual on standard output:

```sh
typst-doc man/ > reference.typ
```

Write a package that documents many functions into a directory, one file per topic plus an index:

```sh
typst-doc src/lib.typ -o reference/
```

== See Also

`typst`\(1\), `man`\(7\), `groff_man`\(7\)
