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

#let doc-name = "typst-doc"
#let doc-label = <typst-doc>
#let doc-title = [Render R, Python, Typst, and man page documentation as Typst]
#let doc-aliases = ()
#let doc-source = none
#let doc-signature = ```sh
typst-doc [-o|--output] [--template] [--template-starter]
          [--include-internal] [-h|--help] [-V|--version] <INPUTS>
```
#let doc-params = (
  (names: ("-o", "--output <OUTPUT>"), type: none, default: none, optional: false, body: [Directory to write the manual into, created if missing: one \`\<topic\>\.typ\` file per topic, plus an \`index\.typ\` that outlines and includes them all\. Without it, the whole manual goes to stdout as a single document]),
  (names: ("--template <FILE>",), type: none, default: none, optional: false, body: [A Typst file whose contents replace the default template: the half of each generated document that renders the data block above it\. See \`\-\-template-starter\` for the default, as a starting point]),
  (names: ("--template-starter",), type: none, default: none, optional: false, body: [Also write the default template to \`template-default\.typ\` in the output directory, overwriting what is there, as a starting point for \`\-\-template\`\. Ignored without \`\-\-output\`; otherwise a run writes only the manual]),
  (names: ("--include-internal",), type: none, default: none, optional: false, body: [Include internal topics: \`\\keyword{internal}\` in R \(the signal pkgdown filters on\), and \`\_\`-prefixed names in Python\. Skipped by default\. Typst \`\_\` definitions are always private]),
  (names: ("-h", "--help"), type: none, default: none, optional: false, body: [Print help \(see a summary with '-h'\)]),
  (names: ("-V", "--version"), type: none, default: none, optional: false, body: [Print version]),
  (names: ("<INPUTS>",), type: none, default: none, optional: false, body: [Input \`\.Rd\`, \`\.py\`, \`\.typ\`, or man page \(\`\.1\`, \`\.3\`, \`\.man\`\) files, or directories of them\. Topics are joined into one document, in the order given; a directory contributes its recognised files in name order]),
)
#let doc-raises = ()
#let doc-examples = ()
#let doc-sections = (
  (id: "description", title: [Description], kind: "prose", body: [
typst-doc reads documentation from source files and writes Typst markup\. Each input is a file or a directory; every documented entity becomes one manual entry\. A directory contributes every recognised file beneath it, descending into subdirectories, each in name order; entries whose name begins with a dot are skipped, so a \.venv or \.git in the tree is not mistaken for source\.

Where the entries go is decided by the output target\. With \-\-output, each one is written to its own \<topic\>\.typ file in that directory, alongside an index\.typ that outlines and includes them all; without it, they are joined into a single document on standard output, in the order given\.

A Typst package is read through its entry point: each definition is named by the path it is imported under, and what the entry point does not export is treated as internal\. Elsewhere, two topics can share a name: the same function documented in two modules, say\. Their files and heading labels then take the shortest part of the source path that tells them apart, each entry shows the file it came from, and a reference to the shared name is left as plain code, since it addresses no single entry\.

Four input languages are recognised, by extension: R documentation \(\.Rd\), Python modules and packages \(\.py\), Typst source documented with \/\// comments \(\.typ\), and Unix manual pages in either macro package, man\(7\) or mdoc\(7\) \(a section number such as \.1 or \.3, or \.man\)\.

Each generated document is in two halves\. The first binds the topic's content to a fixed set of doc- variables \(doc-title, doc-params, doc-sections, and the rest\), every one of them defined for every topic, empty where the topic has nothing\. The second is the template, which renders them, and is the only half that decides how an entry looks\. It is inlined rather than imported, so an entry compiles on its own with nothing beside it\. Pass \-\-template FILE to supply your own; \-\-template-starter writes the default beside the manual as a place to start\.

Each topic title is a level-1 heading\. To nest the output under a title of your own, set the offset where you include it: \`\#set heading\(offset: 1\)\`\.

Cross-references resolve within a run, from where they are written: a link, oran author-written \@name, becomes a real link to the entry it names, choosingthe nearest definition when a name is shared: the same file first, then thesame directory\. A target this run does not define, or one still ambiguous fromwhere it was written, renders as plain code, with a warning\.

Returns 0 on success\. An input that cannot be read, or whose extension is not recognised, is an error; unrecognised files found while scanning a directory are skipped instead\.
  ]),
  (id: "arguments", title: [Arguments], kind: "params", items: doc-params),
  (id: "custom", title: [Examples], kind: "prose", body: [
Convert one R help file and print the Typst to standard output:

```sh
typst-doc man/mean_ci.Rd
```

Convert a Python module, writing the Typst to a file:

```sh
typst-doc stats.py > stats.typ
```

Convert a whole R package's help directory into one manual on standard output:

```sh
typst-doc man/ > reference.typ
```

Write a package that documents many functions into a directory, one file per topic plus an index:

```sh
typst-doc src/lib.typ -o reference/
```

Restyle a whole manual by replacing the template rather than passing options:

```sh
typst-doc man/ --template mine.typ -o reference/
```
  ]),
  (id: "seealso", title: [See Also], kind: "prose", body: [
`typst`\(1\), `man`\(7\), `groff_man`\(7\)
  ]),
)

// The default typst-doc template.
//
// Everything above this line is data: a fixed set of `doc-` bindings that every
// generated entry defines, empty where the topic has nothing. Everything below
// is presentation, and is the only half that decides how an entry looks.
//
// Copy this file, edit it, and pass it back with `--template FILE`. The data
// block does not change, so a template written against these names keeps
// working for R, Python, Typst, and man page entries alike.
//
// Bindings in scope:
//
//   doc-name       str          the topic's identifier
//   doc-label      label|none   the heading's label, for cross-references
//   doc-title      content      the one-line title
//   doc-aliases    array(str)   other names the topic answers to
//   doc-source     str|none     the file it came from, when a name is shared
//   doc-signature  content|none how the entity is called, as a raw block
//   doc-params     array(dict)  (names, type, default, optional, body)
//   doc-raises     array(dict)  same shape as doc-params
//   doc-examples   array(dict)  (run, code)
//   doc-sections   array(dict)  (id, title, kind, and body or items)
//
// `doc-sections` is the whole entry in order. Each has kind "prose" (with a
// `body`), "params", or "examples" (with `items`), so one loop renders
// everything, and reordering or retitling is list manipulation rather than an
// edit to the control flow below.
//
// Names starting with `doc-` are reserved for this contract; anything else a
// template defines is its own.

#let doc-render-params(items) = table(
  columns: 2,
  stroke: none,
  ..items
    .map(param => (
      raw(param.names.join(", ")),
      if param.type == none { param.body } else [#raw(param.type) \ #param.body],
    ))
    .flatten()
)

#let doc-render-examples(items) = items.map(example => example.code).join(parbreak())

#heading(level: 1, doc-title) #doc-label

#doc-signature

#if doc-source != none [
  #emph[Defined in] #raw(doc-source)
]

#for section in doc-sections {
  heading(level: 2, section.title)
  if section.kind == "prose" {
    section.body
  } else if section.kind == "params" {
    doc-render-params(section.items)
  } else if section.kind == "examples" {
    doc-render-examples(section.items)
  }
}
