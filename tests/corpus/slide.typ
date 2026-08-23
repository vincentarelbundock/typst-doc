// A hand-written fixture exercising the Typst reader: doc blocks, typed and
// defaulted parameters, a sink, sections, and a private definition.

/// Creates one logical slide command.
///
/// A logical slide is *one unit* of content, which may render as several
/// physical frames once incremental steps are applied.
///
/// = Examples
///
/// ```typ
/// #mosaic.slide[Hello]
/// ```
///
/// = See also
///
/// The `setup` function.
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
) = { none }

/// The package version.
/// -> str
#let version = "1.0.0"

/// Documented but private.
#let _hidden(x) = x
