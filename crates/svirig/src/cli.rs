//! The command line, and nothing else.
//!
//! Every `usage` derive in the crate is in this file, so that the commands
//! themselves are ordinary functions over ordinary types. The parser is young
//! and this is a spike; keeping it to one module is what makes replacing it a
//! one-file change rather than a sweep.
//!
//! # What is not here
//!
//! Flags that would not do anything yet. `--color` wants a diagnostic
//! renderer, `-o` wants more plumbing than a shell redirect, and a flag that
//! is accepted and ignored is worse than one that does not exist. The one
//! exception is [`Fmt`], which is declared in order to pin the shape of
//! `--check` and `--write` before there is a formatter behind them.

use std::path::PathBuf;

use usage::{Args, Cli, Subcommands, ValueEnum};

/// SystemVerilog tooling. Each subcommand stops the pipeline one stage later
/// and prints what it has.
#[derive(Cli)]
// `unknown_flags` because the default is to hand an unrecognised word
// through as a positional, and every positional here is a file: a mistyped
// flag would be reported as a file that does not exist.
#[usage(bin = "svirig", version = "0.0.0", completion, unknown_flags = "error")]
pub struct Svirig {
    #[usage(subcommand)]
    pub command: Commands,
}

/// Listed in the order the pipeline runs rather than alphabetically, which is
/// the one thing the list can say about how the stages relate.
#[derive(Subcommands)]
pub enum Commands {
    #[usage(display_order = 1)]
    Lex(Lex),
    #[usage(alias = "pp", display_order = 2)]
    Preprocess(Preprocess),
    #[usage(display_order = 3)]
    Parse(Parse),
    #[usage(display_order = 4)]
    Fmt(Fmt),
    #[usage(display_order = 5)]
    Completion(Completion),
}

/// Print the token stream a file lexes to.
#[derive(Args)]
pub struct Lex {
    #[usage(flatten)]
    pub sources: Sources,
    /// Hide whitespace and comments, leaving what the grammar sees
    #[usage(long)]
    pub no_trivia: bool,
    #[usage(flatten)]
    pub report: Report,
}

/// Print what the preprocessor makes of a file.
#[derive(Args)]
pub struct Preprocess {
    #[usage(flatten)]
    pub sources: Sources,
    /// What to print
    #[usage(long, value_enum, default = "text")]
    pub emit: Emit,
    #[usage(flatten)]
    pub build: BuildArgs,
    #[usage(flatten)]
    pub report: Report,
}

/// What `preprocess` prints.
#[derive(ValueEnum, Clone, Copy, PartialEq, Eq)]
pub enum Emit {
    /// The source with its macros expanded and its `include`s followed
    Text,
    /// The same, as a token stream
    Tokens,
    /// Where each token a macro placed was written
    Origins,
    /// Every directive and macro reference in the file as written
    Directives,
    /// The macro table the file builds
    Table,
}

/// Print the syntax tree a file parses to.
#[derive(Args)]
pub struct Parse {
    #[usage(flatten)]
    pub sources: Sources,
    #[usage(flatten)]
    pub report: Report,
}

/// Format a file. Not implemented.
#[derive(Args)]
pub struct Fmt {
    #[usage(flatten)]
    pub sources: Sources,
    /// Exit non-zero if a file is not already formatted, and write nothing
    #[usage(long)]
    pub check: bool,
    /// Rewrite each file in place instead of printing it
    #[usage(short = 'w', long)]
    pub write: bool,
}

/// Print a shell completion script.
#[derive(Args)]
pub struct Completion {
    /// Which shell to generate for
    #[usage(value_enum)]
    pub shell: Shell,
}

/// The shells `usage` can write a script for.
#[derive(ValueEnum, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

/// What to read.
///
/// Files named on the command line, files named by a filelist, or both. Two
/// flags for a filelist rather than one because a relative path in one has two
/// answers in circulation and the flag is the only place to say which:
/// `docs/limitations.md` has what this does not read.
#[derive(Args, Default)]
pub struct Sources {
    /// The files to read
    #[usage(arg)]
    pub files: Vec<PathBuf>,
    /// A filelist, whose relative paths are relative to the working directory
    #[usage(short = 'f', long = "filelist")]
    pub filelist: Vec<PathBuf>,
    /// A filelist, whose relative paths are relative to the filelist itself
    #[usage(short = 'F')]
    pub relative: Vec<PathBuf>,
}

/// How much of a run to print.
///
/// The summary is not a flag: every command that reads files ends with one,
/// because the figure worth having is over the run and not over a file. What
/// `--quiet` drops is the dump in front of it, which is what a timing run and
/// a "which of these 400 files is broken" run both want.
#[derive(Args, Default)]
pub struct Report {
    /// Print only the summary, not the files themselves
    #[usage(short = 'q', long)]
    pub quiet: bool,
}

/// What a build passes: where an `include` looks, and what is defined before
/// the first line.
///
/// One struct rather than two flags in two places, because they arrive from
/// the same place -- a filelist, a manifest, a command line -- and neither is
/// a property of the file. `docs/api.md` has the shape this grows into.
///
/// Only `preprocess` takes it as flags. `lex` has no use for either, and raw
/// mode -- what `parse` reads -- cannot be handed a seeded table yet, so
/// offering them there would be offering flags that do nothing. A filelist
/// carries them to every command all the same, since a filelist is not
/// written per command; what those commands do is say so and read on.
#[derive(Args, Default)]
pub struct BuildArgs {
    /// A directory to search for `include "..."`, repeatable
    #[usage(short = 'I', long = "incdir")]
    pub incdir: Vec<PathBuf>,
    /// A macro defined before the first line: NAME or NAME=VALUE, repeatable
    #[usage(short = 'D', long = "define")]
    pub define: Vec<String>,
}
