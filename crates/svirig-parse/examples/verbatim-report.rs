//! What the verbatim rate is *made of*, run by run.
//!
//!     cargo run --release --example verbatim-report -- corpus
//!     cargo run --release --example verbatim-report -- corpus/FlooNoC
//!
//! `metrics` reports the rate and `corpus_verbatim_rate_does_not_rise`
//! asserts it; neither says which rule to write next. This prints one line per
//! [`VERBATIM`] run -- where it starts, how many tokens it cost, and a guess
//! at what put it there -- and then totals those guesses, which is how a
//! milestone picks what to spend itself on.
//!
//! # The causes are a guess, and the lines are the evidence
//!
//! A run carries no record of the rule that gave up on it, so [`cause`] reads
//! the tokens and infers one. It is right often enough to rank the work and
//! wrong often enough that a row worth acting on should be read back in the
//! per-run lines above the table. `other` is the honest bucket: when it grows,
//! the classifier has fallen behind the parser.

use rowan::NodeOrToken;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use svirig_syntax::preproc::Preprocessor;
use svirig_syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode};

/// What a run is most likely to be, from the tokens in it.
///
/// Ordered by what a token settles rather than by how much it costs: a
/// keyword that can only open one construct answers first, and the shapes
/// that need several tokens to tell apart come after.
fn cause(kinds: &[SyntaxKind], inside: SyntaxKind) -> &'static str {
    // A run may open with tokens that say nothing about what it is: an
    // attribute, its own label, or a label the run before it left behind.
    // What follows them does.
    let mut cut = 0;
    loop {
        match kinds.get(cut).copied() {
            // `(* … *)` -- the lexer has no attribute token, so this is the
            // shape rather than a kind.
            Some(L_PAREN) if kinds.get(cut + 1) == Some(&STAR) => {
                cut += 2;
                while cut < kinds.len() && !(kinds[cut] == R_PAREN && kinds[cut - 1] == STAR) {
                    cut += 1;
                }
                cut += 1;
            }
            Some(IDENT | ESCAPED_IDENT) if kinds.get(cut + 1) == Some(&COLON) => cut += 2,
            Some(COLON) if cut == 0 && matches!(kinds.get(1), Some(IDENT | ESCAPED_IDENT)) => {
                cut += 2
            }
            _ => break,
        }
    }

    let has = |kind: SyntaxKind| kinds.contains(&kind);
    // Near the head, for the keywords that mean one thing in a declaration and
    // another a few tokens into a statement.
    let near = |kind: SyntaxKind| kinds[cut..].iter().take(4).any(|&seen| seen == kind);
    let builtin_type = kinds.iter().any(|kind| {
        matches!(
            kind,
            LOGIC_KW
                | BIT_KW
                | INT_KW
                | REG_KW
                | WIRE_KW
                | BYTE_KW
                | SHORTINT_KW
                | LONGINT_KW
                | INTEGER_KW
                | STRING_KW
                | REAL_KW
        )
    });
    // `randomize() with { … }` takes a constraint block where the array
    // methods take an expression, which is what the brace tells apart.
    let randomize_with = has(WITH_KW) && has(L_BRACE);

    match kinds.get(cut).copied().unwrap_or(EOF) {
        COVERGROUP_KW | COVERPOINT_KW | CROSS_KW | ENDGROUP_KW => "covergroup / coverpoint / bins",
        CONSTRAINT_KW => "constraint block",
        BIND_KW => "bind",
        CLOCKING_KW => "clocking block",
        SPECIFY_KW => "specify block",
        PROPERTY_KW | SEQUENCE_KW => "property / sequence declaration",
        ASSERT_KW | ASSUME_KW | COVER_KW | EXPECT_KW => {
            match near(PROPERTY_KW) || near(SEQUENCE_KW) {
                true => "concurrent assertion",
                false => "immediate / deferred assertion",
            }
        }
        RANDCASE_KW | RANDSEQUENCE_KW => "randcase / randsequence",
        INT_LITERAL | DEFAULT_KW => "an arm of a randcase above it",
        DOT => "port connections left by a broken instantiation",
        SYSTEM_IDENT => "system task in item position",
        SEMICOLON if kinds.len() == 1 => "stray `;` after a macro item",
        L_BRACK => "case arm written as a range",
        FORCE_KW | RELEASE_KW => "`force` / `release`",
        MODULE_KW | MACROMODULE_KW => "`(* … *)` attribute before a `module`",
        VOID_KW => match randomize_with {
            true => "`randomize() with { … }`",
            false => "`void'( … )` call",
        },
        _ if inside == CONSTRAINT_DECL => "constraint block",
        _ if randomize_with => "`randomize() with { … }`",
        IDENT | ESCAPED_IDENT if has(HASH) && builtin_type => "type-valued parameter override",
        // No brackets at all is what leaves `foo_t bar;` and nothing else.
        IDENT | ESCAPED_IDENT if !has(L_PAREN) && !has(HASH) => {
            "declaration of an unknown type name"
        }
        _ => "other",
    }
}

