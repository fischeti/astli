//! `astli files` subcommand.
//!
//! Reads a filelist and writes one back: trimmed to what the tops need,
//! ordered so each file follows what it depends on, or both. Each file is
//! expanded as it would be compiled, so what its macros write and its
//! headers declare count.

use std::io::Write;
use std::path::{Path, PathBuf};

use astli_index::{Index, Summary, Uses, summarize};
use astli_text::clean;
use usage::{Args, RunWith, ValueEnum};

use crate::cli::{BuildArgs, RunArgs, Sources};
use crate::cmd::{self, Ctx};
use crate::error::{Error, Result};
use crate::render;
use crate::session;
use crate::sources;

/// Trim and order a filelist by the modules, packages and classes its files declare and use
#[derive(Args)]
pub struct Files {
    #[usage(flatten)]
    pub sources: Sources,
    /// Keep only the files this top needs (can be repeated)
    #[usage(long)]
    pub top: Vec<String>,
    /// Put each file after the files it depends on
    #[usage(long)]
    pub order: bool,
    /// Output to emit
    #[usage(long, value_enum, default = "filelist")]
    pub emit: Emit,
    /// Print why the tops need this file, as the chain of references from one
    #[usage(long, value_name = "FILE", requires("--top"))]
    pub why: Option<PathBuf>,
    #[usage(flatten)]
    pub build: BuildArgs,
}

/// What `files` writes.
#[derive(ValueEnum, Clone, Copy, PartialEq, Eq)]
pub enum Emit {
    /// Include directories, definitions and files, as a filelist
    Filelist,
    /// The files alone
    Files,
    /// The include directories alone
    Incdirs,
    /// The modules and programs no file references, which could be tops
    Tops,
}

impl RunWith<Ctx<'_>> for Files {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        let resolved = sources::resolve(&self.sources, &self.build)?;

        // Nothing is printed per file: the output is about all of them.
        let silent = RunArgs {
            quiet: true,
            jobs: run.jobs,
        };
        let outcome = cmd::each(out, &resolved.files, &silent, "", |sink, file| {
            one(sink, file, &resolved.build)
        })?;
        if outcome.failed > 0 {
            return outcome.finish();
        }
        let index = Index::new(outcome.values);

        if self.emit == Emit::Tops {
            for (_, declaration) in index.tops() {
                writeln!(out, "{}", declaration.name)?;
            }
            return Ok(());
        }

        let mut kept: Vec<usize> = match self.top.is_empty() {
            true => (0..index.summaries().len()).collect(),
            false => index
                .reachable(&self.top)
                .map_err(|err| Error::failed(err.to_string()))?,
        };
        warn(&index, &kept);

        if let Some(file) = &self.why {
            return why(out, &index, &self.top, file);
        }

        if self.order {
            kept = index.ordered(&kept);
        }
        let summaries = kept.iter().map(|&file| &index.summaries()[file]);
        let incdirs = match self.top.is_empty() {
            true => resolved.incdir.clone(),
            false => used(&resolved.incdir, summaries.clone()),
        };

        if matches!(self.emit, Emit::Filelist | Emit::Incdirs) {
            for dir in &incdirs {
                writeln!(out, "+incdir+{}", dir.display())?;
            }
        }
        if self.emit == Emit::Filelist {
            for define in &resolved.define {
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
/// parser reported. Those are context for the answer, not a failure of it.
fn one(sink: &mut cmd::Sink, path: &Path, build: &astli_preproc::Build) -> Result<Summary> {
    let mut opened = session::open(path, build)?;
    let (summary, diagnostics) = summarize(&mut opened.session, opened.file);
    render::diagnostics(sink.diagnostics, opened.session.origins(), &diagnostics)?;
    Ok(summary)
}

/// Warns of each name the kept files need that no file declares, and each
/// name declared twice.
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
    for (earlier, later) in index.redeclared() {
        eprintln!(
            "astli: {}: `{}` is declared again, and this declaration is used; {} declared it first",
            later.location, later.name, earlier.location
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
            Some(reference) => writeln!(
                out,
                "{path}: declares `{}`, used at {}",
                reference.name, reference.location
            )?,
        }
    }
    Ok(())
}
