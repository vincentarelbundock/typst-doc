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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let options = Options {
        params_format: cli.params.into(),
        base_level: cli.base_level,
    };

    let mut topics = Vec::new();
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
            for file in &files {
                topics.extend(parse_file(file)?);
            }
        } else {
            topics.extend(parse_file(input)?);
        }
    }

    let found = topics.len();
    if !cli.include_internal {
        topics.retain(|topic| !topic.is_internal());
    }

    if topics.is_empty() {
        if found > 0 {
            bail!("only internal topics found; pass --include-internal to render them");
        }
        bail!("no documented entities found");
    }

    let rendered: Vec<String> = topics
        .iter()
        .map(|topic| topic_to_typst(topic, &options))
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