/// Every outermost `VERBATIM` in `tree`, and the grammar tokens in and out.
fn runs(tree: &SyntaxNode, tokens: &mut usize, verbatim: &mut usize, found: &mut Vec<SyntaxNode>) {
    fn walk(
        node: &SyntaxNode,
        inside: bool,
        tokens: &mut usize,
        verbatim: &mut usize,
        found: &mut Vec<SyntaxNode>,
    ) {
        let now = inside || node.kind() == VERBATIM;
        // Outermost only: a run nested in another is the same failure counted
        // twice, and its cause is the one above it.
        if now && !inside {
            found.push(node.clone());
        }
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => walk(&node, now, tokens, verbatim, found),
                NodeOrToken::Token(token) if !token.kind().is_trivia() => {
                    *tokens += 1;
                    *verbatim += usize::from(now);
                }
                NodeOrToken::Token(_) => {}
            }
        }
    }

    walk(tree, false, tokens, verbatim, found);
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("sv" | "svh")
        ) {
            out.push(path);
        }
    }
}

fn main() {
    let root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "corpus".to_string());
    let mut paths = Vec::new();
    files(Path::new(&root), &mut paths);
    paths.sort();
    if paths.is_empty() {
        eprintln!("no SystemVerilog under {root} -- run scripts/fetch-corpus.sh");
        std::process::exit(1);
    }

    let mut by_cause: HashMap<&'static str, (usize, usize)> = HashMap::new();
    let (mut tokens, mut verbatim) = (0usize, 0usize);

    for path in &paths {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let mut pp = Preprocessor::new();
        let file = pp.add(path.clone(), text);
        let tree = svirig_parse::parse(pp.input(file));

        let mut found = Vec::new();
        runs(&tree, &mut tokens, &mut verbatim, &mut found);

        for node in found {
            let kinds: Vec<SyntaxKind> = node
                .descendants_with_tokens()
                .filter_map(NodeOrToken::into_token)
                .map(|token| token.kind())
                .filter(|kind| !kind.is_trivia())
                .collect();
            let inside = node.parent().map(|parent| parent.kind()).unwrap_or(EOF);
            let cause = cause(&kinds, inside);

            // One line, so that a row of the table below can be grepped back
            // to the runs that built it.
            let at = pp
                .origins()
                .line_col(file, u32::from(node.text_range().start()));
            let snippet: String = node
                .text()
                .to_string()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(90)
                .collect();
            println!(
                "{}:{}\t{}\t{cause}\t{:?} in {inside:?}\t{snippet}",
                path.display(),
                at.line,
                kinds.len(),
                kinds.first().copied().unwrap_or(EOF),
            );

            let entry = by_cause.entry(cause).or_default();
            entry.0 += 1;
            entry.1 += kinds.len();
        }
    }

    let mut rows: Vec<_> = by_cause.into_iter().collect();
    rows.sort_by_key(|&(cause, (_, tokens))| (std::cmp::Reverse(tokens), cause));

    println!("\n| Tokens | Runs | Share | Cause |\n| ---: | ---: | ---: | --- |");
    for (cause, (runs, in_runs)) in rows {
        println!(
            "| {in_runs} | {runs} | {:.2}% | {cause} |",
            100.0 * in_runs as f64 / tokens as f64
        );
    }
    println!(
        "\n{verbatim} of {tokens} tokens are verbatim ({:.2}%), over {} files.",
        100.0 * verbatim as f64 / tokens as f64,
        paths.len(),
    );
}
