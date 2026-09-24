//! `svirig preprocess` subcommand.
//!
//! Runs the SystemVerilog preprocessor and inspects output at different stages
//! of the pipeline (`--emit text`, `tokens`, `origins`, `directives`, or `table`).

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use svirig_preproc::{Arity, ExpandedToken, IncludePath, Input, Item, Operands, TokenSpan, render};
use usage::{Args, RunWith, ValueEnum};

use crate::cli::{BuildArgs, Sources};
use crate::cmd::{self, Ctx};
use crate::error::{Error, Result};
use crate::render::{elide, flat};
use crate::session;
use crate::sources::{self, Build};

/// Preprocess SystemVerilog source files and inspect preprocessor state.
#[derive(Args)]
pub struct Preprocess {
    #[usage(flatten)]
    pub sources: Sources,
    /// Output format to emit
    #[usage(long, value_enum, default = "text")]
    pub emit: Emit,
    #[usage(flatten)]
    pub build: BuildArgs,
}

/// Available preprocessor output formats.
#[derive(ValueEnum, Clone, Copy, PartialEq, Eq)]
pub enum Emit {
    /// Expanded source code with macros replaced and `include` directives resolved
    Text,
    /// Expanded token stream
    Tokens,
    /// Origin mapping tracing each macro-expanded token to its definition
    Origins,
    /// Directives and macro calls found in raw source without expansion
    Directives,
    /// Macro definitions table resulting from preprocessing
    Table,
}

/// Per-file preprocessor counts for summary reporting.
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

        // Use a comment prefix for multi-file headers when emitting expanded source
        // text so the output remains valid SystemVerilog code.
        let prefix = match self.emit {
            Emit::Text => "// ",
            _ => "",
        };

        let quiet = run.quiet;
        let outcome = cmd::each(out, &resolved.files, run, prefix, |sink, file| {
            one(sink, file, self.emit, &resolved.build, quiet)
        })?;

        // Emit summaries for views where item counts are meaningful.
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

/// Outputs an empty line before the summary if output was not quieted.
fn blank(out: &mut dyn Write, quiet: bool) -> Result {
    if !quiet {
        writeln!(out)?;
    }
    Ok(())
}

fn one(
    sink: &mut cmd::Sink,
    path: &Path,
    emit: Emit,
    build: &Build,
    quiet: bool,
) -> Result<Counts> {
    let mut opened = session::open(path, build)?;
    let out = &mut *sink.out;

    let counts = match emit {
        Emit::Text => {
            let tokens = opened.expand().tokens;
            if !quiet {
                write!(out, "{}", render(opened.session.origins(), &tokens))?;
            }
            Ok(Counts::default())
        }
        Emit::Tokens => tokens(out, &mut opened, quiet),
        Emit::Origins => origins(out, &mut opened, quiet),
        Emit::Directives => directives(out, &opened, quiet),
        Emit::Table => table(out, &mut opened, quiet),
    }?;

    // Diagnostics are written to stderr so that redirected stdout remains clean.
    sink.errors += crate::render::diagnostics(
        sink.diagnostics,
        opened.session.origins(),
        opened.diagnostics(),
    )?;

    Ok(counts)
}

/// Prints the expanded preprocessor token stream.
fn tokens(out: &mut dyn Write, opened: &mut session::Opened, quiet: bool) -> Result<Counts> {
    let expanded = opened.expand().tokens;
    let origins = opened.session.origins();

    if !quiet {
        for token in &expanded {
            writeln!(
                out,
                "{:<16} {}",
                format!("{:?}", token.kind),
                elide(origins.slice(token.span))
            )?;
        }
    }

    Ok(Counts {
        tokens: expanded.len(),
        ..Counts::default()
    })
}

/// Prints token origins for tokens produced by macro expansions.
fn origins(out: &mut dyn Write, opened: &mut session::Opened, quiet: bool) -> Result<Counts> {
    if quiet {
        return Ok(Counts::default());
    }

    let expanded = opened.expand().tokens;
    let origins = opened.session.origins();

    let placed = |token: &&ExpandedToken| origins.placed_by(token.span.file).is_some();
    for token in expanded.iter().filter(placed) {
        let spelled = token.span;
        let at = match origins.path(spelled.file) {
            Some(path) => format!(
                "{}:{}",
                path.display(),
                origins.line_col(spelled.file, spelled.start)
            ),
            // Synthesized from macro operators like `"```"` or concatenation ````""````.
            None => "<synthesised>".to_string(),
        };
        let through: Vec<_> = origins
            .trace(spelled.file)
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

/// Scans and prints preprocessor directives and macro calls in the raw input file.
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

/// Prints the final macro definition table after all includes and expansions.
fn table(out: &mut dyn Write, opened: &mut session::Opened, quiet: bool) -> Result<Counts> {
    let table = opened.expand().macros;
    if quiet {
        return Ok(Counts::default());
    }

    // Group definitions by source file, placing command-line defines first followed by files alphabetically.
    let mut files: BTreeMap<(bool, String), Vec<(String, String)>> = BTreeMap::new();
    for (name, entry) in table.iter() {
        let def = &entry.def;
        let input = opened.session.input(def.body.file);
        let origins = opened.session.origins();
        let line = origins
            .line_col(def.body.file, input.token(def.tokens.start).start)
            .line;
        let named = origins
            .path(def.body.file)
            .map(|path| path.display().to_string());

        let arity = match entry.arity {
            Arity::Nullary => String::new(),
            Arity::Formals(count) => format!("/{count}"),
            Arity::Unknown => "/?".to_string(),
        };
        let body = flat(text(&input, def.body));
        let row = format!("{line:>7}  {name}{arity} = {body}");

        let file = named.unwrap_or_else(|| "<synthesised>".to_string());
        files
            .entry((file != session::COMMAND_LINE, file))
            .or_default()
            .push((name.to_string(), row));
    }

    for ((_, file), mut entries) in files {
        entries.sort();
        writeln!(out, "{file}")?;
        for (_, row) in entries {
            writeln!(out, "{}", row.trim_end())?;
        }
    }

    Ok(Counts::default())
}

/// Formats directive operands for display.
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

/// Returns source text covered by the specified token span.
fn text<'a>(input: &Input<'a>, span: TokenSpan) -> &'a str {
    if span.is_empty() {
        return "";
    }
    let start = input.token(span.start).start as usize;
    let end = input.token(span.end - 1).end as usize;
    &input.source[start..end]
}
