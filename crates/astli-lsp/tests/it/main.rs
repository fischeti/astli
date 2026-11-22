//! The crate's integration tests, one module per file: every test binary links
//! the whole dependency graph, so the crate has one.
//!
//! Each drives a server as a client drives it, over an in-memory connection,
//! with a host of its own in place of the driver's.

mod actions;
mod buffers;
mod definition;
mod diagnostics;
mod formatting;
mod hover;
mod references;
mod server;
mod snapshots;
mod support;
mod symbols;
mod tokens;
mod watched;
mod workspace_symbols;
