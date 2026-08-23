use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};

use man2typst::typst::{Options, ParamsFormat};
use man2typst::{python, r, topic_to_typst};

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
    about = "Render R and Python API documentation as Typst"
)]
struct Cli {
    /// Input file: an `.Rd` file, or a `.py` file.
    input: PathBuf,

    /// Output file. Defaults to stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// How to render the parameter list.
    #[arg(long, value_enum, default_value_t = Params::Table)]
    params: Params,

    /// Heading level for the topic title.
    #[arg(long, default_value_t = 1)]
    base_level: u8,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let source = std::fs::read_to_string(&cli.input)
        .with_context(|| format!("reading {}", cli.input.display()))?;

    let options = Options {
        params_format: cli.params.into(),
        base_level: cli.base_level,
    };

    let topics = match extension(&cli.input).as_deref() {
        Some("Rd") | Some("rd") => {
            vec![r::parse(&source).with_context(|| format!("parsing {}", cli.input.display()))?]
        }
        Some("py") => python::parse(&source, &cli.input.to_string_lossy())
            .map_err(|error| anyhow::anyhow!("{error}"))
            .with_context(|| format!("parsing {}", cli.input.display()))?,
        _ => bail!(
            "unrecognised input type: {} (expected .Rd or .py)",
            cli.input.display()
        ),
    };

    if topics.is_empty() {
        bail!("no documented entities found in {}", cli.input.display());
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

fn extension(path: &Path) -> Option<String> {
    Some(path.extension()?.to_string_lossy().into_owned())
}
