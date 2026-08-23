//! Escaping Typst markup and string literals.
//!
//! Ported from rd2qmd (`crates/rd2qmd-mdast/src/typst/mod.rs`), MIT licensed,
//! Copyright (c) 2026 rd2md authors. See NOTICE.md.

/// Escape a Typst string literal, including the surrounding quotes.
pub fn typst_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(character),
        }
    }
    out.push('"');
    out
}

/// Render a Typst array of string literals.
pub fn typst_string_array(values: &[String]) -> String {
    let items: Vec<_> = values.iter().map(|value| typst_string(value)).collect();
    // A one-element Typst array needs a trailing comma to stay an array.
    if items.len() == 1 {
        format!("({},)", items[0])
    } else {
        format!("({})", items.join(", "))
    }
}

/// Escape text for Typst markup, assuming it is not at the start of a line.
pub fn escape_text(value: &str) -> String {
    escape_text_at(value, false)
}

/// Escape text for Typst markup.
///
/// Two classes of character need escaping. The first is special anywhere:
/// `\` (escape), `#` (code), `$` (math), `*`/`_` (strong/emph), `` ` ``
/// (raw), `<`/`>` (labels), `@` (references), `~` (non-breaking space) and
/// `[`/`]` (content blocks), and parentheses (which can call the preceding
/// content expression). The second is special only at the start of a
/// line, where it would begin a heading, list, or term: `=`, `-`, `+`, `/`
/// and a digit run followed by `.`.
pub fn escape_text_at(value: &str, at_line_start: bool) -> String {
    let mut out = String::with_capacity(value.len());
    let mut line_start = at_line_start;

    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if line_start {
            // Leading whitespace does not end the "start of line" state:
            // Typst treats an indented `-` as a list marker too.
            if character == ' ' || character == '\t' {
                out.push(character);
                continue;
            }
            match character {
                '=' | '-' | '+' | '/' => {
                    out.push('\\');
                    out.push(character);
                    line_start = false;
                    continue;
                }
                '0'..='9' => {
                    // A digit run followed by `.` starts an enumeration.
                    let mut digits = String::from(character);
                    while let Some(next) = chars.peek().copied() {
                        if next.is_ascii_digit() {
                            digits.push(next);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    out.push_str(&digits);
                    if chars.peek() == Some(&'.') {
                        chars.next();
                        out.push_str("\\.");
                    }
                    line_start = false;
                    continue;
                }
                _ => {}
            }
        }

        match character {
            // A period immediately following a `#function(..)` expression is
            // parsed as field access. Escaping every prose period is harmless
            // and keeps text safe regardless of its previous sibling.
            '\\' | '#' | '$' | '*' | '_' | '`' | '<' | '>' | '@' | '~' | '[' | ']' | '(' | ')'
            | '.' => {
                out.push('\\');
                out.push(character);
                line_start = false;
            }
            // Typst turns `--` and `---` into en/em dashes. Documentation prose
            // often contains CLI flags and literal numeric ranges, so preserve
            // runs.
            '-' if chars.peek() == Some(&'-') => {
                out.push('\\');
                out.push(character);
                while chars.peek() == Some(&'-') {
                    out.push('\\');
                    out.push(chars.next().expect("peeked hyphen must exist"));
                }
                line_start = false;
            }
            '\n' => {
                out.push('\n');
                line_start = true;
            }
            _ => {
                out.push(character);
                line_start = false;
            }
        }
    }

    out
}

/// Indent every line after the first by `indent` spaces.
pub fn indent_continuation(value: &str, indent: usize) -> String {
    let pad = " ".repeat(indent);
    let mut out = String::with_capacity(value.len());
    for (i, line) in value.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
            if !line.is_empty() {
                out.push_str(&pad);
            }
        }
        out.push_str(line);
    }
    out
}
