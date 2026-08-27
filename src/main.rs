use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;

use typst_doc::cli::Cli;
use typst_doc::typst::escape::typst_string;
use typst_doc::typst::{Entry, Options};
use typst_doc::{Topic, man, python, r, topic_to_typst, typ};

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Each topic keeps its source path: it is what tells two same-named
    // topics apart, in their labels, their file names, and the entries
    // themselves.
    let mut topics: Vec<(PathBuf, Topic)> = Vec::new();
    for input in &cli.inputs {
        if input.is_dir() {
            let mut files = Vec::new();
            let mut visited = std::collections::HashSet::new();
            recognised_files(input, &mut files, &mut visited)?;
            for file in files {
                let parsed = parse_file(&file, Lenient::Yes)?;
                topics.extend(parsed.into_iter().map(|topic| (file.clone(), topic)));
            }
        } else {
            let parsed = parse_file(input, Lenient::No)?;
            topics.extend(parsed.into_iter().map(|topic| (input.clone(), topic)));
        }
    }

    // A Typst package names its own API: the entry point says what each
    // binding is called and, by not mentioning it, what is private.
    let api = typ::package::Api::discover(&cli.inputs);
    let unexported = apply_package_api(&api, &mut topics);

    let found = topics.len();
    if !cli.include_internal {
        topics.retain(|(_, topic)| !topic.is_internal());
        if unexported > 0 {
            let entrypoints: Vec<String> = api
                .entrypoints
                .iter()
                .map(|path| path.display().to_string())
                .collect();
            let plural = if unexported == 1 { "" } else { "s" };
            eprintln!(
                "note: {unexported} definition{plural} not exported by {}; \
                 pass --include-internal to document {}",
                entrypoints.join(", "),
                if unexported == 1 { "it" } else { "them" }
            );
        }
    }

    if topics.is_empty() {
        if found > 0 {
            bail!("only internal topics found; pass --include-internal to render them");
        }
        bail!("no documented entities found");
    }

    let addresses = Addresses::of(&topics);

    let template = cli
        .template
        .as_ref()
        .map(|path| {
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
        })
        .transpose()?;

    let options = Options {
        template,
        // A shared name addresses two headings, so it addresses neither.
        labels: topics
            .iter()
            .zip(&addresses.slugs)
            .flat_map(|((_, topic), slug)| {
                names_of(topic)
                    .into_iter()
                    .filter(|name| !addresses.ambiguous.contains(*name))
                    .map(|name| (name.to_owned(), slug.clone()))
                    .collect::<Vec<_>>()
            })
            .collect(),
    };

    let sources: Vec<String> = topics
        .iter()
        .map(|(path, _)| path.display().to_string())
        .collect();

    let scopes = scopes(&topics, &addresses);

    let rendered: Vec<String> = topics
        .iter()
        .zip(&addresses.slugs)
        .zip(&sources)
        .zip(&scopes)
        .map(|((((_, topic), slug), source), scope)| {
            let entry = Entry {
                label: Some(slug.as_str()),
                source: addresses
                    .ambiguous
                    .contains(&topic.name)
                    .then_some(source.as_str()),
                scope: Some(scope),
            };
            let document = topic_to_typst(topic, &entry, &options);
            resolve_refs(&document, |name| {
                scope
                    .get(name)
                    .or_else(|| options.labels.get(name))
                    .cloned()
            })
        })
        .collect();

    for (topic, target) in dangling_refs(&topics, &rendered) {
        if addresses.ambiguous.contains(&target) {
            eprintln!("warning: ambiguous reference @{target} (in topic `{topic}`)");
        } else {
            eprintln!("warning: unresolved reference @{target} (in topic `{topic}`)");
        }
    }

    let Some(dir) = &cli.output else {
        print!("{}", rendered.join("\n"));
        return Ok(());
    };

    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let names: Vec<String> = addresses
        .slugs
        .iter()
        .map(|slug| format!("{slug}.typ"))
        .collect();
    for (document, file_name) in rendered.iter().zip(&names) {
        let file = dir.join(file_name);
        std::fs::write(&file, document).with_context(|| format!("writing {}", file.display()))?;
    }
    write_template_starters(dir, &names)?;
    write_index(dir, &names)
}

