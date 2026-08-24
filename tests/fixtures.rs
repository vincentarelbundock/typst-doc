//! End-to-end tests: source in, Typst out, validated by Typst's own parser.

use typst_doc::typst::{Options, ParamsFormat};
use typst_doc::{Entry, man, python, r, topic_to_typst, typ};

/// Assert that generated markup parses as well-formed Typst.
///
/// This is the official parser, used here only as a validator: the writer
/// emits strings, and this is what proves those strings are syntactically
/// sound.
fn assert_valid_typst(source: &str) {
    let root = typst_syntax::parse(source);
    let (errors, _warnings) = root.errors_and_warnings();
    assert!(
        errors.is_empty(),
        "generated Typst does not parse: {errors:#?}\n---\n{source}"
    );
}

const RD: &str = r#"
\name{mean_ci}
\alias{mean_ci}
\title{Confidence Interval for a \emph{Mean}}
\usage{
mean_ci(x, level = 0.95)
}
\arguments{
  \item{x}{A numeric vector. Values of \code{NA} are dropped.}
  \item{level, alpha}{Confidence level, in \eqn{(0, 1)}.}
}
\description{
  Computes a confidence interval, following \link[stats]{t.test}.

  A second paragraph with a URL: \url{https://example.org} and 100.5 units.
}
\details{
  The interval is \deqn{\bar{x} \pm t_{\alpha/2} s / \sqrt{n}}

  \itemize{
    \item First item
    \item Second item with \strong{bold}
  }

  \describe{
    \item{normal}{The usual case.}
    \item{small n}{Uses a t distribution.}
  }

  \tabular{lr}{
    Name \tab Value \cr
    Alpha \tab 0.05 \cr
  }
}
\value{
  A numeric vector of length two.
}
\examples{
x <- rnorm(100)
mean_ci(x)
\dontrun{
mean_ci(x, level = 0.99)
}
}
\seealso{\code{\link{t.test}}}
"#;

#[test]
fn rd_round_trips_to_valid_typst() {
    let topic = r::parse(RD).expect("Rd parses");
    assert_eq!(topic.name, "mean_ci");
    assert_eq!(topic.params.len(), 2);
    assert_eq!(topic.params[1].names, vec!["level", "alpha"]);
    assert_eq!(topic.examples.len(), 2);
    assert!(!topic.examples[1].run);

    for format in [ParamsFormat::Table, ParamsFormat::Terms] {
        let output = topic_to_typst(
            &topic,
            &Entry::default(),
            &Options {
                params_format: format,
                ..Options::default()
            },
        );
        assert_valid_typst(&output);
        // Math anywhere in the document pulls in the MiTeX import, once.
        assert_eq!(output.matches("#import").count(), 1);
        // `\describe` renders as explicit `terms.item` entries; the array
        // shorthand is deprecated in Typst 0.14+.
        assert!(output.contains("terms.item("), "{output}");
    }
}

#[test]
fn escaping_survives_hostile_prose() {
    // Every character that is special in Typst markup, in text position.
    let rd = r#"\name{x}\title{t}\description{A #b $c *d* _e_ `f` <g> @h ~i [j] (k) 1. l -- m}"#;
    let topic = r::parse(rd).expect("Rd parses");
    let output = topic_to_typst(&topic, &Entry::default(), &Options::default());
    assert_valid_typst(&output);
}

const PY: &str = r#"
def mean_ci(x, level=0.95):
    """Compute a confidence interval.

    A longer description spanning
    two source lines.

    Parameters
    ----------
    x : array_like
        A numeric vector.
    level : float, optional
        Confidence level.

    Returns
    -------
    tuple
        Lower and upper bounds.

    Raises
    ------
    ValueError
        If ``x`` is empty.
    """
"#;

#[test]
fn python_docstring_round_trips_to_valid_typst() {
    let topics = python::parse(PY, "stats.py").expect("Python parses");
    assert_eq!(topics.len(), 1);

    let topic = &topics[0];
    assert_eq!(topic.name, "stats.mean_ci");
    assert_eq!(
        topic.signature.as_deref(),
        Some("def mean_ci(x, level=0.95)")
    );
    assert_eq!(topic.params.len(), 2);
    assert_eq!(topic.params[0].names, vec!["x"]);
    assert_eq!(topic.params[0].ty.as_deref(), Some("array_like"));
    assert_eq!(topic.raises.len(), 1);

    let output = topic_to_typst(topic, &Entry::default(), &Options::default());
    assert_valid_typst(&output);
    // No math, so no import.
    assert!(!output.contains("#import"));
    // The signature fence carries the source language, not a hardcoded `r`.
    assert!(output.contains("```python\ndef mean_ci"), "{output}");
}

/// `\item` means two different things in Rd, and the difference is invisible
/// until the output is empty: in `\itemize`/`\enumerate` it is a zero-arity
/// marker whose content follows as siblings, while in `\describe` and
/// `\arguments` it carries two argument groups.
#[test]
fn itemize_items_keep_their_content() {
    let rd = r"\name{x}\title{t}\details{\itemize{\item first \item second}}";
    let topic = r::parse(rd).expect("Rd parses");
    let output = topic_to_typst(&topic, &Entry::default(), &Options::default());

    assert!(output.contains("- first"), "{output}");
    assert!(output.contains("- second"), "{output}");
    assert_valid_typst(&output);
}

/// `\tabular` carries its column spec as a positional group, not as a tag
/// option; reading the option instead silently produced a one-column table
/// with every cell fused together.
#[test]
fn tabular_splits_into_rows_and_columns() {
    let rd = r"\name{x}\title{t}\details{\tabular{lr}{A \tab B \cr C \tab D \cr}}";
    let topic = r::parse(rd).expect("Rd parses");
    let output = topic_to_typst(&topic, &Entry::default(), &Options::default());

    assert!(output.contains("columns: 2"), "{output}");
    assert!(output.contains("align: (left, right,)"), "{output}");
    assert!(output.contains("[A], [B],"), "{output}");
    assert!(output.contains("[C], [D],"), "{output}");
    assert_valid_typst(&output);
}

/// Text arrives split across nodes, and the space between `\code{f}` and the
/// word after it lives at that boundary.
#[test]
fn whitespace_between_inline_nodes_survives() {
    let rd = r"\name{x}\title{t}\description{alias to \code{get_draws()} keep forever}";
    let topic = r::parse(rd).expect("Rd parses");
    let output = topic_to_typst(&topic, &Entry::default(), &Options::default());

    assert!(output.contains("`get_draws()` keep forever"), "{output}");
}

/// `\ifelse` used to emit both branches unconditionally, so an HTML-only
/// phrase and its print alternative ran together in every target. Both
/// branches are kept, but guarded, so `target()` chooses at compile time.
#[test]
fn conditionals_guard_their_branches() {
    let rd = r"\name{x}\title{t}\details{\ifelse{html}{click here}{see page 3}}";
    let topic = r::parse(rd).expect("Rd parses");
    let output = topic_to_typst(&topic, &Entry::default(), &Options::default());

    assert!(output.contains("target() == \"html\""), "{output}");
    assert!(output.contains("click here"), "{output}");
    assert!(output.contains("see page 3"), "{output}");
    // The two branches must not run together as they did before.
    assert!(!output.contains("click heresee page 3"), "{output}");
    assert_valid_typst(&output);
}

#[test]
fn latex_only_content_is_guarded_to_the_print_target() {
    let rd = r"\name{x}\title{t}\details{\if{latex}{print only}}";
    let topic = r::parse(rd).expect("Rd parses");
    let output = topic_to_typst(&topic, &Entry::default(), &Options::default());

    assert!(output.contains("target() != \"html\""), "{output}");
    assert_valid_typst(&output);
}

/// A condition naming every target, or one this writer does not recognise, is
/// no reason to hide the content behind a guard.
#[test]
fn unconditional_content_gets_no_guard() {
    for rd in [
        r"\name{x}\title{t}\details{\if{html,latex}{everywhere}}",
        r"\name{x}\title{t}\details{\if{madeUpFormat}{everywhere}}",
    ] {
        let topic = r::parse(rd).expect("Rd parses");
        let output = topic_to_typst(&topic, &Entry::default(), &Options::default());
        assert!(output.contains("everywhere"), "{output}");
        assert!(!output.contains("target()"), "{output}");
        assert_valid_typst(&output);
    }
}

/// `#ifdef` selects on the build platform, which Typst has no notion of, so
/// the content is kept rather than dropped.
#[test]
fn platform_conditionals_keep_their_content() {
    let rd = "\\name{x}\\title{t}\\details{#ifdef unix\nunix note\n#endif}";
    let topic = r::parse(rd).expect("Rd parses");
    let output = topic_to_typst(&topic, &Entry::default(), &Options::default());

    assert!(output.contains("unix note"), "{output}");
    assert_valid_typst(&output);
}

/// `\Sexpr` is R code needing a live session. It renders visibly unevaluated
/// rather than being unwrapped into prose that looks authored.
#[test]
fn sexpr_renders_as_visibly_unevaluated() {
    let rd = r#"\name{x}\title{t}\description{Version \Sexpr{packageVersion("x")} here}"#;
    let topic = r::parse(rd).expect("Rd parses");
    let output = topic_to_typst(&topic, &Entry::default(), &Options::default());

    assert!(
        output.contains(r#"\Sexpr{packageVersion("x")}"#),
        "{output}"
    );
    assert_valid_typst(&output);
}

/// A single HTML element becomes an `html.elem` call guarded by `target()`;
/// anything more involved is kept verbatim rather than dropped.
#[test]
fn out_html_becomes_a_guarded_element() {
    let rd = r#"\name{x}\title{t}\details{\out{<span class="note">hi</span>}}"#;
    let topic = r::parse(rd).expect("Rd parses");
    let output = topic_to_typst(&topic, &Entry::default(), &Options::default());

    assert!(output.contains("html.elem(\"span\""), "{output}");
    assert!(output.contains("attrs: (class: \"note\")"), "{output}");
    assert!(output.contains("target() == \"html\""), "{output}");
    assert_valid_typst(&output);
}

/// Annotations come from slicing the original source, so arbitrary annotation
/// expressions survive in the author's own formatting.
#[test]
fn python_signatures_keep_complex_annotations() {
    let py =
        "def f(x: list[int], *, flag: bool = True, **kw: Any) -> None:\n    \"\"\"Doc.\"\"\"\n";
    let topics = python::parse(py, "m.py").expect("Python parses");

    assert_eq!(
        topics[0].signature.as_deref(),
        Some("def f(x: list[int], *, flag: bool = True, **kw: Any)")
    );
}

const TYP: &str = r#"/// Creates one logical slide command.
///
/// A logical slide is *one unit* of content, which may render as several
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
) = { none }
"#;

#[test]
fn typ_doc_comments_round_trip_to_valid_typst() {
    let topics = typ::parse(TYP);
    assert_eq!(topics.len(), 1);

    let topic = &topics[0];
    assert_eq!(topic.name, "slide");
    assert_eq!(
        topic.signature.as_deref(),
        Some("slide(layout: auto, numbered: auto, ..bodies) -> content")
    );
    assert_eq!(topic.params.len(), 3);
    assert_eq!(topic.params[0].names, vec!["layout"]);
    assert_eq!(
        topic.params[0].ty.as_deref(),
        Some("auto | str | dictionary")
    );
    assert_eq!(topic.params[0].default.as_deref(), Some("auto"));
    assert_eq!(topic.params[1].ty.as_deref(), Some("auto | bool"));
    assert_eq!(topic.params[2].names, vec!["..bodies"]);
    assert_eq!(topic.params[2].ty, None);

    let output = topic_to_typst(topic, &Entry::default(), &Options::default());
    assert_valid_typst(&output);
    // The signature fence carries the source language.
    assert!(output.contains("```typ\nslide("), "{output}");
    // The body is already Typst markup and must arrive verbatim, unescaped.
    assert!(output.contains("*one unit*"), "{output}");
    assert!(
        output.contains("```typ\n#mosaic.slide[Hello]\n```"),
        "{output}"
    );
    assert!(!output.contains("\\*one unit\\*"), "{output}");
}

/// tidy splits the type off the last `->` occurring anywhere in the block,
/// truncating prose. Only the final non-empty line is an annotation here.
#[test]
fn typ_arrow_in_prose_is_not_a_type() {
    let source = r#"/// Maps keys -> values eagerly.
///
/// The mapping keys -> values is total.
/// -> int
#let count(x) = x
"#;
    let topics = typ::parse(source);
    assert_eq!(topics.len(), 1);
    assert_eq!(topics[0].signature.as_deref(), Some("count(x) -> int"));

    let output = topic_to_typst(&topics[0], &Entry::default(), &Options::default());
    assert!(output.contains("Maps keys -> values eagerly."), "{output}");
    assert!(
        output.contains("The mapping keys -> values is total."),
        "{output}"
    );
    assert_valid_typst(&output);
}

#[test]
fn typ_headings_route_to_sections() {
    let source = r#"/// Frobnicates.
///
/// = Examples
///
/// ```typ
/// #frob(1)
/// ```
///
/// = See also
///
/// @slide
///
/// = Whatever
///
/// Custom prose.
#let frob(x) = x
"#;
    let topics = typ::parse(source);
    let topic = &topics[0];

    // `Examples` is a custom section, not `Topic::examples`: the doc body
    // interleaves prose and fenced code, which the examples field cannot hold.
    assert!(topic.examples.is_empty());
    assert_eq!(topic.sections.len(), 2);
    assert_eq!(
        topic.seealso,
        vec![typst_doc::ir::Block::Raw("@slide".into())]
    );

    let output = topic_to_typst(topic, &Entry::default(), &Options::default());
    assert!(output.contains("== Examples"), "{output}");
    assert!(output.contains("== Whatever"), "{output}");
    assert!(output.contains("Custom prose."), "{output}");
    assert_valid_typst(&output);
}

#[test]
fn typ_private_and_undocumented_definitions_are_skipped() {
    let source = r#"/// Documented but private.
#let _hidden(x) = x

#let undocumented(x) = x
"#;
    assert!(typ::parse(source).is_empty());
}

/// The regex-parser trap: tidy's `let` regex matches inside string literals.
/// The CST sees a string as a string.
#[test]
fn typ_let_inside_a_string_is_not_a_definition() {
    let source = r#"/// Holds source text.
#let snippet = "let fake(x) = x"
"#;
    let topics = typ::parse(source);
    assert_eq!(topics.len(), 1);
    assert_eq!(topics[0].name, "snippet");
    assert_eq!(topics[0].signature, None);
}

#[test]
fn typ_variable_binding_gets_no_signature() {
    let source = r#"/// The answer.
/// -> int
#let answer = 42
"#;
    let topics = typ::parse(source);
    let topic = &topics[0];
    assert_eq!(topic.name, "answer");
    assert_eq!(topic.signature, None);
    assert!(topic.params.is_empty());

    let output = topic_to_typst(topic, &Entry::default(), &Options::default());
    // The type annotation leads the description as inline code.
    assert!(output.contains("`int`"), "{output}");
    assert_valid_typst(&output);
}

/// The internal signal, per source: `\keyword{internal}` in Rd (what pkgdown
/// filters on), a `_`-prefixed name segment in Python. Dunders stay public.
#[test]
fn internal_topics_are_detectable() {
    let rd = r"\name{sanitize}\title{t}\keyword{internal}\description{Checks inputs.}";
    let topic = r::parse(rd).expect("Rd parses");
    assert!(topic.is_internal());

    let rd = r"\name{mean_ci}\title{t}\keyword{misc}\description{Public.}";
    assert!(!r::parse(rd).expect("Rd parses").is_internal());

    let py = r#"
def _helper(x):
    """Private."""

class Public:
    """A class."""
    def __init__(self):
        """Construct."""

class _Impl:
    """Hidden."""
    def run(self):
        """Even public methods of a private class are internal."""
"#;
    let topics = python::parse(py, "m.py").expect("Python parses");
    let by_name = |name: &str| {
        topics
            .iter()
            .find(|topic| topic.name == name)
            .unwrap_or_else(|| panic!("topic {name}"))
    };
    assert!(by_name("m._helper").is_internal());
    assert!(!by_name("m.Public").is_internal());
    assert!(!by_name("m.Public.__init__").is_internal());
    assert!(by_name("m._Impl").is_internal());
    assert!(by_name("m._Impl.run").is_internal());
}

/// Many-parameter signatures break one per line, matching the R and Typst
/// readers, instead of running the whole argument list together.
#[test]
fn python_long_signatures_break_one_parameter_per_line() {
    let py = "def convert(sourcevar, origin, destination, warn=True, nomatch=None):\n    \"\"\"Doc.\"\"\"\n";
    let topics = python::parse(py, "m.py").expect("Python parses");
    assert_eq!(
        topics[0].signature.as_deref(),
        Some(
            "def convert(\n  sourcevar,\n  origin,\n  destination,\n  warn=True,\n  nomatch=None\n)"
        )
    );

    // Three real parameters stay on one line; `*` is not a parameter.
    let py = "def f(x, *, flag=True, level=1):\n    \"\"\"Doc.\"\"\"\n";
    let topics = python::parse(py, "m.py").expect("Python parses");
    assert_eq!(
        topics[0].signature.as_deref(),
        Some("def f(x, *, flag=True, level=1)")
    );
}

/// `@name` references to unnumbered headings are Typst compile errors, and a
/// link to a label the document never defines is one too. Topic links
/// therefore render as `#link` to the heading label when the target is part
/// of the same run, and as plain code otherwise.
#[test]
fn topic_links_resolve_only_within_the_run() {
    let rd = r"\name{a}\title{t}\seealso{\link{b}}";
    let topic = r::parse(rd).expect("Rd parses");

    let output = topic_to_typst(&topic, &Entry::default(), &Options::default());
    assert!(output.contains("`b`"), "{output}");
    assert!(!output.contains("label("), "{output}");

    let options = Options {
        labels: [("b".to_owned(), "b".to_owned())].into(),
        ..Options::default()
    };
    let output = topic_to_typst(&topic, &Entry::default(), &options);
    assert!(output.contains("#link(label(\"b\"))[`b`]"), "{output}");
    assert_valid_typst(&output);
}

const MAN: &str = r#".TH GREET 1 "August 2026" "greet 1.0" "User Commands"
.SH NAME
greet, hello \- write a greeting
.SH SYNOPSIS
.B greet
[\fB\-n\fR \fINAME\fR]
[\fIFILE\fR...]
.SH DESCRIPTION
Write a greeting to standard output, one per
.IR FILE .
.SH OPTIONS
.TP
.BR \-n ", " \-\-name =\fINAME\fR
Greet \fINAME\fR instead of the world.
.TP
.B \-\-help
Display this help and exit.
.SH EXAMPLES
.EX
greet \-n Ada
.EE
.SH SEE ALSO
.BR echo (1)
"#;

#[test]
fn man_page_round_trips_to_valid_typst() {
    let topic = man::parse(MAN).expect("man page parses");

    // `.TH GREET` shouts; the NAME section spells the entity.
    assert_eq!(topic.name, "greet");
    assert_eq!(topic.aliases, vec!["hello"]);
    assert_eq!(
        typst_doc::ir::to_plain_text(&topic.title),
        "write a greeting"
    );
    // Filling joins the `.B greet` line with the argument lines that follow,
    // as groff would.
    assert_eq!(
        topic.signature.as_deref(),
        Some("greet [-n NAME] [FILE...]")
    );
    assert_eq!(topic.params.len(), 2);
    assert_eq!(topic.params[0].names, vec!["-n", "--name=NAME"]);
    assert_eq!(topic.examples.len(), 1);
    assert_eq!(topic.examples[0].code, "greet -n Ada");

    let output = topic_to_typst(&topic, &Entry::default(), &Options::default());
    assert_valid_typst(&output);
    // A section-1 page documents a command, so the fences say so.
    assert!(output.contains("```sh\ngreet ["), "{output}");
}

/// Generators such as `clap_mangen` emit the whole usage on one line, which a
/// raw block will not soft-wrap.
#[test]
fn man_long_usage_lines_wrap_under_a_hanging_indent() {
    let source = ".TH X 1\n.SH NAME\nx \\- t\n.SH SYNOPSIS\n\\fBx\\fR \
[\\fB\\-o\\fR|\\fB\\-\\-output <FILE>\\fR] [\\fB\\-\\-params\\fR] \
[\\fB\\-\\-base\\-level\\fR] [\\fB\\-\\-include\\-internal\\fR] \
[\\fB\\-\\-split\\fR] <\\fIINPUTS\\fR>\n";
    let topic = man::parse(source).expect("man page parses");
    let signature = topic.signature.expect("a synopsis");

    assert!(
        signature.lines().count() > 1,
        "long usage should wrap: {signature}"
    );
    assert!(
        signature.lines().all(|line| line.chars().count() <= 72),
        "{signature}"
    );
    // Continuations hang under the command name, and no break lands inside a
    // bracketed group.
    for line in signature.lines().skip(1) {
        assert!(line.starts_with("  "), "{signature}");
        assert_eq!(
            line.matches('[').count(),
            line.matches(']').count(),
            "{signature}"
        );
    }
}

/// Prose that lands in a SYNOPSIS section is not usage, and reflowing a
/// sentence under a hanging indent helps nobody.
#[test]
fn man_synopsis_prose_is_left_alone() {
    let sentence = "This submenu configures administrative policies using the \
org.bluez.AdminPolicySet(5) interface and nothing else whatsoever.";
    let source = format!(".TH X 1\n.SH NAME\nx \\- t\n.SH SYNOPSIS\n{sentence}\n");
    let topic = man::parse(&source).expect("man page parses");

    assert_eq!(topic.signature.as_deref(), Some(sentence));
}

/// `.TP` puts the tag on the *next* line, and only the section title says
/// whether that tag is a parameter or a term in prose.
#[test]
fn man_tp_lists_are_parameters_only_under_options() {
    let source = ".TH X 1\n.SH NAME\nx \\- t\n.SH DESCRIPTION\n.TP\n.B \\-v\nBe verbose.\n";
    let topic = man::parse(source).expect("man page parses");

    assert!(topic.params.is_empty());
    let output = topic_to_typst(&topic, &Entry::default(), &Options::default());
    assert!(output.contains("terms.item("), "{output}");
    assert!(output.contains("-v"), "{output}");
    assert_valid_typst(&output);
}

/// Font escapes are state, not markup: `\fB` opened on one line is still
/// open on the next until `\fR` closes it.
#[test]
fn man_font_state_survives_line_breaks() {
    let source =
        ".TH X 1\n.SH NAME\nx \\- t\n.SH DESCRIPTION\n\\fBbold across\nthe break\\fR plain\n";
    let topic = man::parse(source).expect("man page parses");
    let output = topic_to_typst(&topic, &Entry::default(), &Options::default());

    assert!(output.contains("*bold across the break*"), "{output}");
    assert!(output.contains("plain"), "{output}");
    assert_valid_typst(&output);
}

/// `ls(1)` in SEE ALSO is a cross-reference: a real link when that page is
/// converted in the same run, plain code otherwise.
#[test]
fn man_cross_references_resolve_within_the_run() {
    let source = ".TH X 1\n.SH NAME\nx \\- t\n.SH SEE ALSO\n.BR echo (1)\n";
    let topic = man::parse(source).expect("man page parses");

    let output = topic_to_typst(&topic, &Entry::default(), &Options::default());
    assert!(output.contains("`echo`"), "{output}");
    assert!(!output.contains("link(label("), "{output}");

    let options = Options {
        labels: [("echo".to_owned(), "echo".to_owned())].into(),
        ..Options::default()
    };
    let output = topic_to_typst(&topic, &Entry::default(), &options);
    assert!(
        output.contains("#link(label(\"echo\"))[`echo`]"),
        "{output}"
    );
    assert_valid_typst(&output);
}

/// A manual directory holds files that are not pages: `.so` stubs, and
/// mdoc(7) pages written in the other macro package. Each is reported as
/// what it is rather than as a parse failure.
#[test]
fn man_non_pages_are_named_not_guessed() {
    assert!(matches!(
        man::parse(".so man1/other.1\n"),
        Err(man::ManError::Redirect { .. })
    ));
    assert!(matches!(
        man::parse(".Dd August 23, 2026\n.Dt SSH 1\n.Sh NAME\n"),
        Err(man::ManError::Mdoc)
    ));
    assert!(matches!(
        man::parse("just some text\n"),
        Err(man::ManError::NotAManPage)
    ));
}

/// A man page name is not a Python name: `_exit(2)` is public API, and the
/// underscore convention must not hide it.
#[test]
fn man_underscore_names_are_not_internal() {
    let source = ".TH _EXIT 2\n.SH NAME\n_exit \\- terminate the calling process\n";
    let topic = man::parse(source).expect("man page parses");
    assert_eq!(topic.name, "_exit");
    assert!(!topic.is_internal());
    // Section 2 documents a C function, so the fences say `c`.
    assert_eq!(topic.lang.as_deref(), Some("c"));
}

/// Three characters that are ordinary in prose and structural in Typst, all
/// found in man pages: `_n_th` never closes its emphasis, `//` starts a
/// comment that eats the rest of a `#table(..)` call, and `*/` ends a block
/// comment.
#[test]
fn writer_escapes_typst_structure_hiding_in_prose() {
    use typst_doc::ir::{Block, Inline, Param};

    let mut topic = typst_doc::Topic::new("x");
    topic.title = vec![Inline::text("t")];
    topic.description = vec![Block::Paragraph(vec![
        Inline::Emph(vec![Inline::text("n")]),
        Inline::text("th time, see http://example.org/a/b, in "),
        Inline::Strong(vec![Inline::text("/etc")]),
        Inline::text("."),
    ])];
    topic.params = vec![Param {
        names: vec!["--url".to_owned()],
        body: vec![Block::Paragraph(vec![Inline::text(
            "/ a leading slash, and https://example.org//x",
        )])],
        ..Param::default()
    }];

    let output = topic_to_typst(&topic, &Entry::default(), &Options::default());
    assert_valid_typst(&output);
    // Emphasis with no word boundary needs the function form; the slashes
    // that would open or close a comment are escaped instead.
    assert!(output.contains("#emph[n]th time"), "{output}");
    assert!(output.contains("http:\\//example"), "{output}");
    assert!(output.contains("*\\/etc*"), "{output}");
    assert!(output.contains("[\\/ a leading slash"), "{output}");
}

/// Docstrings carry inline code markup that `pydocstring` reports verbatim.
/// Reading it keeps code looking like code instead of escaped punctuation.
#[test]
fn python_docstring_inline_code_markup_becomes_code() {
    let py = concat!(
        "def f(x):\n",
        "    \"\"\"Summary.\n",
        "\n",
        "    Pass ``None`` to keep the input, as :func:`countrycode` does,\n",
        "    or set `warn` to False.\n",
        "    \"\"\"\n",
    );
    let topics = python::parse(py, "m.py").expect("Python parses");
    let output = topic_to_typst(&topics[0], &Entry::default(), &Options::default());

    assert!(output.contains("`None`"), "{output}");
    assert!(output.contains("`countrycode`"), "{output}");
    assert!(output.contains("`warn`"), "{output}");
    assert!(!output.contains(":func:"), "{output}");
    assert_valid_typst(&output);
}

/// Unterminated markup is prose, not an invitation to swallow the paragraph.
#[test]
fn python_docstring_unterminated_backtick_stays_text() {
    let py = "def f(x):\n    \"\"\"Summary.\n\n    A stray ` backtick and a ratio 3:4 here.\n    \"\"\"\n";
    let topics = python::parse(py, "m.py").expect("Python parses");
    let output = topic_to_typst(&topics[0], &Entry::default(), &Options::default());

    assert!(output.contains("stray"), "{output}");
    assert!(output.contains("backtick"), "{output}");
    assert!(output.contains("3:4"), "{output}");
    assert_valid_typst(&output);
}

/// Two topics of the same name are told apart by their label and by the file
/// each entry names, since their titles and signatures are identical.
#[test]
fn a_shared_name_is_disambiguated_by_label_and_provenance() {
    let topic = r::parse(r"\name{image}\title{t}").expect("Rd parses");

    let entry = Entry {
        label: Some("component-image"),
        source: Some("src/component/image.typ"),
        ..Entry::default()
    };
    let output = topic_to_typst(&topic, &entry, &Options::default());

    assert!(output.contains("<component-image>"), "{output}");
    assert!(!output.contains("<image>"), "{output}");
    assert!(output.contains("`src/component/image.typ`"), "{output}");
    assert_valid_typst(&output);
}

/// A link resolves to the target's label, which is not always its name; a
/// name shared by two topics addresses neither, so it stays plain code.
#[test]
fn links_follow_the_target_label_and_stop_at_ambiguity() {
    let topic = r::parse(r"\name{a}\title{t}\seealso{\link{image}}").expect("Rd parses");

    let options = Options {
        labels: [("image".to_owned(), "layout-image".to_owned())].into(),
        ..Options::default()
    };
    let output = topic_to_typst(&topic, &Entry::default(), &options);
    assert!(
        output.contains("#link(label(\"layout-image\"))[`image`]"),
        "{output}"
    );
    assert_valid_typst(&output);

    // Ambiguous names are simply absent from the map.
    let output = topic_to_typst(&topic, &Entry::default(), &Options::default());
    assert!(output.contains("`image`"), "{output}");
    assert!(!output.contains("label("), "{output}");
}

/// A reference is written from somewhere: where a name is shared, the topic's
/// own scope decides which definition it means, ahead of the run-wide map.
#[test]
fn a_reference_resolves_in_the_referring_topics_scope_first() {
    let topic = r::parse(r"\name{a}\title{t}\seealso{\link{image}}").expect("Rd parses");

    let scope = [("image".to_owned(), "component-image".to_owned())].into();
    let entry = Entry {
        scope: Some(&scope),
        ..Entry::default()
    };
    let options = Options {
        labels: [("image".to_owned(), "layout-image".to_owned())].into(),
        ..Options::default()
    };

    let output = topic_to_typst(&topic, &entry, &options);
    assert!(
        output.contains("#link(label(\"component-image\"))[`image`]"),
        "{output}"
    );
    assert_valid_typst(&output);
}
