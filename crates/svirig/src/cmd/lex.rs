//! `svirig lex` -- how a file lexes.
//!
//! Whitespace and comments are tokens like anything else, because a formatter
//! needs every byte of the input in the tree. `--no-trivia` hides them for
//! when what is being read is the grammar-visible sequence instead.

use std::io::Write;
use std::path::Path;

use svirig_syntax::{SyntaxKind, tokenize};
use svirig_text::Origins;

use crate::cli::{BuildArgs, Lex};
use crate::cmd;
use crate::error::{Error, Result};
use crate::render::{Out, elide};
use crate::sources;

/// What one file contributed to the run's figures.
pub struct Stats {
    bytes: usize,
    tokens: usize,
}

pub fn run(out: &mut Out, args: &Lex) -> Result {
    let resolved = sources::resolve(&args.sources, &BuildArgs::default())?;
    resolved.warn_unused_build("lex");

    let quiet = args.report.quiet;
    let outcome = cmd::each(out, &resolved.files, (!quiet).then_some(""), |out, file| {
        one(out, file, args.no_trivia, quiet)
    })?;

    if !outcome.values.is_empty() {
        if !quiet {
            writeln!(out)?;
        }
        writeln!(
            out,
            "{}, {} bytes, {} tokens, round-trips: true",
            outcome.files(),
            outcome.values.iter().map(|file| file.bytes).sum::<usize>(),
            outcome.values.iter().map(|file| file.tokens).sum::<usize>(),
        )?;
    }

    outcome.finish()
}

fn one(out: &mut dyn Write, path: &Path, no_trivia: bool, quiet: bool) -> Result<Stats> {
    let text = std::fs::read_to_string(path).map_err(|err| Error::io(path, err))?;

    // No session: one file, no include to follow and no macro to expand. The
    // origin map alone is what turns an offset into something a reader can
    // find, and this is the shape it has to be cheap in.
    let mut origins = Origins::new();
    let file = origins.add_file(path, text);
    let source = origins.text(file);
    let tokens = tokenize(source);

    if !quiet {
        for token in &tokens {
            if no_trivia && token.kind.is_trivia() {
                continue;
            }
            writeln!(
                out,
                "{:?}@{}..{} {}",
                token.kind,
                token.start,
                token.end,
                elide(token.text(source))
            )?;
        }
    }

    // The property everything downstream leans on. It is checked per file and
    // reported per run, because what a reader wants to know is that it held
    // everywhere -- and where it did not, the file is a failure and says so.
    let rejoined: String = tokens.iter().map(|token| token.text(source)).collect();
    let round_trips = rejoined == source;

    // Not part of the dump: a span that will not lex is the reason to run this
    // command at all, so it is printed even when the tokens are not.
    let errors: Vec<_> = tokens
        .iter()
        .filter(|token| token.kind == SyntaxKind::LEX_ERROR)
        .collect();
    if !errors.is_empty() {
        writeln!(out, "\n{} unlexable span(s):", errors.len())?;
        for token in &errors {
            let at = origins.line_col(file, token.start);
            writeln!(
                out,
                "  {}:{at}: {}",
                path.display(),
                elide(token.text(source))
            )?;
        }
    }

    match (round_trips, errors.len()) {
        (true, 0) => Ok(Stats {
            bytes: source.len(),
            tokens: tokens.len(),
        }),
        (false, _) => Err(Error::failed("the token stream does not round-trip")),
        (_, count) => Err(Error::failed(format!("{count} unlexable span(s)"))),
    }
}
