//! Prints the syntax tree for a SystemVerilog file.
//!
//! There is no grammar yet, so every file comes back as one `VERBATIM` node.
//! What the output shows until then is the trivia placement, which is a real
//! decision and the one thing a tree of one node still has.
//!
//!     cargo run --example dump-cst -- file.sv
//!     cargo run --release --example dump-cst -- file.sv --stats

use std::process::ExitCode;
use std::time::Instant;

use rowan::NodeOrToken;
use svirig_syntax::SyntaxNode;
use svirig_syntax::parser::parse;
use svirig_syntax::preproc::Preprocessor;

/// Longer texts are cut short; one block comment is not worth a screen.
const MAX_TEXT: usize = 60;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stats = args.iter().any(|arg| arg == "--stats");
    let Some(path) = args.iter().find(|arg| !arg.starts_with("--")) else {
        eprintln!("usage: dump-cst <file.sv> [--stats]");
        return ExitCode::FAILURE;
    };

    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("{path}: {err}");
            return ExitCode::FAILURE;
        }
    };

    let mut pp = Preprocessor::new();

    // `add` stores the text and lexes it, so this covers both. The parse then
    // reads those tokens back rather than lexing a second time.
    let loaded = Instant::now();
    let file = pp.add(path, text);
    let loading = loaded.elapsed();
    let tokens = pp.tokens(file);
    let source = pp.origins().text(file);

    let started = Instant::now();
    let tree = parse(pp.input(file));
    let parsing = started.elapsed();

    if !stats {
        print(&tree, 0);
    }

    // The invariant the whole tree exists to keep.
    let round_trips = tree.text() == source;
    let (nodes, leaves) = count(&tree);
    println!();
    println!(
        "{} bytes, {} tokens, {nodes} nodes, {leaves} leaves, round-trips: {round_trips}",
        source.len(),
        tokens.len()
    );
    println!(
        "load {:?}, parse {:?} ({:.1} Mtok/s)",
        loading,
        parsing,
        tokens.len() as f64 / parsing.as_secs_f64() / 1e6
    );

    if round_trips {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn print(node: &SyntaxNode, depth: usize) {
    let indent = "  ".repeat(depth);
    println!("{indent}{:?}@{:?}", node.kind(), node.text_range());
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(node) => print(&node, depth + 1),
            NodeOrToken::Token(token) => println!(
                "{indent}  {:?}@{:?} {}",
                token.kind(),
                token.text_range(),
                elide(token.text())
            ),
        }
    }
}

fn count(node: &SyntaxNode) -> (usize, usize) {
    let mut nodes = 1;
    let mut leaves = 0;
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(child) => {
                let (n, l) = count(&child);
                nodes += n;
                leaves += l;
            }
            NodeOrToken::Token(_) => leaves += 1,
        }
    }
    (nodes, leaves)
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
