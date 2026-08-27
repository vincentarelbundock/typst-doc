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
