//! The shape of the command line: the root, the list of stages, and the flag
//! groups more than one of them takes.
//!
//! What a single command takes is declared with that command, under `cmd`,
//! because a flag and the code reading it are one thought. What is here is
//! what no one command owns.
//!
//! # What is not here
//!
//! Flags that would not do anything yet. `--color` wants a diagnostic
//! renderer, `-o` wants more plumbing than a shell redirect, and a flag that
//! is accepted and ignored is worse than one that does not exist. The one
//! exception is `fmt`, which is declared in order to pin the shape of
//! `--check` and `--write` before there is a formatter behind them.

use std::path::PathBuf;

use usage::{Args, Cli, Subcommands};

use crate::cmd::completion::Completion;
use crate::cmd::fmt::Fmt;
use crate::cmd::lex::Lex;
use crate::cmd::parse::Parse;
use crate::cmd::preprocess::Preprocess;

/// SystemVerilog tooling. Each subcommand stops the pipeline one stage later
/// and prints what it has.
#[derive(Cli)]
// `unknown_flags` because the default is to hand an unrecognised word
// through as a positional, and every positional here is a file: a mistyped
// flag would be reported as a file that does not exist.
#[usage(bin = "svirig", long_version, completion, unknown_flags = "error")]
pub struct Svirig {
    #[usage(subcommand)]
    pub command: Commands,
    #[usage(flatten)]
    pub run: RunArgs,
}

/// Listed in the order the pipeline runs rather than alphabetically, which is
/// the one thing the list can say about how the stages relate.
// `run_with` so that the match from a parsed command to the code carrying
// it out is generated from this list rather than kept in step with it by
// hand. The context is the output handle every command but `fmt` writes to.
#[derive(Subcommands)]
#[usage(run_with)]
pub enum Commands {
    #[usage(display_order = 1)]
    Lex(Lex),
    #[usage(alias = "pp", display_order = 2)]
    Preprocess(Preprocess),
    #[usage(display_order = 3)]
    Parse(Parse),
    // No context: there is nothing to write until there is a formatter.
    #[usage(display_order = 4, no_ctx)]
    Fmt(Fmt),
    #[usage(display_order = 5)]
    Completion(Completion),
}

/// What to read.
///
/// Files named on the command line, files named by a filelist, or both. Two
/// flags for a filelist rather than one because a relative path in one has two
/// answers in circulation and the flag is the only place to say which:
/// `docs/limitations.md` has what this does not read.
///
/// The short forms are the ones a filelist itself uses to name a nested one,
/// which is also what generates these files. The long forms are here because
/// case alone is a poor thing to hang the difference on, and because a flag
/// with no long form is a flag `--help` cannot explain.
#[derive(Args, Default)]
pub struct Sources {
    /// The files to read
    #[usage(arg)]
    pub files: Vec<PathBuf>,
    /// A filelist, whose relative paths are relative to the working directory
    #[usage(short = 'f', long = "filelist")]
    pub filelist: Vec<PathBuf>,
    /// A filelist, whose relative paths are relative to the filelist itself
    #[usage(short = 'F', long = "filelist-relative", value_name = "FILELIST")]
    pub relative: Vec<PathBuf>,
}

/// How a run over several files goes: how much of it to print, and how many
/// files to read at once.
///
/// The summary is not a flag: every command that reads files ends with one,
/// because the figure worth having is over the run and not over a file. What
/// `--quiet` drops is the dump in front of it, which is what a timing run and
/// a "which of these 400 files is broken" run both want.
///
/// `--jobs` exists for the two runs that need the number fixed rather than
/// fast: `-j1` is what reproduces a figure, since threads share a memory bus
/// and a rate measured against a busy one is not the parser's.
///
/// On the root rather than on each command that reads files, and `global` so
/// that either side of the subcommand works: `svirig -q lex` and `svirig lex
/// -q` are the same run. The cost is `completion`, which reads no files and
/// advertises both anyway. One declaration is worth one command listing two
/// flags it ignores; four copies of it were not.
#[derive(Args, Default)]
pub struct RunArgs {
    /// Print only the summary, not the files themselves
    #[usage(short = 'q', long, global)]
    pub quiet: bool,
    /// How many files to read at once; 0 is one per core
    #[usage(short = 'j', long, default = "0", global)]
    pub jobs: usize,
}

/// How a build is asked for: where an `include` looks, and what is defined
/// before the first line.
///
/// Two spellings of each, because both are in circulation and a caller should
/// not have to translate. `-I` and `-D` are what a compiler takes; `+incdir+`
/// and `+define+` are what a simulator takes, and are the two a filelist may
/// carry, so a command line that was pasted out of one works. `plus` is where
/// the plus-separated form is unpacked, and [`sources::resolve`] is where all
/// four become the build itself.
///
/// [`sources::resolve`]: crate::sources::resolve
///
/// One struct rather than four flags in four places, because they arrive from
/// the same place -- a filelist, a manifest, a command line -- and none is a
/// property of the file. `docs/api.md` has the shape this grows into.
///
/// `preprocess` and `parse` take it as flags, for different reasons: expanded
/// mode uses a build to decide what the text *is*, and raw mode uses it only
/// to know what is a call and how wide. `lex` takes neither, having no use for
/// either -- what the bytes are is not a question a definition answers. A
/// filelist carries them to every command all the same, since a filelist is
/// not written per command; `lex` says so and reads on.
#[derive(Args, Default)]
pub struct BuildArgs {
    /// A directory to search for `include "..."`, repeatable
    #[usage(short = 'I', long = "incdir")]
    pub incdir: Vec<PathBuf>,
    /// A directory to search, plus-separated
    #[usage(arg, sigil = "+incdir+", value_name = "+incdir+DIR+...", hide)]
    pub incdir_plus: Vec<String>,
    /// A macro defined before the first line: NAME or NAME=VALUE, repeatable
    #[usage(short = 'D', long = "define")]
    pub define: Vec<String>,
    /// A macro defined before the first line, plus-separated
    #[usage(arg, sigil = "+define+", value_name = "+define+NAME[=VALUE]+...", hide)]
    pub define_plus: Vec<String>,
}
