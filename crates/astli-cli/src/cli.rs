//! Command-line interface definitions and shared argument groups.
//!
//! Subcommand-specific flags are declared alongside their implementations in
//! [`crate::cmd`]. This module defines the top-level CLI (`astli`), subcommands,
//! and shared arguments reused across commands (input sources, execution/job flags,
//! and preprocessor options).

use std::path::PathBuf;

use usage::{Args, Cli, Subcommands};

use crate::cmd::completion::Completion;
use crate::cmd::files::Files;
use crate::cmd::fmt::Fmt;
use crate::cmd::lex::Lex;
use crate::cmd::parse::Parse;
use crate::cmd::preprocess::Preprocess;

/// SystemVerilog formatting, and a dump of each stage of the pipeline.
#[derive(Cli)]
// Disallow unknown flags so typos are reported immediately instead of
// being mistakenly treated as input file paths.
#[usage(bin = "astli", long_version, completion, unknown_flags = "error")]
pub struct Astli {
    #[usage(subcommand)]
    pub command: Commands,
    #[usage(flatten)]
    pub run: RunArgs,
}

/// Available subcommands (ordered by compilation pipeline stage).
// `run_with` automatically dispatches parsed commands to their execution handlers.
#[derive(Subcommands)]
#[usage(run_with)]
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
    Files(Files),
    #[usage(display_order = 6)]
    Completion(Completion),
}

/// Input source files and filelists.
///
/// Accepts files directly from the command line or via EDA filelists (`.f`).
/// Supports two filelist resolution modes:
/// - `-f`: Resolves relative paths against the current working directory.
/// - `-F`: Resolves relative paths against the filelist's directory.
#[derive(Args, Default)]
pub struct Sources {
    /// Source files to process
    #[usage(arg)]
    pub files: Vec<PathBuf>,
    /// Filelist (.f) with paths relative to current working directory
    #[usage(short = 'f', long = "filelist")]
    pub filelist: Vec<PathBuf>,
    /// Filelist (.f) with paths relative to the filelist itself
    #[usage(short = 'F', long = "filelist-relative", value_name = "FILELIST")]
    pub relative: Vec<PathBuf>,
}

/// Execution options shared across subcommands (output verbosity and parallelism).
///
/// Configured globally so options can be specified before or after the subcommand
/// (e.g. `astli -q parse` or `astli parse -q`).
#[derive(Args, Default)]
pub struct RunArgs {
    /// Suppress file output and only display summary statistics and errors
    #[usage(short = 'q', long, global)]
    pub quiet: bool,
    /// Number of worker threads to run in parallel (0 uses all available CPU cores)
    #[usage(short = 'j', long, default = "0", global)]
    pub jobs: usize,
}

/// Preprocessor configuration: include search paths and macro definitions.
///
/// Accepts standard compiler flags (`-I`, `-D`) as well as EDA simulator plusargs
/// (`+incdir+`, `+define+`) commonly encountered in filelists.
#[derive(Args, Default)]
pub struct BuildArgs {
    /// Add directory to the `include` search path (can be repeated)
    #[usage(short = 'I', long = "incdir")]
    pub incdir: Vec<PathBuf>,
    /// Add include search path(s), separated by '+'
    #[usage(arg, sigil = "+incdir+", value_name = "+incdir+DIR+...")]
    pub incdir_plus: Vec<String>,
    /// Define preprocessor macro as NAME or NAME=VALUE (can be repeated)
    #[usage(short = 'D', long = "define")]
    pub define: Vec<String>,
    /// Define preprocessor macro(s), separated by '+'
    #[usage(arg, sigil = "+define+", value_name = "+define+NAME[=VALUE]+...")]
    pub define_plus: Vec<String>,
}
