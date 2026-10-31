//! The crate's integration tests, one module per file: every test binary links
//! the whole dependency graph, so the crate has one.

mod conditional;
mod diagnostics;
mod differential;
mod directive;
mod expand;
mod include;
mod macros;
