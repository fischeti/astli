//! `svirig preprocess` -- what the preprocessor makes of a file.
//!
//! Five views of one pipeline rather than five commands, because they are the
//! same run stopped at different points. `directives` and `table` are the file
//! as written -- the scan, which is what raw mode and the formatter read --
//! and `text`, `tokens` and `origins` are what it means once the macros are
//! gone, which is what a compiler would read.

use std::io::Write;

use svirig_preproc::{Arity, IncludePath, Input, Item, MacroTable, Operands, TokenSpan, render};

use crate::cli::{Emit, Preprocess};
use crate::error::{Error, Result};
use crate::render::{Out, elide, flat};
use crate::session;

pub fn run(out: &mut Out, args: &Preprocess) -> Result {
    let mut opened = session::open(&args.file, &args.build)?;

    match args.emit {
        Emit::Text => {
            let tokens = opened.expand();
            write!(out, "{}", render(opened.session.origins(), &tokens))?;
            Ok(())
        }
        Emit::Tokens => tokens(out, &mut opened),
        Emit::Origins => origins(out, &mut opened),
        Emit::Directives => directives(out, &opened),
        Emit::Table => table(out, &opened),
    }
}

/// The expanded stream itself, which is what the preprocessor actually
/// produces: the text is a rendering of this, not the other way round.
fn tokens(out: &mut Out, opened: &mut session::Opened) -> Result {
    let expanded = opened.expand();
    let origins = opened.session.origins();

    for token in &expanded {
        writeln!(
            out,
            "{:<16} {}",
            format!("{:?}", token.kind),
            elide(origins.slice(token.origin.spelled))
        )?;
    }

    writeln!(out)?;
    writeln!(out, "{} token(s)", expanded.len())?;
    Ok(())
}

/// Where the tokens a macro placed were written.
///
/// Only those. The rest are where the reader left them, and printing those
/// would bury the ones worth looking at.
fn origins(out: &mut Out, opened: &mut session::Opened) -> Result {
    let expanded = opened.expand();
    let origins = opened.session.origins();

    for token in expanded.iter().filter(|token| token.origin.from.is_some()) {
        let spelled = token.origin.spelled;
        let at = match origins.path(spelled.file) {
            Some(path) => format!(
                "{}:{}",
                path.display(),
                origins.line_col(spelled.file, spelled.start)
            ),
            // Pasted or stringified: the bytes are in no file.
            None => "<synthesised>".to_string(),
        };
        let through: Vec<_> = origins
            .trace(token.origin)
            .map(|expansion| origins.slice(expansion.name))
            .collect();

        writeln!(
            out,
            "{:<28} {:<16} {:<20} through {}",
            at,
            format!("{:?}", token.kind),
            format!("{:?}", origins.slice(spelled)),
            through.join(" < ")
        )?;
    }

    Ok(())
}

/// Every directive and macro reference, with the operands or arguments each
/// one carries.
///
/// The question worth asking when a macro call comes out the wrong shape,
/// since a call's shape depends on the table and not on the bytes.
fn directives(out: &mut Out, opened: &session::Opened) -> Result {
    let input = opened.session.input(opened.file);
    let found = opened.session.scan(opened.file);
    let origins = opened.session.origins();

    let mut malformed = 0;
    for item in &found.items {
        let at = origins.line_col(opened.file, input.token(item.tokens().start).start);

        match item {
            Item::Directive(directive) => {
                if directive.operands == Operands::Malformed {
                    malformed += 1;
                }
                writeln!(
                    out,
                    "{:>7}  {:<20} {}",
                    at.to_string(),
                    format!("{:?}", directive.ty),
                    operands(&input, &directive.operands)
                )?;
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
                writeln!(out, "{:>7}  {:<20} {name}{shape}", at.to_string(), "macro")?;
            }
        }
    }

    writeln!(out)?;
    writeln!(
        out,
        "{} directive(s), {} macro reference(s), {} macro(s) defined",
        found.directives().count(),
        found.references().count(),
        found.macros.len()
    )?;

    match malformed {
        0 => Ok(()),
        count => Err(Error::failed(format!("{count} malformed directive(s)"))),
    }
}

/// The macro table the file builds.
///
/// `-D` first and the file second, which is the order they are defined in --
/// a command-line definition acts as though it were written before the first
/// line -- and they are two lists rather than one because an entry's spans
/// address the buffer it was read from.
fn table(out: &mut Out, opened: &session::Opened) -> Result {
    if let Some((file, defines)) = opened.defines() {
        let input = opened.session.input(file);
        entries(out, &input, defines, "-D")?;
    }

    let input = opened.session.input(opened.file);
    let found = opened.session.scan(opened.file);
    entries(out, &input, &found.macros, "")
}

fn entries(out: &mut Out, input: &Input, table: &MacroTable, note: &str) -> Result {
    let mut names: Vec<_> = table.iter().collect();
    names.sort_by_key(|(name, _)| *name);
    for (name, entry) in names {
        let arity = match entry.arity {
            Arity::Nullary => String::new(),
            Arity::Formals(count) => format!("/{count}"),
            // Two definitions that disagreed. An adjacent `(` is read as an
            // argument list wherever this name is used.
            Arity::Unknown => "/?".to_string(),
        };
        writeln!(
            out,
            "  {name}{arity} = {}{}",
            elide(text(input, entry.def.body)),
            match note.is_empty() {
                true => String::new(),
                false => format!("   [{note}]"),
            }
        )?;
    }

    Ok(())
}

fn operands(input: &Input, operands: &Operands) -> String {
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
        Operands::Include(IncludePath::Quoted(at)) => input.text(at.index).to_string(),
        Operands::Include(IncludePath::Angle(name)) => format!("<{}>", text(input, *name)),
        Operands::Include(IncludePath::Expanded(name)) => text(input, *name).to_string(),
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
