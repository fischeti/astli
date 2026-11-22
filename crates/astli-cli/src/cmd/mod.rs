//! Subcommand implementations, and what they share: the context a command
//! is given ([`Ctx`]), the loop over the packages it acts on ([`members`]),
//! and the runner of its work on each file ([`each`]), with the tally of
//! what failed ([`Outcome`]).

pub mod add;
pub mod bender;
pub mod build;
pub mod cache;
pub mod check;
pub mod clean;
pub mod completion;
pub mod elab;
pub mod fetch;
pub mod files;
pub mod fmt;
pub mod init;
pub mod lex;
pub mod lint;
pub mod lsp;
pub mod parse;
pub mod patch;
pub mod path;
pub mod pickle;
pub mod preprocess;
pub mod remove;
pub mod run;
pub mod snapshot;
pub mod test;
pub mod tree;
pub mod update;

mod each;
mod workspace;

pub use each::{Outcome, Sink, each, tally};
pub use workspace::{choose, members};

use crate::render::Out;
use crate::run::Run;

/// What a subcommand is given to run with.
pub struct Ctx<'a> {
    /// Standard output writer for command results.
    pub out: &'a mut Out,
    /// The run: its flags, such as verbosity and parallel jobs, and what
    /// the packages and designs it checks share.
    pub run: &'a Run<'a>,
}
