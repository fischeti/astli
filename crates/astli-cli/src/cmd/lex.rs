//! `astli lex` subcommand.
//!
//! Tokenizes input SystemVerilog source files and prints the resulting token stream.
//! Preserves all tokens including whitespace and comments unless `--no-trivia` is set.

use std::io::Write;
use std::path::Path;

use astli_syntax::{SyntaxKind, tokenize};
use astli_text::Origins;
use usage::{Args, RunWith};

use crate::cli::{BuildArgs, Sources};
use crate::cmd::{self, Ctx};
use crate::error::{Error, Result};
use crate::render::elide;
use crate::sources;

/// Tokenize SystemVerilog source files and print tokens.
#[derive(Args)]
pub struct Lex {
    #[usage(flatten)]
    pub sources: Sources,
    /// Omit trivia tokens (whitespace and comments) from output
    #[usage(long)]
    pub no_trivia: bool,
}

/// Tokenization statistics for a single file.
pub struct Stats {
    bytes: usize,
    tokens: usize,
}

impl RunWith<Ctx<'_>> for Lex {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        let resolved = sources::resolve(&self.sources, &BuildArgs::default())?;
        resolved.warn_unused_build("lex");

        let quiet = run.quiet;
        let outcome = cmd::each(out, &resolved.files, run, "", |sink, file| {
            one(sink.out, file, self.no_trivia, quiet)
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
}

fn one(out: &mut dyn Write, path: &Path, no_trivia: bool, quiet: bool) -> Result<Stats> {
    let text = std::fs::read_to_string(path).map_err(|err| Error::io(path, err))?;

    // Direct lexing of a single file does not require preprocessor session state;
    // an Origins map suffices for line and column mapping.
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

    // Validate lossless tokenization: concatenating all tokens must match the input exactly.
    let rejoined: String = tokens.iter().map(|token| token.text(source)).collect();
    let round_trips = rejoined == source;

    // Report any lexical errors found in the file.
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