/// Rename Typst topics to the paths their package exports them under, and
/// report how many it does not export at all.
///
/// A package's `lib.typ` decides both: `component/image.typ`'s `image` is
/// `image` if the entry point re-exports it plainly, `layout.image` if behind
/// a module alias, and private if the entry point never mentions it. Since
/// Typst cannot expose two things under one name, the names this produces are
/// unique by construction, and the file-path disambiguation is left for
/// sources no entry point speaks for.
///
/// Anything unexported is marked internal, joining `\keyword{internal}` in R
/// and `_`-prefixed names in Python: the same idea, spelled the way each
/// language spells it.
fn apply_package_api(api: &typ::package::Api, topics: &mut [(PathBuf, Topic)]) -> usize {
    let mut unexported = 0;
    for (path, topic) in topics {
        if extension(path).as_deref() != Some("typ") || !api.covers(path) {
            continue;
        }
        match api.public_name(path, &topic.name) {
            Some(public) => {
                // A doc comment refers to what the source calls the binding,
                // so the entry keeps answering to that too. Where two modules
                // define the same name, both claim it, and the reference
                // resolves from the scope it was written in.
                let source_name = std::mem::replace(&mut topic.name, public.clone());
                if source_name != topic.name {
                    topic.aliases.push(source_name);
                }
            }
            None => {
                topic.keywords.push("internal".to_owned());
                unexported += 1;
            }
        }
    }
    unexported
}

/// The two files a template author starts from: the default template, and one
/// entry's data block showing what it has to work with.
///
/// Both are generated rather than written by hand. The template is the one the
/// binary actually inlines, and the example data comes out of the same emitter
/// as every real entry, so neither can drift from what `--template` is handed.
/// Both are rewritten on every run, which is why the template says to copy it
/// before editing.
fn write_template_starters(dir: &Path, names: &[String]) -> Result<()> {
    for reserved in ["template-default.typ", "example-data.typ"] {
        if names.iter().any(|name| name == reserved) {
            eprintln!("warning: a topic file is named {reserved}; not writing it");
            return Ok(());
        }
    }

    let file = dir.join("template-default.typ");
    std::fs::write(&file, typst_doc::DEFAULT_TEMPLATE)
        .with_context(|| format!("writing {}", file.display()))?;

    // An empty template renders nothing, leaving the data block alone.
    let data = topic_to_typst(
        &example_topic(),
        &Entry::default(),
        &Options {
            template: Some(String::new()),
            ..Options::default()
        },
    );
    let file = dir.join("example-data.typ");
    std::fs::write(
        &file,
        format!(
            "// One entry's data block, as `typst-doc` emits it. A template is \
             inlined\n// below a block of exactly this shape. See \
             template-default.typ.\n\n{data}"
        ),
    )
    .with_context(|| format!("writing {}", file.display()))
}

