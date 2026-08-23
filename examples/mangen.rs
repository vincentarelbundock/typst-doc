//! Render the man page from the clap command definition.
//!
//! ```console
//! $ cargo run --example mangen             # writes man/typst-doc.1
//! $ cargo run --example mangen -- -        # writes to stdout
//! ```
//!
//! NAME, SYNOPSIS, DESCRIPTION, and OPTIONS come from clap, so they cannot
//! drift from the binary. The sections after OPTIONS have no counterpart in a
//! clap command and are written here, in roff: `after_long_help` would render
//! them as one undifferentiated EXTRA section.

use std::io::Write;
use std::path::PathBuf;

use clap::CommandFactory;
use typst_doc::cli::Cli;

/// Examples are `.EX`/`.EE` blocks so a reader knows they are code and not
/// prose; `SEE ALSO` entries are the usual `name(section)` cross-references.
const TRAILING_SECTIONS: &str = r#".SH EXAMPLES
Convert one R help file and print the Typst to standard output:
.EX
typst\-doc man/mean_ci.Rd
.EE
.PP
Convert a Python module, rendering parameters as a term list:
.EX
typst\-doc stats.py \-\-params terms \-o stats.typ
.EE
.PP
Convert a whole R package's help directory into one manual:
.EX
typst\-doc man/ \-o reference.typ
.EE
.PP
Split a package that documents many functions into one file per topic:
.EX
typst\-doc src/lib.typ \-\-split \-o reference/
.EE
.SH SEE ALSO
.BR typst (1),
.BR man (7),
.BR groff_man (7)
"#;

fn main() -> std::io::Result<()> {
    let man = clap_mangen::Man::new(Cli::command())
        .title("TYPST-DOC")
        .section("1")
        .manual("User Commands");

    let mut page = Vec::new();
    man.render_title(&mut page)?;
    man.render_name_section(&mut page)?;
    man.render_synopsis_section(&mut page)?;
    man.render_description_section(&mut page)?;
    man.render_options_section(&mut page)?;
    page.extend_from_slice(TRAILING_SECTIONS.as_bytes());
    man.render_authors_section(&mut page)?;

    match std::env::args().nth(1).as_deref() {
        Some("-") => std::io::stdout().write_all(&page),
        Some(path) => std::fs::write(path, page),
        None => {
            let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("man")
                .join("typst-doc.1");
            std::fs::create_dir_all(target.parent().expect("man/ has a parent"))?;
            std::fs::write(&target, page)?;
            eprintln!("wrote {}", target.display());
            Ok(())
        }
    }
}
