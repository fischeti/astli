//! Command-line interface definitions and shared argument groups.
//!
//! Subcommand-specific flags are declared alongside their implementations in
//! [`crate::cmd`]. This module defines the top-level CLI (`astli`), subcommands,
//! and shared arguments reused across commands (input sources, execution/job flags,
//! and preprocessor options).

use std::path::PathBuf;

use astli_package::Simulator;
use astli_text::clean;
use usage_rs::{Args, Cli, Subcommands, ValueEnum};

use crate::cmd::add::Add;
use crate::cmd::bender::Bender;
use crate::cmd::build::Build;
use crate::cmd::cache::Cache;
use crate::cmd::check::Check;
use crate::cmd::clean::Clean;
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
use crate::cmd::remove::Remove;
use crate::cmd::run::Run;
use crate::cmd::snapshot::Snapshot;
use crate::cmd::test::Test;
use crate::cmd::tree::Tree;
use crate::cmd::update::Update;
use crate::error::{Error, Result};

/// Modern SystemVerilog toolchain: format, lint, check, elaborate, build, and manage dependencies.
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
    #[usage(help_heading = "Design", display_order = 7)]
    Build(Build),
    #[usage(help_heading = "Design", display_order = 8)]
    Run(Run),
    #[usage(help_heading = "Design", display_order = 9)]
    Test(Test),
    #[usage(help_heading = "Design", display_order = 10)]
    Clean(Clean),
    #[usage(help_heading = "Dependencies", display_order = 11)]
    Init(Init),
    #[usage(help_heading = "Dependencies", display_order = 12)]
    Add(Add),
    #[usage(help_heading = "Dependencies", display_order = 13)]
    Remove(Remove),
    #[usage(help_heading = "Dependencies", display_order = 14)]
    Update(Update),
    #[usage(alias = "checkout", help_heading = "Dependencies", display_order = 15)]
    Fetch(Fetch),
    #[usage(help_heading = "Dependencies", display_order = 16)]
    Tree(Tree),
    #[usage(help_heading = "Dependencies", display_order = 17)]
    Path(Path),
    #[usage(alias = "clone", help_heading = "Dependencies", display_order = 18)]
    Patch(Patch),
    #[usage(help_heading = "Dependencies", display_order = 19)]
    Snapshot(Snapshot),
    #[usage(help_heading = "Dependencies", display_order = 20)]
    Cache(Cache),
    #[usage(help_heading = "Dependencies", display_order = 21)]
    Bender(Bender),
    #[usage(display_order = 22)]
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

impl Sources {
    /// Whether any file is named, which a command reads in place of the
    /// package's.
    pub fn named(&self) -> bool {
        !(self.files.is_empty() && self.filelist.is_empty() && self.relative.is_empty())
    }
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

    /// The flags, as a command that repeats the run gives them.
    pub fn repeat_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        for feature in &self.features {
            args.extend(["--features".to_string(), feature.clone()]);
        }
        if !self.all_features {
            args.push("--no-features".to_string());
        }
        args
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
    /// When to colour the output
    #[usage(long, value_enum, value_name = "WHEN", default = "auto", global)]
    pub color: Color,
    /// Require astli.lock to be up to date; exit with an error instead of modifying the lockfile
    #[usage(long, global)]
    pub locked: bool,
    /// Never clone or fetch: resolve over the dependency cache as it is, and fail naming what it lacks
    #[usage(long, global, env = "ASTLI_OFFLINE")]
    pub offline: bool,
}

/// Diagnostic output formats.
#[derive(ValueEnum, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagnosticFormat {
    /// Rich terminal output with source code snippets, underlines, and error context
    #[default]
    Pretty,
    /// Compact single-line messages (`file:line:col: severity[code]: message`) for editors and CI
    Short,
    /// The compact lines as GitHub Actions workflow commands, which annotate the pull request
    Github,
    /// Machine-readable JSON Lines format with source ranges, macro expansions, and include traces
    Json,
    /// GitLab Code Quality report formatted for merge request diff annotations
    Gitlab,
}

impl DiagnosticFormat {
    /// Whether stderr holds the diagnostics alone, as one document a program
    /// reads whole: no status, no progress, no colour.
    pub fn owns_stderr(self) -> bool {
        matches!(self, DiagnosticFormat::Json | DiagnosticFormat::Gitlab)
    }
}