/// A topic exercising every field a template can read, so the example data is
/// complete rather than whatever the first real entry happened to contain.
fn example_topic() -> Topic {
    use typst_doc::ir::{Block, Example, Inline, Param, Section};

    let prose = |text: &str| vec![Block::Paragraph(vec![Inline::text(text)])];
    Topic {
        name: "mean_ci".to_owned(),
        title: vec![Inline::text("Confidence interval for a mean")],
        aliases: vec!["mean_ci".to_owned()],
        keywords: Vec::new(),
        signature: Some("mean_ci(x, level = 0.95)".to_owned()),
        lang: Some("r".to_owned()),
        description: prose("Computes a normal-approximation confidence interval."),
        details: prose("The interval is symmetric about the sample mean."),
        params: vec![
            Param {
                names: vec!["x".to_owned()],
                ty: Some("numeric".to_owned()),
                default: None,
                optional: false,
                body: prose("A numeric vector."),
            },
            Param {
                names: vec!["level".to_owned()],
                ty: Some("numeric".to_owned()),
                default: Some("0.95".to_owned()),
                optional: true,
                body: prose("Coverage probability."),
            },
        ],
        value: prose("A numeric vector of length two."),
        raises: vec![Param {
            names: vec!["ValueError".to_owned()],
            ty: None,
            default: None,
            optional: false,
            body: prose("If x is empty."),
        }],
        examples: vec![
            Example {
                code: "mean_ci(rnorm(100))".to_owned(),
                run: true,
            },
            Example {
                code: "mean_ci(x, level = 0.99)".to_owned(),
                run: false,
            },
        ],
        seealso: prose("t.test"),
        references: prose("Casella and Berger (2002)."),
        note: prose("Assumes approximate normality."),
        author: prose("A. Person"),
        sections: vec![Section {
            title: vec![Inline::text("Caveats")],
            body: prose("An unrecognised section keeps its own heading."),
        }],
    }
}

/// The entry point of a split manual: a table of contents followed by an
/// `#include` of every topic file, so one `typst compile index.typ` builds
/// the whole reference and cross-topic references resolve.
fn write_index(dir: &Path, names: &[String]) -> Result<()> {
    if names.iter().any(|name| name == "index.typ") {
        eprintln!("warning: a topic file is named index.typ; not writing an index");
        return Ok(());
    }
    let mut index = String::from("#outline(depth: 1)\n");
    for name in names {
        index.push_str(&format!("\n#include {}\n", typst_string(name)));
    }
    let file = dir.join("index.typ");
    std::fs::write(&file, index).with_context(|| format!("writing {}", file.display()))
}

/// Rewrite the `@name` references an author wrote into links.
///
/// The R and man readers produce semantic links, which the writer resolves as
/// it goes. A Typst doc comment instead carries `@name` as markup, in tidy's
/// convention, and it passes through to the output verbatim, where Typst
/// rejects it, since referencing an unnumbered heading is an error even when
/// the name is unique. Resolving it here, against the same scope-first map the
/// writer uses, turns it into the `#link(label(..))` form that works, and
/// leaves a name nothing defines untouched for [`dangling_refs`] to report.
///
/// The rewrite reads the *rendered* document, so a `@` inside a code block or
/// a raw span is not a reference and is never touched: only what Typst itself
/// parses as a `RefMarker` is.
fn resolve_refs(document: &str, label_of: impl Fn(&str) -> Option<String>) -> String {
    let mut edits = Vec::new();
    collect_refs(&typst_syntax::parse(document), 0, &mut edits);

    let mut out = document.to_owned();
    // Back to front, so an earlier edit cannot shift a later range.
    for (range, target) in edits.into_iter().rev() {
        let Some(label) = label_of(&target) else {
            continue;
        };
        out.replace_range(
            range,
            &format!(
                "#link(label({}))[#raw({})]",
                typst_string(&label),
                typst_string(&target)
            ),
        );
    }
    out
}

/// The byte range and target of every `@name` in the document, in order.
fn collect_refs(
    node: &typst_syntax::SyntaxNode,
    offset: usize,
    out: &mut Vec<(std::ops::Range<usize>, String)>,
) -> usize {
    if node.children().len() == 0 {
        let text = node.leaf_text();
        if node.kind() == typst_syntax::SyntaxKind::RefMarker {
            out.push((
                offset..offset + text.len(),
                text.trim_start_matches('@').to_owned(),
            ));
        }
        return offset + text.len();
    }

    let mut offset = offset;
    for child in node.children() {
        offset = collect_refs(child, offset, out);
    }
    offset
}

