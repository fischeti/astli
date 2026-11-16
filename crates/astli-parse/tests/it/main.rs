//! The crate's integration tests, one module per file: every test binary links
//! the whole dependency graph, so the crate has one.

mod generate;
mod grammar;
#[path = "../../../astli-syntax/tests/it/ungram.rs"]
mod ungram;

mod ast;
mod expanded;
mod fuzz;
mod gates;
mod snapshots;
