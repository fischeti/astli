//! `astli files` subcommand.
//!
//! Reads a filelist and writes one back: trimmed to what the tops need,
//! ordered so each file follows what it depends on, or both. Each file is
//! expanded as it would be compiled, so what its macros write and its
//! headers declare count.
//!
//! With no file named, the files are the package the current directory is
//! in and its dependencies, and the include directories written are those
//! a tool needs to find every header without astli's search by suffix.

use std::io::Write;
use std::path::{Path, PathBuf};

use astli_index::{Index, Summary, Uses, summarize};
use astli_text::clean;
use usage::{Args, RunWith, ValueEnum};

use crate::cli::{BuildArgs, FeatureArgs, RunArgs, Sources};
use crate::cmd::{self, Ctx};
use crate::discovery::{Plan, kept_only};
use crate::error::{Error, Result};
use crate::render;
use crate::session;

/// Prune and topologically sort filelists based on symbol dependencies
#[derive(Args)]
pub struct Files {
    #[usage(flatten)]
    pub sources: Sources,
    #[usage(flatten)]
    pub features: FeatureArgs,
    /// Retain only the files required to build the specified top-level module (can be repeated)
    #[usage(long)]
    pub top: Vec<String>,
    /// Topologically sort files so dependencies precede their dependents
    #[usage(long)]
    pub order: bool,
    /// Format of output to emit
    #[usage(long, value_enum, default = "filelist")]
    pub emit: Emit,
    /// Trace dependency path showing why the top module requires this specific file
    #[usage(long, value_name = "FILE", requires("--top"))]
    pub why: Option<PathBuf>,
    #[usage(flatten)]
    pub build: BuildArgs,
}

/// Filelist output emission formats.
#[derive(ValueEnum, Clone, Copy, PartialEq, Eq)]
pub enum Emit {
    /// Complete EDA filelist including +incdir+, +define+, and source file paths
    Filelist,
    /// Bare source file paths only
    Files,
    /// Include search directories (+incdir+) only
    Incdirs,
    /// Candidate top-level modules and programs not referenced by any other file
    Tops,
}

impl RunWith<Ctx<'_>> for Files {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        let mut plan = Plan::new(&self.sources, &self.features, &self.build, run)?;

        // Nothing is printed per file: the output is about all of them.
        let silent = RunArgs {
            quiet: true,
            jobs: run.jobs,
            diagnostics: run.diagnostics,
            locked: run.locked,
        };
        let outcome = cmd::each(out, &plan.units, &silent, "", |sink, file| {
            let unit = plan.unit(file);
            one(sink, file, plan.build(unit), unit.reported)
        })?;
        if outcome.failed > 0 {
            return outcome.finish();
        }
        let unreplaced = plan.replace(&outcome.values)?;
        let index = Index::new(kept_only(outcome.values, &unreplaced));

        if self.emit == Emit::Tops {
            for (_, declaration) in index.tops() {
                writeln!(out, "{}", declaration.name)?;
            }
            return Ok(());
        }

        let reported: Vec<usize> = (plan.units.iter().enumerate())
            .filter(|(_, unit)| unit.reported)
            .map(|(at, _)| at)
            .collect();
        let mut kept = kept(&index, &self.top, &reported)?;

        if let Some(file) = &self.why {
            return why(out, &index, &self.top, file);
        }

        if self.order {
            kept = index.ordered(&kept);
        }
        let summaries = kept.iter().map(|&file| &index.summaries()[file]);
        let incdirs = match self.top.is_empty() {
            true => plan.incdirs.clone(),
            false => used(&plan.incdirs, summaries.clone()),
        };

        if matches!(self.emit, Emit::Filelist | Emit::Incdirs) {
            for dir in &incdirs {
                writeln!(out, "+incdir+{}", dir.display())?;
            }
        }
        if self.emit == Emit::Filelist {
            for define in &plan.defines {
                writeln!(out, "+define+{define}")?;
            }
        }
        if matches!(self.emit, Emit::Filelist | Emit::Files) {
            for summary in summaries {
                writeln!(out, "{}", summary.path.display())?;
            }
        }
        Ok(())
    }
}