/// References that no topic in this run defines.
///
/// Semantic links from the R reader already degrade to plain code when their
/// target is unknown; what remains are author-written `@name` refs passing
/// verbatim through Typst doc bodies. Those are Typst compile errors, not
/// dead ends, so they deserve a warning while the source is still in view.
/// Detection parses the *rendered* documents, so refs and labels are counted
/// exactly as Typst will see them.
fn dangling_refs(topics: &[(PathBuf, Topic)], rendered: &[String]) -> Vec<(String, String)> {
    let mut labels = std::collections::HashSet::new();
    let mut refs = Vec::new();
    for ((_, topic), document) in topics.iter().zip(rendered) {
        let root = typst_syntax::parse(document);
        scan_refs(&root, &topic.name, &mut labels, &mut refs);
    }
    refs.retain(|(_, target)| !labels.contains(target));
    refs
}

fn scan_refs(
    node: &typst_syntax::SyntaxNode,
    topic: &str,
    labels: &mut std::collections::HashSet<String>,
    refs: &mut Vec<(String, String)>,
) {
    match node.kind() {
        typst_syntax::SyntaxKind::Label => {
            let text = node.leaf_text();
            labels.insert(text.trim_matches(['<', '>']).to_owned());
        }
        typst_syntax::SyntaxKind::RefMarker => {
            let text = node.leaf_text();
            refs.push((topic.to_owned(), text.trim_start_matches('@').to_owned()));
        }
        _ => {}
    }
    for child in node.children() {
        scan_refs(child, topic, labels, refs);
    }
}

/// How each topic is addressed: by Typst, as a heading label, and on disk, as
/// a file name.
///
/// One pass serves both, so a topic's label and its file always agree, and
/// every reader is disambiguated the same way.
struct Addresses {
    /// One slug per topic, index-aligned with `topics`: the label its heading
    /// carries, and the stem of the file it is written to.
    slugs: Vec<String>,
    /// Names that more than one topic answers to. Such a name addresses no
    /// single heading run-wide, so it resolves only from the scope of one of
    /// them (see [`scopes`]), and every entry carrying one shows the file it
    /// came from.
    ambiguous: std::collections::HashSet<String>,
}

impl Addresses {
    /// The usual slug is the topic's name, but topics sharing a name would
    /// address, and overwrite, each other. A colliding group instead takes
    /// the shortest suffix of each source path that tells its members apart
    /// (mosaic's two `image` functions become `component-image` and
    /// `layout-image`), and a warning reports the choice. The suffix
    /// disambiguates the topic within this run; it makes no claim about the
    /// function's qualified name, which the source layout does not determine.
    fn of(topics: &[(PathBuf, Topic)]) -> Self {
        Self {
            slugs: topic_slugs(topics),
            ambiguous: duplicated_names(topics),
        }
    }
}

/// For each topic, the labels only its own scope can resolve.
///
/// A reference is written from somewhere. `@image` in a doc comment inside
/// `component/` means the `image` defined there, the way an import written at
/// that spot would, so a shared name still resolves for the topics sitting
/// beside one of its definitions: the same file first, then the same
/// directory. It falls back to plain code only where it is genuinely ambiguous
/// from where it was written, which leaves a package documented in one run
/// with its links intact.
fn scopes(
    topics: &[(PathBuf, Topic)],
    addresses: &Addresses,
) -> Vec<std::collections::HashMap<String, String>> {
    let mut scopes = vec![std::collections::HashMap::new(); topics.len()];
    if addresses.ambiguous.is_empty() {
        return scopes;
    }

    for (index, (path, _)) in topics.iter().enumerate() {
        for name in &addresses.ambiguous {
            let candidates = || {
                topics
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, topic))| names_of(topic).contains(&name.as_str()))
            };
            let nearest = |matches: Vec<usize>| match matches[..] {
                [only] => Some(only),
                _ => None,
            };

            let same_file = candidates()
                .filter(|(_, (other, _))| other == path)
                .map(|(index, _)| index)
                .collect();
            let found = nearest(same_file).or_else(|| {
                let same_directory = candidates()
                    .filter(|(_, (other, _))| other.parent() == path.parent())
                    .map(|(index, _)| index)
                    .collect();
                nearest(same_directory)
            });

            if let Some(found) = found {
                scopes[index].insert(name.clone(), addresses.slugs[found].clone());
            }
        }
    }
    scopes
}

