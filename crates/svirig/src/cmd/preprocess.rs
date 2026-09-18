//! `svirig preprocess` -- what the preprocessor makes of a file.
//!
//! Five views of one pipeline rather than five commands, because they are the
//! same run stopped at different points. `directives` and `table` are the file
//! as written -- the scan, which is what raw mode and the formatter read --
//! and `text`, `tokens` and `origins` are what it means once the macros are
//! gone, which is what a compiler would read.

use std::io::Write;
use std::path::Path;

use svirig_preproc::{Arity, IncludePath, Input, Item, MacroTable, Operands, TokenSpan, render};
use usage::{Args, RunWith, ValueEnum};

use crate::cli::{BuildArgs, Sources};
use crate::cmd::{self, Ctx};
use crate::error::{Error, Result};
use crate::render::{elide, flat};
use crate::session;
use crate::sources::{self, Build};

/// Print what the preprocessor makes of a file.
#[derive(Args)]
pub struct Preprocess {
    #[usage(flatten)]
    pub sources: Sources,
    /// What to print
    #[usage(long, value_enum, default = "text")]
    pub emit: Emit,
    #[usage(flatten)]
    pub build: BuildArgs,
}

/// What `preprocess` prints.
#[derive(ValueEnum, Clone, Copy, PartialEq, Eq)]
pub enum Emit {
    /// The source with its macros expanded and its `include`s followed
    Text,
    /// The same, as a token stream
    Tokens,
    /// Where each token a macro placed was written
    Origins,
    /// Every directive and macro reference in the file as written
    Directives,
    /// The macro table the file builds
    Table,
}

/// What one file contributed to the run's figures. Which of them mean
/// anything depends on what was asked for; the rest stay zero.
#[derive(Default)]
pub struct Counts {
    tokens: usize,
    directives: usize,
    references: usize,
    macros: usize,
}
impl RunWith<Ctx<'_>> for Preprocess {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        let resolved = sources::resolve(&self.sources, &self.build)?;
        // The expanded source is the one output something else reads, so its
        // heading is a comment and the file it prints is still a file.
        let prefix = match self.emit {
            Emit::Text => "// ",
            _ => "",
        };

        let quiet = run.quiet;
        let outcome = cmd::each(out, &resolved.files, run, prefix, |out, file| {
            one(out, file, self.emit, &resolved.build, quiet)
        })?;

        // Only two of the five views count anything. The expanded source is read
        // by something other than a person and a summary would be noise in it,
        // and the other two are lists whose length says nothing.
        if !outcome.values.is_empty() {
            let sum = |of: fn(&Counts) -> usize| outcome.values.iter().map(of).sum::<usize>();
            match self.emit {
                Emit::Tokens => {
                    blank(out, quiet)?;
                    writeln!(
                        out,
                        "{}, {} token(s)",
                        outcome.files(),
                        sum(|file| file.tokens)
                    )?;
                }
                Emit::Directives => {
                    blank(out, quiet)?;
                    writeln!(
                        out,
                        "{}, {} directive(s), {} macro reference(s), {} macro(s) defined",
                        outcome.files(),
                        sum(|file| file.directives),
                        sum(|file| file.references),
                        sum(|file| file.macros),
                    )?;
                }
                Emit::Text | Emit::Origins | Emit::Table => {}
            }
        }

        outcome.finish()
    }
}

/// The summary is separated from the dump it follows, and there is nothing to
/// separate it from when there was no dump.
fn blank(out: &mut dyn Write, quiet: bool) -> Result {
    if !quiet {
        writeln!(out)?;
    }
    Ok(())
}

fn one(out: &mut dyn Write, path: &Path, emit: Emit, build: &Build, quiet: bool) -> Result<Counts> {
    let mut opened = session::open(path, build)?;

    match emit {
        Emit::Text => {
            let tokens = opened.expand();
            if !quiet {
                write!(out, "{}", render(opened.session.origins(), &tokens))?;
            }
            Ok(Counts::default())
        }
        Emit::Tokens => tokens(out, &mut opened, quiet),
        Emit::Origins => origins(out, &mut opened, quiet),
        Emit::Directives => directives(out, &opened, quiet),
        Emit::Table => table(out, &opened, quiet),
    }
}

/// The expanded stream itself, which is what the preprocessor actually
/// produces: the text is a rendering of this, not the other way round.
fn tokens(out: &mut dyn Write, opened: &mut session::Opened, quiet: bool) -> Result<Counts> {
    let expanded = opened.expand();
    let origins = opened.session.origins();

    if !quiet {
        for token in &expanded {
            writeln!(
                out,
                "{:<16} {}",
                format!("{:?}", token.kind),
                elide(origins.slice(token.origin.spelled))
            )?;
        }
    }

    Ok(Counts {
        tokens: expanded.len(),
        ..Counts::default()
    })
}

/// Where the tokens a macro placed were written.
///
/// Only those. The rest are where the reader left them, and printing those
/// would bury the ones worth looking at.
fn origins(out: &mut dyn Write, opened: &mut session::Opened, quiet: bool) -> Result<Counts> {
    if quiet {
        return Ok(Counts::default());
    }

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

    Ok(Counts::default())
}

/// Every directive and macro reference, with the operands or arguments each
/// one carries.
///
/// The question worth asking when a macro call comes out the wrong shape,
/// since a call's shape depends on the table and not on the bytes.
fn directives(out: &mut dyn Write, opened: &session::Opened, quiet: bool) -> Result<Counts> {
    let input = opened.session.input(opened.file);
    let found = opened.session.scan(opened.file);
    let origins = opened.session.origins();

    let mut malformed = 0;
    for item in &found.items {
        let at = origins.line_col(opened.file, input.token(item.tokens().start).start);

        if directive_is_malformed(item) {
            malformed += 1;
        }
        if quiet {
            continue;
        }

        match item {
            Item::Directive(directive) => {
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

    match malformed {
        0 => Ok(Counts {
            directives: found.directives().count(),
            references: found.references().count(),
            macros: found.macros.len(),
            ..Counts::default()
        }),
        count => Err(Error::failed(format!("{count} malformed directive(s)"))),
    }
}

fn directive_is_malformed(item: &Item) -> bool {
    matches!(item, Item::Directive(directive) if directive.operands == Operands::Malformed)
}

/// The macro table the file builds.
///
/// What a build defined first and the file second, which is the order they are
/// defined in -- a `-D` or a `+define+` acts as though it were written before
/// the first line -- and they are two lists rather than one because an entry's
/// spans address the buffer it was read from.
fn table(out: &mut dyn Write, opened: &session::Opened, quiet: bool) -> Result<Counts> {
    if quiet {
        return Ok(Counts::default());
    }

    if let Some((file, defines)) = opened.defines() {
        let input = opened.session.input(file);
        entries(out, &input, defines, "<command-line>")?;
    }

    let input = opened.session.input(opened.file);
    let found = opened.session.scan(opened.file);
    entries(out, &input, &found.macros, "")?;
    Ok(Counts::default())
}

fn entries(out: &mut dyn Write, input: &Input, table: &MacroTable, note: &str) -> Result {
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
