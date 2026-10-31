//! The crate's integration tests, one module per file: every test binary links
//! the whole dependency graph, so the crate has one.

mod corpus;
#[path = "../../../astli-parse/tests/it/generate.rs"]
mod generate;

mod fuzz;
mod gates;
mod snapshots;
