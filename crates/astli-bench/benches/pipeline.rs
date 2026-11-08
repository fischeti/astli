//! Each stage of the pipeline over the same design, so that a change shows in
//! the stage it lands in: lexing, preprocessing, parsing, formatting, lint
//! rules, lowering, and the checks over an elaborated design.
//!
//!     cargo bench -p astli-bench
//!
//! The design is one file of ordinary IP. Lexing, parsing and formatting also
//! run over it repeated, renamed each time so that it stays a valid design,
//! to show how a stage scales with the length of a file.

use std::sync::Arc;

use astli_lint::Config;
use astli_parse::{SyntaxTree, parse_expanded};
use astli_preproc::Session;
use astli_sema::{Design, Elaboration, check, lower};
use divan::{Bencher, black_box};

const DESIGN: &str = include_str!("data/design.sv");

fn main() {
    divan::main();
}

/// The design repeated `copies` times, each copy's definitions renamed.
fn design(copies: usize) -> String {
    (0..copies)
        .map(|copy| DESIGN.replace("bench_", &format!("bench{copy}_")))
        .collect()
}

#[divan::bench(args = [1, 16])]
fn lex(bencher: Bencher, copies: usize) {
    let text = design(copies);
    bencher.bench_local(|| astli_syntax::tokenize(black_box(&text)));
}

#[divan::bench(args = [1, 16])]
fn parse(bencher: Bencher, copies: usize) {
    let text = design(copies);
    bencher
        .with_inputs(|| text.clone())
        .bench_local_values(|text| SyntaxTree::parse("design.sv", text));
}

#[divan::bench]
fn preprocess(bencher: Bencher) {
    bencher
        .with_inputs(|| {
            let mut session = Session::new();
            let file = session.add("design.sv", DESIGN.to_string());
            (session, file)
        })
        .bench_local_refs(|(session, file)| session.expand(*file));
}

#[divan::bench]
fn parse_expanded_design(bencher: Bencher) {
    let mut session = Session::new();
    let file = session.add("design.sv", DESIGN.to_string());
    let expanded = session.expand(file);
    bencher.bench_local(|| parse_expanded(&session, black_box(&expanded.tokens)));
}

#[divan::bench(args = [1, 16])]
fn format(bencher: Bencher, copies: usize) {
    let tree = SyntaxTree::parse("design.sv", design(copies));
    let options = Default::default();
    bencher.bench_local(|| astli_fmt::format(black_box(&tree), &options).unwrap());
}

#[divan::bench]
fn lint(bencher: Bencher) {
    let tree = SyntaxTree::parse("design.sv", DESIGN.to_string());
    let config = Config::default();
    bencher.bench_local(|| astli_lint::lint(black_box(&tree), &config));
}

#[divan::bench]
fn sema_lower(bencher: Bencher) {
    let mut session = Session::new();
    let file = session.add("design.sv", DESIGN.to_string());
    let expanded = session.expand(file);
    let parsed = parse_expanded(&session, &expanded.tokens);
    bencher.bench_local(|| lower(black_box(&parsed)));
}

/// Name resolution, elaboration and the checks, on a fresh design each time,
/// since an elaboration caches what it computes.
#[divan::bench]
fn sema_check(bencher: Bencher) {
    let mut session = Session::new();
    let file = session.add("design.sv", DESIGN.to_string());
    let expanded = session.expand(file);
    let hir = Arc::new(lower(&parse_expanded(&session, &expanded.tokens)));
    bencher
        .with_inputs(|| Elaboration::new(Arc::new(Design::new(vec![hir.clone()]))))
        .bench_local_refs(|elaboration| {
            let (file, _) = elaboration.design().files().next().unwrap();
            check(elaboration, file, None)
        });
}

/// What `astli fmt` does with a file: read it, parse it, format it.
#[divan::bench]
fn parse_and_format(bencher: Bencher) {
    let options = Default::default();
    bencher
        .with_inputs(|| DESIGN.to_string())
        .bench_local_values(|text| {
            let tree = SyntaxTree::parse("design.sv", text);
            astli_fmt::format(&tree, &options).unwrap()
        });
}
