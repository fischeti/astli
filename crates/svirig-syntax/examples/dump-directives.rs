//! Prints what the preprocessor makes of a SystemVerilog file: every directive
//! and every macro reference, with the operands or arguments each one carries.
//!
//! The counterpart to `dump-tokens` one level up. Where that one answers "how
//! did this lex", this answers "what did the preprocessor decide this is" --
//! which is the question worth asking when a macro call comes out the wrong
//! shape, since a call's shape depends on the table and not on the bytes.
//!
//!     cargo run --example dump-directives -- file.sv --table

use std::process::ExitCode;

use svirig_syntax::preproc::{Arity, Item, Operands, scan};
use svirig_syntax::{Token, tokenize};
use svirig_text::Origins;

/// Longer texts are cut short; one macro body is not worth a screen.
const MAX_TEXT: usize = 60;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let show_table = args.iter().any(|arg| arg == "--table");
    let Some(path) = args.iter().find(|arg| !arg.starts_with("--")) else {
        eprintln!("usage: dump-directives <file.sv> [--table]");
        return ExitCode::FAILURE;
    };

    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) => {
            eprintln!("{path}: {err}");
            return ExitCode::FAILURE;
        }
    };

    let mut origins = Origins::new();
    let file = origins.add_file(path, contents);
    let source = origins.text(file);
    let tokens = tokenize(source);
    let found = scan(source, &tokens);

    let mut malformed = 0;
    for item in &found.items {
        let at = origins.line_col(file, tokens[item.tokens().start as usize].start);

        match item {
            Item::Directive(directive) => {
                if directive.operands == Operands::Malformed {
                    malformed += 1;
                }
                println!(
                    "{:>7}  {:<20} {}",
                    at.to_string(),
                    format!("{:?}", directive.ty),
                    operands(source, &tokens, &directive.operands)
                );
            }
            Item::Macro(reference) => {
                let name = tokens[reference.name as usize].text(source);
                let shape = match &reference.args {
                    Some(args) => {
                        let args: Vec<_> = args
                            .iter()
                            .map(|arg| flat(text(source, &tokens, arg)))
                            .collect();
                        format!("({})", args.join(", "))
                    }
                    None => String::new(),
                };
                println!("{:>7}  {:<20} {name}{shape}", at.to_string(), "macro");
            }
        }
    }

    println!();
    println!(
        "{} directive(s), {} macro reference(s), {} macro(s) defined",
        found.directives().count(),
        found.references().count(),
        found.macros.len()
    );

    if show_table {
        let mut names: Vec<_> = found.macros.iter().collect();
        names.sort_by_key(|(name, _)| *name);
        println!();
        for (name, entry) in names {
            let arity = match entry.arity {
                Arity::Nullary => String::new(),
                Arity::Formals(count) => format!("/{count}"),
                // Two definitions that disagreed. An adjacent `(` is read as an
                // argument list wherever this name is used.
                Arity::Unknown => "/?".to_string(),
            };
            println!(
                "  {name}{arity} = {}",
                elide(text(source, &tokens, &entry.def.body))
            );
        }
    }

    if malformed == 0 {
        ExitCode::SUCCESS
    } else {
        eprintln!("\n{malformed} malformed directive(s)");
        ExitCode::FAILURE
    }
}

fn operands(source: &str, tokens: &[Token], operands: &Operands) -> String {
    use svirig_syntax::preproc::IncludePath::*;

    match operands {
        Operands::Define(def) => {
            let name = tokens[def.name as usize].text(source);
            let formals = match &def.formals {
                Some(formals) => {
                    let names: Vec<_> = formals
                        .iter()
                        .map(|formal| tokens[formal.name as usize].text(source))
                        .collect();
                    format!("({})", names.join(", "))
                }
                None => String::new(),
            };
            format!(
                "{name}{formals} = {}",
                elide(text(source, tokens, &def.body))
            )
        }
        Operands::Name(at) => tokens[*at as usize].text(source).to_string(),
        Operands::Include(Quoted(at)) => tokens[*at as usize].text(source).to_string(),
        Operands::Include(Angle(name)) => format!("<{}>", text(source, tokens, name)),
        Operands::Include(Expanded(name)) => text(source, tokens, name).to_string(),
        Operands::Bare => String::new(),
        Operands::Unparsed(rest) => elide(text(source, tokens, rest)),
        Operands::Malformed => "<malformed>".to_string(),
    }
}

/// The source a token range covers, whitespace between tokens included.
fn text<'a>(source: &'a str, tokens: &[Token], range: &std::ops::Range<u32>) -> &'a str {
    if range.is_empty() {
        return "";
    }
    let start = tokens[range.start as usize].start as usize;
    let end = tokens[range.end as usize - 1].end as usize;
    &source[start..end]
}

/// An argument on one line. Arguments are read for their shape rather than
/// their bytes, and a wrapped argument list is the common case in macro-heavy
/// code, so the newlines go.
fn flat(text: &str) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(MAX_TEXT) {
        Some((at, _)) => format!("{}...", &flat[..at]),
        None => flat,
    }
}

/// Debug-quoted, so that a continuation is visible rather than printed.
fn elide(text: &str) -> String {
    if text.len() <= MAX_TEXT {
        return format!("{text:?}");
    }
    let mut end = MAX_TEXT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{:?}...", &text[..end])
}
