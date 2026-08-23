use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};

use man2typst::typst::{Options, ParamsFormat};
use man2typst::{Topic, python, r, topic_to_typst, typ};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Params {
    Table,
    Terms,
}

impl From<Params> for ParamsFormat {
    fn from(value: Params) -> Self {
        match value {
            Params::Table => Self::Table,
            Params::Terms => Self::Terms,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "man2typst",
    version,
    about = "Render R, Python, and Typst API documentation as Typst"
)]
struct Cli {
    /// Input `.Rd`, `.py`, or `.typ` files, or directories of them. Topics
    /// are joined into one document, in the order given; a directory
    /// contributes its recognised files in name order.
    #[arg(required = true)]
    inputs: Vec<PathBuf>,

    /// Output file. Defaults to stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// How to render the parameter list.
    #[arg(long, value_enum, default_value_t = Params::Table)]
    params: Params,

    /// Heading level for the topic title.
    #[arg(long, default_value_t = 1)]
    base_level: u8,

    /// Include internal topics: `\keyword{internal}` in R (the signal
    /// pkgdown filters on), and `_`-prefixed names in Python. Skipped by
    /// default. Typst `_` definitions are always private.
    #[arg(long)]
    include_internal: bool,

    /// Write one `<topic>.typ` file per topic into the --output directory
    /// (created if missing) instead of joining everything into one document.
    /// Topics sharing a name are disambiguated by their source path, with a
    /// warning.
    #[arg(long, requires = "output")]
    split: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let options = Options {
        params_format: cli.params.into(),
        base_level: cli.base_level,
    };

    // Each topic keeps its source path: `--split` falls back to it when two
    // topics share a name.
    let mut topics: Vec<(PathBuf, Topic)> = Vec::new();
    for input in &cli.inputs {
        if input.is_dir() {
            // Unrecognised files in a directory are simply not documentation;
            // a file named explicitly, by contrast, errors below.
            let mut files: Vec<PathBuf> = std::fs::read_dir(input)
                .with_context(|| format!("reading {}", input.display()))?
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| recognised(path))
                .collect();
            files.sort();
            for file in files {
                let parsed = parse_file(&file)?;
                topics.extend(parsed.into_iter().map(|topic| (file.clone(), topic)));
            }
        } else {
            let parsed = parse_file(input)?;
            topics.extend(parsed.into_iter().map(|topic| (input.clone(), topic)));
        }
    }

    let found = topics.len();
    if !cli.include_internal {
        topics.retain(|(_, topic)| !topic.is_internal());
    }

    if topics.is_empty() {
        if found > 0 {
            bail!("only internal topics found; pass --include-internal to render them");
        }
        bail!("no documented entities found");
    }

    if cli.split {
        let dir = cli.output.as_ref().expect("clap enforces --output");
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        for ((_, topic), file_name) in topics.iter().zip(split_file_names(&topics)) {
            let file = dir.join(file_name);
            std::fs::write(&file, topic_to_typst(topic, &options))
                .with_context(|| format!("writing {}", file.display()))?;
        }
        return Ok(());
    }

    let rendered: Vec<String> = topics
        .iter()
        .map(|(_, topic)| topic_to_typst(topic, &options))
        .collect();
    let document = rendered.join("\n");

    match &cli.output {
        Some(path) => {
            std::fs::write(path, document).with_context(|| format!("writing {}", path.display()))?
        }
        None => print!("{document}"),
    }
    Ok(())
}

/// One output file name per topic for `--split`, index-aligned with `topics`.
///
/// The usual name is `<topic>.typ`, but topics sharing a name would silently
/// overwrite each other. A colliding group instead takes the shortest suffix
/// of each source path that tells its members apart — mosaic's two `image`
/// functions become `component-image.typ` and `layout-image.typ` — and a
/// warning reports the choice. The suffix disambiguates the file on disk; it
/// makes no claim about the function's qualified name.
fn split_file_names(topics: &[(PathBuf, Topic)]) -> Vec<String> {
    let mut groups: std::collections::BTreeMap<&str, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (index, (_, topic)) in topics.iter().enumerate() {
        groups.entry(topic.name.as_str()).or_default().push(index);
    }

    let mut names = vec![String::new(); topics.len()];
    for (name, indices) in groups {
        if let [index] = indices[..] {
            names[index] = format!("{}.typ", sanitize(name));
            continue;
        }
        let files: Vec<String> = disambiguate(&indices, topics)
            .into_iter()
            .map(|stem| format!("{stem}.typ"))
            .collect();
        eprintln!(
            "warning: {} topics named `{name}`; writing {}",
            indices.len(),
            files.join(", ")
        );
        for (&index, file) in indices.iter().zip(files) {
            names[index] = file;
        }
    }
    names
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

/// Parse one source file with the reader its extension selects.
fn parse_file(path: &Path) -> Result<Vec<Topic>> {
    let source =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    match extension(path).as_deref() {
        Some("Rd") | Some("rd") => Ok(vec![
            r::parse(&source).with_context(|| format!("parsing {}", path.display()))?,
        ]),
        Some("py") => python::parse(&source, &path.to_string_lossy())
            .map_err(|error| anyhow::anyhow!("{error}"))
            .with_context(|| format!("parsing {}", path.display())),
        Some("typ") => Ok(typ::parse(&source)),
        _ => bail!(
            "unrecognised input type: {} (expected .Rd, .py, or .typ)",
            path.display()
        ),
    }
}

fn recognised(path: &Path) -> bool {
    matches!(
        extension(path).as_deref(),
        Some("Rd") | Some("rd") | Some("py") | Some("typ")
    )
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
    fn split_names_are_plain_when_unique() {
        let topics = vec![entry("src/a.typ", "on"), entry("src/a.typ", "reveal")];
        assert_eq!(split_file_names(&topics), vec!["on.typ", "reveal.typ"]);
    }

    #[test]
    fn split_collisions_take_the_shortest_distinguishing_path_suffix() {
        // Same stem, so the parent directory is what tells them apart.
        let topics = vec![
            entry("src/component/image.typ", "image"),
            entry("src/layout/image.typ", "image"),
        ];
        assert_eq!(
            split_file_names(&topics),
            vec!["component-image.typ", "layout-image.typ"]
        );

        // Different stems already suffice, and the topic name is appended.
        let topics = vec![entry("src/a.typ", "on"), entry("src/b.typ", "on")];
        assert_eq!(split_file_names(&topics), vec!["a-on.typ", "b-on.typ"]);
    }

    #[test]
    fn split_collisions_within_one_file_fall_back_to_a_counter() {
        let topics = vec![entry("man/x.Rd", "x"), entry("man/x.Rd", "x")];
        assert_eq!(split_file_names(&topics), vec!["x-1.typ", "x-2.typ"]);
    }
}
