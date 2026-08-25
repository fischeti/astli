//! Prints the token stream for a SystemVerilog file.
//!
//! Whitespace and comments are tokens like anything else, because a formatter
//! needs every byte of the input in the tree. `--no-trivia` hides them for when
//! what you are reading is the grammar-visible sequence instead.
//!
//!     cargo run --example dump-tokens -- file.sv --no-trivia

use std::process::ExitCode;

use svirig_syntax::{SyntaxKind, tokenize};
use svirig_text::Origins;

/// Longer texts are cut short; one block comment is not worth a screen.
const MAX_TEXT: usize = 60;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let hide_trivia = args.iter().any(|arg| arg == "--no-trivia");
    let Some(path) = args.iter().find(|arg| !arg.starts_with("--")) else {
        eprintln!("usage: dump-tokens <file.sv> [--no-trivia]");
        return ExitCode::FAILURE;
    };

    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("{path}: {err}");
            return ExitCode::FAILURE;
        }
    };

    // The origin map is what turns an offset into something a reader can find.
    // One file and no expansion here, which is the shape it has to be cheap in.
    let mut origins = Origins::new();
    let file = origins.add_file(path, text);
    let source = origins.text(file);
    let tokens = tokenize(source);

    for token in &tokens {
        if hide_trivia && token.kind.is_trivia() {
            continue;
        }
        println!(
            "{:?}@{}..{} {}",
            token.kind,
            token.start,
            token.end,
            elide(token.text(source))
        );
    }

    // The property everything downstream leans on, restated where it can be
    // seen rather than only asserted in the tests.
    let rejoined: String = tokens.iter().map(|token| token.text(source)).collect();
    let round_trips = rejoined == source;
    println!();
    println!("{} tokens, round-trips: {round_trips}", tokens.len());

    let errors: Vec<_> = tokens
        .iter()
        .filter(|token| token.kind == SyntaxKind::LEX_ERROR)
        .collect();
    if !errors.is_empty() {
        println!("\n{} unlexable span(s):", errors.len());
        for token in &errors {
            let at = origins.line_col(file, token.start);
            println!("  {path}:{at}: {}", elide(token.text(source)));
        }
    }

    if round_trips && errors.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Debug-quoted, so that whitespace is visible rather than printed.
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