/// Expands and summarizes one file, printing what the expansion and the
/// parser reported if it is `reported`. Those are context for the answer,
/// not a failure of it.
fn one(
    sink: &mut cmd::Sink,
    path: &Path,
    build: &astli_preproc::Build,
    reported: bool,
) -> Result<Summary> {
    let mut opened = session::open(path, build)?;
    let (summary, diagnostics) = summarize(&mut opened.session, opened.file);
    if reported {
        render::diagnostics(sink.diagnostics, opened.session.origins(), &diagnostics)?;
    }
    Ok(summary)
}

/// The files the tops need, or all of them without a top, warning of each
/// name those files need that no file declares and of each declared more
/// than once. A name declared more than once that one of them uses, or that
/// is a top, is refused, since which declaration it means is a guess.
///
/// Without a top, only `reported`'s problems are told: a dependency's are its
/// authors' to fix. With one, those of every file kept are, since the tool
/// given the list reads them all.
pub(crate) fn kept(index: &Index, tops: &[String], reported: &[usize]) -> Result<Vec<usize>> {
    let (kept, told): (Vec<usize>, Vec<usize>) = match tops.is_empty() {
        true => ((0..index.summaries().len()).collect(), reported.to_vec()),
        false => {
            let kept = (index.reachable(tops)).map_err(|err| Error::failed(err.to_string()))?;
            (kept.clone(), kept)
        }
    };
    if !tops.is_empty() {
        for summary in index.summaries().iter().filter(|summary| summary.encrypted) {
            eprintln!(
                "astli: {}: encrypted, so it is kept whatever the tops need",
                summary.path.display()
            );
        }
    }
    warn(index, &told);
    redeclared(index, &told, tops)?;
    Ok(kept)
}

/// Reports each name declared more than once that `kept` declares or uses:
/// an error if it is used or a top, and a warning otherwise.
pub(crate) fn redeclared(index: &Index, kept: &[usize], tops: &[String]) -> Result {
    let mut refused = 0;
    for redeclared in index.redeclared(kept) {
        let used = redeclared.referenced || tops.iter().any(|top| top == redeclared.name);
        refused += usize::from(used);
        let how = match used {
            true => "used, so which one it means is unclear",
            false => "used nowhere",
        };
        let at: Vec<String> = (redeclared.declarations.iter())
            .map(|declaration| declaration.location.to_string())
            .collect();
        eprintln!(
            "astli: `{}` is declared {} times and {how}: {}",
            redeclared.name,
            at.len(),
            at.join(", ")
        );
    }
    match refused {
        0 => Ok(()),
        n => Err(Error::failed(format!(
            "{n} name(s) declared more than once are used"
        ))),
    }
}

fn warn(index: &Index, kept: &[usize]) {
    for (_, reference) in index.undeclared(kept) {
        let how = match reference.uses {
            Uses::Import => "imported",
            _ => "instantiated",
        };
        eprintln!(
            "astli: {}: `{}` is {how} here and declared in no file",
            reference.location, reference.name
        );
    }
}

/// The include directories that hold a header one of `summaries` read.
fn used<'a>(incdirs: &[PathBuf], summaries: impl Iterator<Item = &'a Summary>) -> Vec<PathBuf> {
    let headers: Vec<PathBuf> = summaries
        .flat_map(|summary| &summary.includes)
        .map(|header| clean(header))
        .collect();
    incdirs
        .iter()
        .filter(|dir| {
            let dir = clean(dir);
            headers.iter().any(|header| header.starts_with(&dir))
        })
        .cloned()
        .collect()
}

/// Prints the chain of references from a top to `file`.
fn why(out: &mut dyn Write, index: &Index, tops: &[String], file: &Path) -> Result {
    let wanted = clean(file);
    let Some(at) = index
        .summaries()
        .iter()
        .position(|summary| clean(&summary.path) == wanted)
    else {
        return Err(Error::failed(format!(
            "{}: not in the filelist",
            file.display()
        )));
    };

    let chain = index
        .why(tops, at)
        .map_err(|err| Error::failed(err.to_string()))?;
    let Some(chain) = chain else {
        return Err(Error::failed(format!(
            "{}: the tops do not need it",
            file.display()
        )));
    };
    for step in chain {
        let path = index.summaries()[step.file].path.display();
        match step.via {
            None => writeln!(out, "{path}")?,
            Some(reference) if step.binds => writeln!(
                out,
                "{path}: binds into `{}` at {}",
                reference.name, reference.location
            )?,
            Some(reference) => writeln!(
                out,
                "{path}: declares `{}`, used at {}",
                reference.name, reference.location
            )?,
        }
    }
    Ok(())
}