/// Every name a topic answers to: its own, and the aliases beside it.
///
/// An Rd file documents several functions under one `\name` and lists the rest
/// as `\alias`; a man page's NAME line does the same for `printf`, `fprintf`,
/// and `sprintf`. A reader writing `\link{sprintf}` means that page, so every
/// one of those names has to address it, since the entry is reachable under one
/// name only if the others are thrown away. Repeats are dropped, since
/// `\name{x}\alias{x}` is ordinary and does not make `x` ambiguous.
fn names_of(topic: &Topic) -> Vec<&str> {
    let mut names = vec![topic.name.as_str()];
    for alias in &topic.aliases {
        if !names.contains(&alias.as_str()) {
            names.push(alias.as_str());
        }
    }
    names
}

/// Names that more than one topic answers to, aliases included.
fn duplicated_names(topics: &[(PathBuf, Topic)]) -> std::collections::HashSet<String> {
    let mut seen = std::collections::HashSet::new();
    let mut twice = std::collections::HashSet::new();
    for (_, topic) in topics {
        for name in names_of(topic) {
            if !seen.insert(name.to_owned()) {
                twice.insert(name.to_owned());
            }
        }
    }
    twice
}

/// One slug per topic, index-aligned with `topics`.
fn topic_slugs(topics: &[(PathBuf, Topic)]) -> Vec<String> {
    let mut groups: std::collections::BTreeMap<&str, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (index, (_, topic)) in topics.iter().enumerate() {
        groups.entry(topic.name.as_str()).or_default().push(index);
    }

    let mut slugs = vec![String::new(); topics.len()];
    for (name, indices) in groups {
        if let [index] = indices[..] {
            slugs[index] = sanitize(name);
            continue;
        }
        let chosen = disambiguate(&indices, topics);
        eprintln!(
            "warning: {} topics named `{name}`; writing {}; a reference to `{name}` resolves to the nearest, or stays plain code",
            indices.len(),
            chosen
                .iter()
                .map(|slug| format!("{slug}.typ"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        for (&index, slug) in indices.iter().zip(chosen) {
            slugs[index] = slug;
        }
    }
    slugs
}

/// File stems for one group of same-named topics.
fn disambiguate(indices: &[usize], topics: &[(PathBuf, Topic)]) -> Vec<String> {
    let components: Vec<Vec<String>> = indices
        .iter()
        .map(|&index| {
            topics[index]
                .0
                .with_extension("")
                .components()
                .filter_map(|component| match component {
                    std::path::Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect()
        })
        .collect();
    let deepest = components.iter().map(Vec::len).max().unwrap_or(0);

    for depth in 1..=deepest {
        let candidates: Vec<String> = indices
            .iter()
            .zip(&components)
            .map(|(&index, parts)| {
                let suffix = &parts[parts.len().saturating_sub(depth)..];
                let mut chosen: Vec<&str> = suffix.iter().map(String::as_str).collect();
                // The file stem often *is* the topic name (`image.typ`
                // defining `image`); avoid doubling it.
                let name = topics[index].1.name.as_str();
                if chosen.last().copied() != Some(name) {
                    chosen.push(name);
                }
                sanitize(&chosen.join("-"))
            })
            .collect();
        let unique: std::collections::HashSet<&String> = candidates.iter().collect();
        if unique.len() == candidates.len() {
            return candidates;
        }
    }

    // Same-named topics from the very same file: no path can separate them,
    // so a positional counter does.
    indices
        .iter()
        .enumerate()
        .map(|(position, &index)| sanitize(&format!("{}-{}", topics[index].1.name, position + 1)))
        .collect()
}

/// Topic names are identifiers, but an Rd `\name` can hold anything; a path
/// separator must not escape the output directory.
fn sanitize(name: &str) -> String {
    name.replace(['/', '\\'], "-")
}

/// Whether a file that turns out not to be documentation is a warning or an
/// error. Scanning a directory is lenient; naming a file is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lenient {
    Yes,
    No,
}

/// Parse one source file with the reader its extension selects.
fn parse_file(path: &Path, lenient: Lenient) -> Result<Vec<Topic>> {
    let source =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    match extension(path).as_deref() {
        Some("Rd") | Some("rd") => Ok(vec![
            r::parse(&source).with_context(|| format!("parsing {}", path.display()))?,
        ]),
        Some("py") => python::parse_module(&source, &path.to_string_lossy(), &python_module(path))
            .map_err(|error| anyhow::anyhow!("{error}"))
            .with_context(|| format!("parsing {}", path.display())),
        Some("typ") => Ok(typ::parse(&source)),
        // A manual directory holds more than pages: a `.so` stub redirects
        // to another entry rather than standing on its own. Scanning a
        // directory skips one; naming it explicitly reports what it is.
        Some(extension) if is_man_extension(extension) => match man::parse(&source) {
            Ok(topic) => Ok(vec![topic]),
            Err(error @ man::ManError::Redirect { .. }) if lenient == Lenient::Yes => {
                eprintln!("warning: skipping {}: {error}", path.display());
                Ok(Vec::new())
            }
            Err(error) => Err(anyhow::anyhow!("{error}"))
                .with_context(|| format!("parsing {}", path.display())),
        },
        _ => bail!(
            "unrecognised input type: {} (expected .Rd, .py, .typ, or a man page section such as .1)",
            path.display()
        ),
    }
}

/// Every recognised file under `dir`, depth first, each directory's entries in
/// name order.
///
/// Documentation nests: a Python package contains subpackages, a Typst package
/// keeps its modules under `src/`. Only R's flat `man/` needs no descent. A
/// subdirectory skipped without a word is indistinguishable from one that held
/// nothing, so the walk descends rather than reporting success over a manual
/// missing half its entries. Unrecognised files are still simply not
/// documentation; a file named explicitly, by contrast, errors.
///
/// Entries whose name begins with `.` are not source. That convention, not a
/// list of directory names to grow forever, is what keeps `.venv`, `.git`, and
/// `.tox` out of a manual. `visited` holds the directories already walked, by
/// canonical path, so a symlink cycle terminates.
fn recognised_files(
    dir: &Path,
    out: &mut Vec<PathBuf>,
    visited: &mut std::collections::HashSet<PathBuf>,
) -> Result<()> {
    let canonical = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !visited.insert(canonical) {
        return Ok(());
    }

    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| !hidden(path))
        .collect();
    entries.sort();

    for entry in entries {
        if entry.is_dir() {
            recognised_files(&entry, out, visited)?;
        } else if recognised(&entry) {
            out.push(entry);
        }
    }
    Ok(())
}

/// Whether a path's own name marks it as not source, by the dot convention.
fn hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

/// The dotted module name of a Python file, from the packages above it.
///
/// Python's own rule: a directory is part of the import path exactly as far up
/// as `__init__.py` reaches, so `mypkg/core.py` is `mypkg.core` and a loose
/// `stats.py` is just `stats`. Unlike Typst, where a module's public path is
/// whatever `lib.typ` re-exports and the layout claims nothing, here the layout
/// really is the import path, so it is worth reading.
///
/// `__init__.py` documents the package, not a module inside it, and is named
/// for its directory.
///
/// The highest `__init__.py` found decides where the package starts, and every
/// directory below it belongs to the path whether it carries one or not: a
/// PEP 420 namespace subpackage has none and is still imported through its
/// parent. Directories above that highest one are somebody's source tree, not
/// part of any import path, so a loose `src/stats.py` stays `stats`.
fn python_module(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut ancestors = Vec::new();
    let mut package_depth = None;
    let mut directory = path.parent();
    while let Some(current) = directory {
        let Some(name) = current.file_name() else {
            break;
        };
        if current.join("__init__.py").is_file() {
            package_depth = Some(ancestors.len());
        }
        ancestors.push(name.to_string_lossy().into_owned());
        directory = current.parent();
    }

    let mut packages = match package_depth {
        Some(depth) => ancestors[..=depth].to_vec(),
        None => Vec::new(),
    };
    packages.reverse();

    if stem != "__init__" {
        packages.push(stem.clone());
    }
    if packages.is_empty() {
        stem
    } else {
        packages.join(".")
    }
}

fn recognised(path: &Path) -> bool {
    match extension(path).as_deref() {
        Some("Rd") | Some("rd") | Some("py") | Some("typ") => true,
        Some(extension) => is_man_extension(extension),
        None => false,
    }
}

/// Whether an extension names a manual section: `1`, `3`, `8`, and the
/// suffixed forms `1p`, `3perl`, `3x`. Also `man`, which packages that ship
/// unnumbered sources use.
///
/// A man page has no distinguishing extension of its own, so this is the one
/// reader selected by a *pattern* rather than a fixed list.
fn is_man_extension(extension: &str) -> bool {
    if extension == "man" {
        return true;
    }
    let mut chars = extension.chars();
    chars.next().is_some_and(|c| c.is_ascii_digit()) && chars.all(|c| c.is_ascii_alphanumeric())
}

fn extension(path: &Path) -> Option<String> {
    Some(path.extension()?.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, name: &str) -> (PathBuf, Topic) {
        (PathBuf::from(path), Topic::new(name))
    }

    #[test]
    fn slugs_are_plain_names_when_unique() {
        let topics = vec![entry("src/a.typ", "on"), entry("src/a.typ", "reveal")];
        assert_eq!(topic_slugs(&topics), vec!["on", "reveal"]);
    }

    #[test]
    fn colliding_slugs_take_the_shortest_distinguishing_path_suffix() {
        // Same stem, so the parent directory is what tells them apart.
        let topics = vec![
            entry("src/component/image.typ", "image"),
            entry("src/layout/image.typ", "image"),
        ];
        assert_eq!(
            topic_slugs(&topics),
            vec!["component-image", "layout-image"]
        );

        // Different stems already suffice, and the topic name is appended.
        let topics = vec![entry("src/a.typ", "on"), entry("src/b.typ", "on")];
        assert_eq!(topic_slugs(&topics), vec!["a-on", "b-on"]);
    }

    #[test]
    fn collisions_within_one_file_fall_back_to_a_counter() {
        let topics = vec![entry("man/x.Rd", "x"), entry("man/x.Rd", "x")];
        assert_eq!(topic_slugs(&topics), vec!["x-1", "x-2"]);
    }

    #[test]
    fn dangling_refs_are_reported_and_resolved_ones_are_not() {
        let topics = vec![entry("a.typ", "a"), entry("b.typ", "b")];
        let rendered = vec![
            "= A <a>\n\nSee @b and @missing.\n".to_owned(),
            "= B <b>\n\nBack to @a.\n".to_owned(),
        ];
        assert_eq!(
            dangling_refs(&topics, &rendered),
            vec![("a".to_owned(), "missing".to_owned())]
        );
    }

    /// A scratch tree, so the `__init__.py` probing has a filesystem to read.
    fn tree(files: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "typst-doc-test-{}",
            files.join("+").replace(['/', '.'], "-")
        ));
        let _ = std::fs::remove_dir_all(&root);
        for file in files {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().expect("a file has a parent"))
                .expect("creating the tree");
            std::fs::write(&path, "").expect("writing a file");
        }
        root
    }

    #[test]
    fn a_module_is_named_under_the_packages_above_it() {
        let root = tree(&["mypkg/__init__.py", "mypkg/core.py"]);
        assert_eq!(python_module(&root.join("mypkg/core.py")), "mypkg.core");
        // `__init__.py` documents the package itself.
        assert_eq!(python_module(&root.join("mypkg/__init__.py")), "mypkg");
    }

    #[test]
    fn a_namespace_subpackage_still_belongs_to_its_parent() {
        let root = tree(&["mypkg/__init__.py", "mypkg/sub/deep.py"]);
        assert_eq!(
            python_module(&root.join("mypkg/sub/deep.py")),
            "mypkg.sub.deep"
        );
    }

    #[test]
    fn a_loose_module_is_named_by_its_stem_alone() {
        let root = tree(&["src/stats.py"]);
        assert_eq!(python_module(&root.join("src/stats.py")), "stats");
    }

    #[test]
    fn the_walk_descends_in_name_order_and_skips_dot_directories() {
        let root = tree(&[
            "pkg/b.py",
            "pkg/a.py",
            "pkg/sub/c.py",
            "pkg/.venv/hidden.py",
            "pkg/notes.txt",
        ]);
        let mut found = Vec::new();
        recognised_files(
            &root.join("pkg"),
            &mut found,
            &mut std::collections::HashSet::new(),
        )
        .expect("walking the tree");
        let names: Vec<String> = found
            .iter()
            .map(|path| {
                path.strip_prefix(&root)
                    .expect("under the root")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, vec!["pkg/a.py", "pkg/b.py", "pkg/sub/c.py"]);
    }

    #[test]
    fn a_shared_name_resolves_from_beside_one_of_its_definitions() {
        let topics = vec![
            entry("src/component/image.typ", "image"),
            entry("src/component/caption.typ", "caption"),
            entry("src/layout/image.typ", "image"),
        ];
        let addresses = Addresses::of(&topics);
        let scopes = scopes(&topics, &addresses);

        // `image` is ambiguous run-wide, so it is absent from the shared map.
        assert!(addresses.ambiguous.contains("image"));

        // Each definition resolves the name to itself, and `caption`, beside
        // one of them, resolves it to its own directory's.
        assert_eq!(scopes[0].get("image"), Some(&"component-image".to_owned()));
        assert_eq!(scopes[1].get("image"), Some(&"component-image".to_owned()));
        assert_eq!(scopes[2].get("image"), Some(&"layout-image".to_owned()));
    }

    #[test]
    fn a_shared_name_stays_unresolved_where_no_definition_is_near() {
        let topics = vec![
            entry("src/component/image.typ", "image"),
            entry("src/layout/image.typ", "image"),
            entry("src/theme/dark.typ", "dark"),
        ];
        let scopes = scopes(&topics, &Addresses::of(&topics));
        assert_eq!(scopes[2].get("image"), None);
    }

    #[test]
    fn author_written_refs_resolve_and_unknown_ones_are_left_alone() {
        let document = "= T <a>\n\nSee @image and @elsewhere.\n\n```typ\n@image\n```\n";
        let resolved = resolve_refs(document, |name| {
            (name == "image").then(|| "component-image".to_owned())
        });

        assert!(resolved.contains("#link(label(\"component-image\"))[#raw(\"image\")]"));
        // A target this run does not define may be the including document's,
        // so it is reported, not rewritten.
        assert!(resolved.contains("@elsewhere"));
        // A `@` inside a raw block is not a reference.
        assert!(resolved.contains("```typ\n@image\n```"));
    }

    #[test]
    fn a_topic_answers_to_its_aliases() {
        let mut topic = Topic::new("printf");
        topic.aliases = vec!["printf".to_owned(), "fprintf".to_owned()];
        // `\name{x}\alias{x}` is ordinary and must not make `x` ambiguous.
        assert_eq!(names_of(&topic), vec!["printf", "fprintf"]);

        let topics = vec![(PathBuf::from("man/printf.3"), topic)];
        assert!(duplicated_names(&topics).is_empty());
    }

    #[test]
    fn two_topics_sharing_an_alias_are_ambiguous() {
        let mut first = Topic::new("a");
        first.aliases = vec!["shared".to_owned()];
        let mut second = Topic::new("b");
        second.aliases = vec!["shared".to_owned()];
        let topics = vec![
            (PathBuf::from("man/a.Rd"), first),
            (PathBuf::from("man/b.Rd"), second),
        ];

        let ambiguous = duplicated_names(&topics);
        assert!(ambiguous.contains("shared"));
        assert!(!ambiguous.contains("a"));
    }
}
