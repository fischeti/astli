//! Command-line interface definitions and shared argument groups.
//!
//! Subcommand-specific flags are declared alongside their implementations in
//! [`crate::cmd`]. This module defines the top-level CLI (`astli`), subcommands,
//! and shared arguments reused across commands (input sources, execution/job flags,
//! and preprocessor options).

use std::path::PathBuf;

use usage_rs::{Args, Cli, Subcommands, ValueEnum};

use crate::cmd::cache::Cache;
use crate::cmd::check::Check;
use crate::cmd::completion::Completion;
use crate::cmd::elab::Elab;
use crate::cmd::fetch::Fetch;
use crate::cmd::files::Files;
use crate::cmd::fmt::Fmt;
use crate::cmd::init::Init;
use crate::cmd::lex::Lex;
use crate::cmd::lint::Lint;
use crate::cmd::parse::Parse;
use crate::cmd::patch::Patch;
use crate::cmd::path::Path;
use crate::cmd::pickle::Pickle;
use crate::cmd::preprocess::Preprocess;
use crate::cmd::snapshot::Snapshot;
use crate::cmd::tree::Tree;
use crate::cmd::update::Update;

/// Modern SystemVerilog toolchain: format, lint, check, elaborate, and manage dependencies.
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

/// Available subcommands for single files, whole designs, and dependencies.
// `run_with` automatically dispatches parsed commands to their execution handlers.
#[derive(Subcommands)]
#[usage(run_with)]
pub enum Commands {
    #[usage(help_heading = "Source", display_order = 1)]
    Fmt(Fmt),
    #[usage(help_heading = "Source", display_order = 2)]
    Lint(Lint),
    #[usage(help_heading = "Design", display_order = 3)]
    Check(Check),
    #[usage(help_heading = "Design", display_order = 4)]
    Files(Files),
    #[usage(help_heading = "Design", display_order = 5)]
    Pickle(Pickle),
    #[usage(help_heading = "Design", display_order = 6)]
    Elab(Elab),
    #[usage(help_heading = "Dependencies", display_order = 7)]
    Init(Init),
    #[usage(help_heading = "Dependencies", display_order = 8)]
    Update(Update),
    #[usage(alias = "checkout", help_heading = "Dependencies", display_order = 9)]
    Fetch(Fetch),
    #[usage(help_heading = "Dependencies", display_order = 10)]
    Tree(Tree),
    #[usage(help_heading = "Dependencies", display_order = 11)]
    Path(Path),
    #[usage(alias = "clone", help_heading = "Dependencies", display_order = 12)]
    Patch(Patch),
    #[usage(help_heading = "Dependencies", display_order = 13)]
    Snapshot(Snapshot),
    #[usage(help_heading = "Dependencies", display_order = 14)]
    Cache(Cache),
    #[usage(display_order = 15)]
    Completion(Completion),
    #[usage(hide)]
    Lex(Lex),
    #[usage(alias = "pp", hide)]
    Preprocess(Preprocess),
    #[usage(hide)]
    Parse(Parse),
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

/// Feature selection flags for package-mode execution.
#[derive(Args, Default)]
pub struct FeatureArgs {
    /// Enable a package feature (can be repeated)
    #[usage(long, value_name = "FEATURE")]
    pub features: Vec<String>,
    /// Enable all package features by default; pass `--no-features` to disable them unless explicitly selected
    #[usage(long, negate = "--no-features", default = "true")]
    pub all_features: bool,
}

impl FeatureArgs {
    /// What the flags select.
    pub fn selection(&self) -> astli_package::Selection {
        astli_package::Selection {
            no_features: !self.all_features,
            features: self.features.clone(),
        }
    }

    /// Whether the flags select anything other than the default.
    pub fn given(&self) -> bool {
        !self.all_features || !self.features.is_empty()
    }
}

/// Execution options shared across subcommands (output verbosity, diagnostics, and parallelism).
///
/// Configured globally so options can be specified before or after the subcommand
/// (e.g. `astli -q parse` or `astli parse -q`).
#[derive(Args, Default)]
pub struct RunArgs {
    /// Suppress informational output and print only errors or explicit results
    #[usage(short = 'q', long, global)]
    pub quiet: bool,
    /// Number of worker threads to run in parallel (0 uses all available CPU cores)
    #[usage(short = 'j', long, default = "0", global, env = "ASTLI_JOBS")]
    pub jobs: usize,
    /// Diagnostic output format
    #[usage(
        long = "diag",
        value_enum,
        value_name = "FMT",
        default = "pretty",
        global,
        env = "ASTLI_DIAGNOSTICS"
    )]
    pub diagnostics: DiagnosticFormat,
    /// Require astli.lock to be up to date; exit with an error instead of modifying the lockfile
    #[usage(long, global)]
    pub locked: bool,
}

/// Diagnostic output formats.
#[derive(ValueEnum, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagnosticFormat {
    /// Rich terminal output with source code snippets, underlines, and error context
    #[default]
    Pretty,
    /// Compact single-line messages (`file:line:col: severity[code]: message`) for editors and CI
    Short,
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
