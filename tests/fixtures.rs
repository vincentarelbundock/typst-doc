//! End-to-end tests: source in, Typst out, validated by Typst's own parser.

use man2typst::typst::{Options, ParamsFormat};
use man2typst::{python, r, topic_to_typst};

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
            &Options {
                params_format: format,
                base_level: 1,
            },
        );
        assert_valid_typst(&output);
        // Math anywhere in the document pulls in the MiTeX import, once.
        assert_eq!(output.matches("#import").count(), 1);
    }
}

#[test]
fn escaping_survives_hostile_prose() {
    // Every character that is special in Typst markup, in text position.
    let rd = r#"\name{x}\title{t}\description{A #b $c *d* _e_ `f` <g> @h ~i [j] (k) 1. l -- m}"#;
    let topic = r::parse(rd).expect("Rd parses");
    let output = topic_to_typst(&topic, &Options::default());
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

    let output = topic_to_typst(topic, &Options::default());
    assert_valid_typst(&output);
    // No math, so no import.
    assert!(!output.contains("#import"));
}

/// `\item` means two different things in Rd, and the difference is invisible
/// until the output is empty: in `\itemize`/`\enumerate` it is a zero-arity
/// marker whose content follows as siblings, while in `\describe` and
/// `\arguments` it carries two argument groups.
#[test]
fn itemize_items_keep_their_content() {
    let rd = r"\name{x}\title{t}\details{\itemize{\item first \item second}}";
    let topic = r::parse(rd).expect("Rd parses");
    let output = topic_to_typst(&topic, &Options::default());

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
    let output = topic_to_typst(&topic, &Options::default());

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
    let output = topic_to_typst(&topic, &Options::default());

    assert!(output.contains("`get_draws()` keep forever"), "{output}");
}

/// `\ifelse` used to emit both branches unconditionally, so an HTML-only
/// phrase and its print alternative ran together in every target. Both
/// branches are kept, but guarded, so `target()` chooses at compile time.
#[test]
fn conditionals_guard_their_branches() {
    let rd = r"\name{x}\title{t}\details{\ifelse{html}{click here}{see page 3}}";
    let topic = r::parse(rd).expect("Rd parses");
    let output = topic_to_typst(&topic, &Options::default());

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
    let output = topic_to_typst(&topic, &Options::default());

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
        let output = topic_to_typst(&topic, &Options::default());
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
    let output = topic_to_typst(&topic, &Options::default());

    assert!(output.contains("unix note"), "{output}");
    assert_valid_typst(&output);
}

/// `\Sexpr` is R code needing a live session. It renders visibly unevaluated
/// rather than being unwrapped into prose that looks authored.
#[test]
fn sexpr_renders_as_visibly_unevaluated() {
    let rd = r#"\name{x}\title{t}\description{Version \Sexpr{packageVersion("x")} here}"#;
    let topic = r::parse(rd).expect("Rd parses");
    let output = topic_to_typst(&topic, &Options::default());

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
    let output = topic_to_typst(&topic, &Options::default());

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
