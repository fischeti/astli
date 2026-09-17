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

use svirig_preproc::{Arity, Input, Item, Operands, Preprocessor, TokenSpan, scan};

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

    let mut pp = Preprocessor::new();
    let file = pp.add(path, contents);
    let input = pp.input(file);
    let found = scan(&input);

    let mut malformed = 0;
    for item in &found.items {
        let at = pp
            .origins()
            .line_col(file, input.token(item.tokens().start).start);

        match item {
            Item::Directive(directive) => {
                if directive.operands == Operands::Malformed {
                    malformed += 1;
                }
                println!(
                    "{:>7}  {:<20} {}",
                    at.to_string(),
                    format!("{:?}", directive.ty),
                    operands(&input, &directive.operands)
                );
            }
            Item::Macro(reference) => {
                let name = input.text(reference.name.index);
                let shape = match &reference.args {
                    Some(args) => {
                        let args: Vec<_> =
                            args.iter().map(|arg| flat(text(&input, *arg))).collect();
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
            println!("  {name}{arity} = {}", elide(text(&input, entry.def.body)));
        }
    }

    if malformed == 0 {
        ExitCode::SUCCESS
    } else {
        eprintln!("\n{malformed} malformed directive(s)");
        ExitCode::FAILURE
    }
}

fn operands(input: &Input, operands: &Operands) -> String {
    use svirig_preproc::IncludePath::*;

    match operands {
        Operands::Define(def) => {
            let name = input.text(def.name.index);
            let formals = match &def.formals {
                Some(formals) => {
                    let names: Vec<_> = formals
                        .iter()
                        .map(|formal| input.text(formal.name.index))
                        .collect();
                    format!("({})", names.join(", "))
                }
                None => String::new(),
            };
            format!("{name}{formals} = {}", elide(text(input, def.body)))
        }
        Operands::Name(at) => input.text(at.index).to_string(),
        Operands::Include(Quoted(at)) => input.text(at.index).to_string(),
        Operands::Include(Angle(name)) => format!("<{}>", text(input, *name)),
        Operands::Include(Expanded(name)) => text(input, *name).to_string(),
        Operands::Bare => String::new(),
        Operands::Unparsed(rest) => elide(text(input, *rest)),
        Operands::Malformed => "<malformed>".to_string(),
    }
}

/// The source a token range covers, whitespace between tokens included.
fn text<'a>(input: &Input<'a>, span: TokenSpan) -> &'a str {
    if span.is_empty() {
        return "";
    }
    let start = input.token(span.start).start as usize;
    let end = input.token(span.end - 1).end as usize;
    &input.source[start..end]
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
