//! `astli files` subcommand.
//!
//! Reads a filelist and writes one back: trimmed to what the tops need,
//! ordered so each file follows what it depends on, or both. Each file is
//! expanded as it would be compiled, so what its macros write and its
//! headers declare count.
//!
//! With no file named, the files are the package the current directory is
//! in and its dependencies, always ordered, and the include directories
//! written are those a tool needs to find every header without astli's
//! search by suffix.

use std::io::Write;
use std::path::{Path, PathBuf};

use astli_index::{Index, Summary};
use astli_text::clean;
use usage_rs::{Args, RunWith, ValueEnum};

use astli_package::mark;

use crate::cli::{BuildArgs, FeatureArgs, PackageArg, Sources};
use crate::cmd::{self, Ctx};
use crate::discovery::{Plan, kept_only};
use crate::error::{Error, Result};
use crate::filelist::quoted;
use crate::listing;
use crate::lockfile;
use crate::render;
use crate::term;
use crate::tops::Chosen;

/// Prune and topologically sort filelists based on symbol dependencies
#[derive(Args)]
pub struct Files {
    #[usage(flatten)]
    pub sources: Sources,
    #[usage(flatten)]
    pub package: PackageArg,
    #[usage(flatten)]
    pub features: FeatureArgs,
    /// Retain only files required to build the specified top module or [top.NAME] table (can be repeated)
    #[usage(long)]
    pub top: Vec<String>,
    /// Topologically sort files so dependencies precede their dependents (always, for a package)
    #[usage(long)]
    pub order: bool,
    /// Format of output to emit
    #[usage(long, value_enum, default = "filelist")]
    pub emit: Emit,
    /// Trace dependency path showing why the top module requires this specific file
    #[usage(long, value_name = "FILE")]
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
        let target = cmd::choose(self.package.package.as_deref(), self.sources.named())?;
        let mut plan = Plan::new(&target, &self.sources, &self.features, &self.build, run)?;
        // Listing the candidates is not a run from any of them.
        let chosen = match self.emit {
            Emit::Tops => Chosen::default(),
            _ => Chosen::of(&plan, &self.top)?,
        };
        plan.define(&chosen);
        let tops: Vec<String> = (chosen.tops.iter()).map(|top| top.module.clone()).collect();
        lockfile::described(run, &plan.described, plan.in_workspace());

        // Nothing is printed per file: the output is about all of them. With
        // a top, what a root file says waits until it is known to be kept.
        let outcome = cmd::each(out, &plan.units, run, None, |sink, file| {
            let unit = plan.unit(file);
            let of = plan.dependency_of(unit);
            let told = |all| match unit.reported {
                true => all,
                false => listing::short_of_a_header(all, of),
            };
            let (summary, said, errors) = listing::summarized(file, plan.build(unit), told)?;
            if unit.reported && tops.is_empty() {
                sink.diagnostics.write_all(&said)?;
                sink.errors += errors;
                return Ok((summary, Vec::new(), 0));
            }
            Ok((summary, said, errors))
        })?;
        // A region that did not parse may hold an instance, so the list could
        // be short: a file with errors fails the run as it does `check`. With
        // a top, that is a file the list holds, as `check` reports those.
        if outcome.failed > 0 || outcome.wrong > 0 {
            return outcome.finish();
        }
        let (summaries, short): (Vec<Summary>, Vec<(Vec<u8>, usize)>) = (outcome.values)
            .into_iter()
            .map(|(summary, said, errors)| (summary, (said, errors)))
            .unzip();
        let unreplaced = plan.replace(&summaries)?;
        let short = kept_only(short, &unreplaced);
        let index = Index::new(kept_only(summaries, &unreplaced));

        if self.emit == Emit::Tops {
            for (_, declaration) in index.tops() {
                writeln!(out, "{}", declaration.name)?;
            }
            return Ok(());
        }

        chosen.unambiguous(|name| index.declaration(name).is_some())?;
        let reported: Vec<usize> = (plan.units.iter().enumerate())
            .filter(|(_, unit)| unit.reported)
            .map(|(at, _)| at)
            .collect();
        let mut kept = listing::kept(run, &index, &tops, &reported)?;
        if !tops.is_empty() {
            listing::foreign_binds(run, &plan, &index, &kept);
        }
        // With a top, so does a dependency's file the list holds that is
        // short of a header, which a tool given the list cannot find either.
        let wrong: Vec<&(Vec<u8>, usize)> = (kept.iter().map(|&file| &short[file]))
            .filter(|_| !tops.is_empty())
            .filter(|(_, errors)| *errors > 0)
            .collect();
        for (said, _) in &wrong {
            term::write(said).map_err(Error::Output)?;
        }
        if !wrong.is_empty() {
            return Err(Error::tally(format!(
                "{} of {} {} errors",
                wrong.len(),
                render::plural(kept.len(), "file"),
                render::have(wrong.len()),
            )));
        }
        // A file the tops do not reach cannot shorten the list by what it
        // instantiates, but may by what it declares: a region of it that did
        // not parse may hold a name they use that no file declares.
        if !tops.is_empty() && !index.undeclared(&kept).is_empty() {
            let unreached: Vec<&(Vec<u8>, usize)> = (short.iter().enumerate())
                .filter(|(file, (_, errors))| *errors > 0 && !kept.contains(file))
                .map(|(_, short)| short)
                .collect();
            for (said, _) in &unreached {
                term::write(said).map_err(Error::Output)?;
            }
            if !unreached.is_empty() {
                term::note("a file with errors may declare what the tops use and no file declares")
                    .report(run);
                return Err(Error::tally(format!(
                    "{} of {} {} errors",
                    unreached.len(),
                    render::plural(short.len(), "file"),
                    render::have(unreached.len()),
                )));
            }
        }

        if let Some(file) = &self.why {
            if tops.is_empty() {
                return Err(Error::failed(format!(
                    "{} asks why a top needs a file: name one with {}",
                    mark::code("--why"),
                    mark::code("--top")
                )));
            }
            return why(out, &index, &tops, file);
        }

        kept = listing::without_stubs(run, &index, kept, true);

        // A filelist's order can carry what no file says, such as a defines
        // file that must come first, so it is kept unless asked otherwise.
        if self.order || plan.is_package() {
            // Sorted by package first, so that each package's files stay
            // together and the list reads as the packages depend on each
            // other: a file still follows what it uses wherever that is.
            kept.sort_by_key(|&file| plan.group(file));
            kept = index.ordered(&kept);
        }
        let summaries = kept.iter().map(|&file| &index.summaries()[file]);
        let incdirs = match tops.is_empty() {
            true => plan.incdirs.clone(),
            false => listing::used(&plan.incdirs, summaries.clone()),
        };

        if plan.is_package() {
            listing::misfound(run, &incdirs, summaries.clone());
        }
        if matches!(self.emit, Emit::Filelist | Emit::Incdirs) {
            for dir in &incdirs {
                writeln!(out, "{}", quoted(&format!("+incdir+{}", dir.display())))?;
            }
        }
        if self.emit == Emit::Filelist {
            for define in &plan.defines {
                writeln!(out, "{}", quoted(&format!("+define+{define}")))?;
            }
        }
        if matches!(self.emit, Emit::Filelist | Emit::Files) {
            for summary in summaries {
                writeln!(out, "{}", quoted(&summary.path.display().to_string()))?;
            }
        }
        Ok(())
    }
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
