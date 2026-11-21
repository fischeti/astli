//! The crate's integration tests, one module per file: every test binary links
//! the whole dependency graph, so the crate has one.

mod benchmarks;
mod bender;
mod build;
mod command_line;
mod corpus;
mod design;
mod diagnostics;
mod elab;
mod files;
mod fmt;
mod lex_parse;
mod lint;
mod packages;
mod plugin;
mod support;
mod test;
mod workspace;