/// When output is coloured.
#[derive(ValueEnum, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    /// Where the stream is a terminal, unless `NO_COLOR` is set or `CLICOLOR_FORCE` forces it
    #[default]
    Auto,
    /// Always, piped or not
    Always,
    /// Never
    Never,
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

impl BuildArgs {
    /// The include directories the flags give, searched in this order:
    /// `+incdir+`s first, since the argument parser does not say where on
    /// the command line each flag was.
    pub fn incdirs(&self) -> impl Iterator<Item = PathBuf> + '_ {
        (self.incdir_plus.iter())
            .flat_map(|arg| crate::filelist::plus(arg))
            .map(PathBuf::from)
            .chain(self.incdir.iter().cloned())
    }

    /// The definitions the flags give, as written, `+define+`s first, so
    /// that a `-D` of the same name wins.
    pub fn defines(&self) -> impl Iterator<Item = String> + '_ {
        (self.define_plus.iter())
            .flat_map(|arg| crate::filelist::plus(arg))
            .map(String::from)
            .chain(self.define.iter().cloned())
    }
}

/// How `build`, `run` and `test` build a top: the profile, the simulator,
/// and the include directories and defines over the package's.
#[derive(Args, Default, Clone)]
pub struct SimArgs {
    /// Build with the release profile, which compiles longer and simulates faster
    #[usage(long)]
    pub release: bool,
    /// Build so that the simulation can write waveforms, whatever the profile sets
    #[usage(long)]
    pub trace: bool,
    /// Simulator to build with, in place of the manifest's [build] simulator
    #[usage(long, value_enum)]
    pub sim: Option<Simulator>,
    // Without `+incdir+` and `+define+`, a filelist's spelling, which would
    // take a simulation's plusarg after `astli run --` for its own.
    /// Add directory to the `include` search path (can be repeated)
    #[usage(short = 'I', long = "incdir")]
    pub incdir: Vec<PathBuf>,
    /// Define preprocessor macro as NAME or NAME=VALUE (can be repeated)
    #[usage(short = 'D', long = "define")]
    pub define: Vec<String>,
}

impl SimArgs {
    /// The include directories and defines, as the preprocessor takes them,
    /// each directory absolute: a relative one is relative to where astli
    /// was run, and the simulator runs at the root.
    pub fn build_args(&self) -> Result<BuildArgs> {
        let incdir = (self.incdir.iter())
            .map(|dir| std::path::absolute(dir).map(|it| clean(&it)))
            .map(|dir| dir.map_err(|err| Error::io(".", err)))
            .collect::<Result<_>>()?;
        Ok(BuildArgs {
            incdir,
            define: self.define.clone(),
            ..BuildArgs::default()
        })
    }

    /// The flags, as a command that repeats the run gives them, in the
    /// order `--help` lists them.
    pub fn repeat_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if self.release {
            args.push("--release".to_string());
        }
        if self.trace {
            args.push("--trace".to_string());
        }
        if let Some(sim) = self.sim {
            args.extend(["--sim".to_string(), sim.name().to_string()]);
        }
        for dir in &self.incdir {
            args.extend(["-I".to_string(), dir.to_string_lossy().into_owned()]);
        }
        for define in &self.define {
            args.extend(["-D".to_string(), define.clone()]);
        }
        args
    }
}

/// Which package of a workspace a command acts on, for a command that
/// describes one design.
#[derive(Args, Default)]
pub struct PackageArg {
    /// Act on the workspace package of this name, as if run inside it
    #[usage(short = 'p', long = "package", value_name = "NAME")]
    pub package: Option<String>,
}

/// Which packages of a workspace a command acts on, for a command that can
/// act on each in turn.
#[derive(Args, Default)]
pub struct Members {
    /// Act on the workspace package of this name, as if run inside it
    #[usage(short = 'p', long = "package", value_name = "NAME")]
    pub package: Option<String>,
    /// Act on every package of the workspace in turn, the root included, or
    /// outside one on the package alone
    #[usage(long, conflicts("--package"))]
    pub workspace: bool,
}
