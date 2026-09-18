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

pub fn run(out: &mut Out, args: &Lex) -> Result {
    let resolved = sources::resolve(&args.sources, &BuildArgs::default())?;
    resolved.warn_unused_build("lex");
    cmd::each(out, &resolved.files, "", |out, file| {
        one(out, file, args.no_trivia)
    })
}

fn one(out: &mut Out, path: &Path, no_trivia: bool) -> Result {
    let text = std::fs::read_to_string(path).map_err(|err| Error::io(path, err))?;

    // No session: one file, no include to follow and no macro to expand. The
    // origin map alone is what turns an offset into something a reader can
    // find, and this is the shape it has to be cheap in.
    let mut origins = Origins::new();
    let file = origins.add_file(path, text);
    let source = origins.text(file);
    let tokens = tokenize(source);

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

    // The property everything downstream leans on, printed where it can be
    // seen rather than only asserted in the tests.
    let rejoined: String = tokens.iter().map(|token| token.text(source)).collect();
    let round_trips = rejoined == source;
    writeln!(out)?;
    writeln!(out, "{} tokens, round-trips: {round_trips}", tokens.len())?;

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
        (true, 0) => Ok(()),
        (false, _) => Err(Error::failed("the token stream does not round-trip")),
        (_, count) => Err(Error::failed(format!("{count} unlexable span(s)"))),
    }
}
