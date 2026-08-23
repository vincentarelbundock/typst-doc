//! Convert every `.Rd` or `.py` file in a directory and check that the
//! generated Typst parses. Usage: `cargo run --example validate -- <dir>`.

use man2typst::typst::Options;
use man2typst::{python, r, topic_to_typst};

fn main() {
    let dir = std::env::args().nth(1).expect("usage: validate <dir>");
    let (mut ok, mut bad) = (0usize, 0usize);

    for entry in std::fs::read_dir(&dir).expect("readable directory") {
        let path = entry.expect("readable entry").path();
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let topics = match path.extension().and_then(|e| e.to_str()) {
            Some("Rd" | "rd") => match r::parse(&source) {
                Ok(topic) => vec![topic],
                Err(error) => {
                    println!("PARSE  {}: {error}", path.display());
                    bad += 1;
                    continue;
                }
            },
            Some("py") => python::parse(&source, &path.to_string_lossy()).unwrap_or_default(),
            _ => continue,
        };

        for topic in &topics {
            let output = topic_to_typst(topic, &Options::default());
            let (errors, _) = typst_syntax::parse(&output).errors_and_warnings();
            if errors.is_empty() {
                ok += 1;
            } else {
                bad += 1;
                println!(
                    "TYPST  {} ({}): {:?}",
                    path.display(),
                    topic.name,
                    errors[0]
                );
            }
        }
    }

    println!("valid: {ok}  invalid: {bad}");
    if bad > 0 {
        std::process::exit(1);
    }
}
