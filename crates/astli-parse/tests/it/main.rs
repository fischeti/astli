//! The crate's integration tests, one module per file: every test binary links
//! the whole dependency graph, so the crate has one.

mod corpus;
mod generate;
mod grammar;

mod ast;
mod expanded;
mod fuzz;
mod gates;
mod snapshots;
