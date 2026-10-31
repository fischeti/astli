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

use astli_index::{Declaration, Declares, Index, Redeclared, Summary, Uses, summarize};
use astli_text::clean;
use usage_rs::{Args, RunWith, ValueEnum};

use astli_package::mark;

use crate::cli::{BuildArgs, FeatureArgs, RunArgs, Sources};
use crate::cmd::{self, Ctx};
use crate::discovery::{Plan, kept_only};
use crate::error::{Error, Result};
use crate::lockfile;
use crate::render;
use crate::session;
use crate::term::{self, Place};

/// Prune and topologically sort filelists based on symbol dependencies
#[derive(Args)]
pub struct Files {
    #[usage(flatten)]
    pub sources: Sources,
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
        // Listing the candidates is not a run from any of them.
        let asked = (self.emit != Emit::Tops).then_some(self.top.as_slice());
        let mut plan = Plan::new(&self.sources, &self.features, &self.build, asked, run)?;
        let tops = std::mem::take(&mut plan.tops.modules);
        lockfile::described(&plan.described);

        // Nothing is printed per file: the output is about all of them.
        let silent = RunArgs {
            quiet: true,
            jobs: run.jobs,
            diagnostics: run.diagnostics,
            color: run.color,
            locked: run.locked,
        };
        let outcome = cmd::each(out, &plan.units, &silent, "", |sink, file| {
            let unit = plan.unit(file);
            one(sink, file, plan.build(unit), unit.reported)
        })?;
        // A region that did not parse may hold an instance, so the list could
        // be short: a file with errors fails the run as it does `check`.
        if outcome.failed > 0 || outcome.wrong > 0 {
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

        plan.tops
            .unambiguous(|name| index.declaration(name).is_some())?;
        let reported: Vec<usize> = (plan.units.iter().enumerate())
            .filter(|(_, unit)| unit.reported)
            .map(|(at, _)| at)
            .collect();
        let mut kept = kept(&index, &tops, &reported)?;

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

        // A filelist's order can carry what no file says, such as a defines
        // file that must come first, so it is kept unless asked otherwise.
        if self.order || plan.is_package() {
            kept = index.ordered(&kept);
        }
        let summaries = kept.iter().map(|&file| &index.summaries()[file]);
        let incdirs = match tops.is_empty() {
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
/// parser reported if it is `reported`, counting its errors.
pub(crate) fn one(
    sink: &mut cmd::Sink,
    path: &Path,
    build: &astli_preproc::Build,
    reported: bool,
) -> Result<Summary> {
    let mut opened = session::open(path, build)?;
    let (summary, diagnostics) = summarize(&mut opened.session, opened.file);
    if reported {
        sink.errors +=
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
            term::note(format!(
                "{}: encrypted, so it is kept whatever the tops need",
                summary.path.display()
            ))
            .report();
        }
    }
    warn(index, &told);
    redeclared(index, &told, tops)?;
    Ok(kept)
}

/// Reports each name declared more than once that `kept` declares or uses:
/// an error if it is used or a top, and a warning otherwise, or if only
/// classes declare it.
pub(crate) fn redeclared(index: &Index, kept: &[usize], tops: &[String]) -> Result {
    let mut refused = 0;
    for redeclared in index.redeclared(kept) {
        let declarations = &redeclared.declarations;
        if (declarations.iter()).all(|(_, declaration)| declaration.declares == Declares::Class) {
            classes(index, &redeclared);
            continue;
        }
        let used = redeclared.referenced || tops.iter().any(|top| top == redeclared.name);
        refused += usize::from(used);
        let how = match used {
            true => "used, so which one it means is unclear",
            false => "used nowhere",
        };
        let message = format!(
            "{} is declared {} times and {how}",
            mark::name(redeclared.name),
            redeclared.declarations.len()
        );
        let mut problem = match used {
            true => term::error(message),
            false => term::warning(message),
        };
        let [(_, first), rest @ ..] = redeclared.declarations.as_slice() else {
            continue;
        };
        problem = (problem.at("declared-twice", &first.location.path, named(first)))
            .label("declared here");
        for (_, declaration) in rest {
            problem = problem.note(format!("also declared at {}", declaration.location));
        }
        problem.report();
    }
    match refused {
        0 => Ok(()),
        1 => Err(Error::failed("1 name declared more than once is used")),
        n => Err(Error::failed(format!(
            "{n} names declared more than once are used"
        ))),
    }
}

/// Warns of a class declared at the top level of more than one file. Each
/// file is its own compilation unit, which holds a class of its own, so no
/// file's use of the name is unclear; compiled as one unit, though, the
/// later declarations declare it again.
fn classes(index: &Index, redeclared: &Redeclared) {
    let summaries = index.summaries();
    // The file that includes a declaration's header, if it is not its own.
    let includer = |&(file, declaration): &(usize, &Declaration)| {
        let path = &summaries[file].path;
        (clean(path) != clean(&declaration.location.path)).then_some(path)
    };
    let declarations = redeclared.declarations.as_slice();
    let [first, rest @ ..] = declarations else {
        return;
    };
    let message = format!(
        "the class {} is declared in {} files, each its own compilation unit",
        mark::name(redeclared.name),
        declarations.len()
    );
    let mut problem = (term::warning(message))
        .at("declared-twice", &first.1.location.path, named(first.1))
        .label(match includer(first) {
            Some(path) => format!(
                "declared here, as {} includes it",
                mark::path(path.display())
            ),
            None => "declared here".to_string(),
        });
    for declaration in rest {
        let at = &declaration.1.location;
        problem = problem.note(match includer(declaration) {
            Some(path) => format!(
                "also declared at {at}, as {} includes it",
                mark::path(path.display())
            ),
            None => format!("also declared at {at}"),
        });
    }
    problem = problem.note("in one compilation unit over all files, it is declared again");
    // A header compiled on its own as well as through an `include`.
    let header = (declarations.iter())
        .filter(|declaration| includer(declaration).is_none())
        .map(|(_, declaration)| &declaration.location.path)
        .find(|header| {
            (declarations.iter()).any(|other| {
                includer(other).is_some() && clean(&other.1.location.path) == clean(header)
            })
        });
    if let Some(header) = header {
        problem = problem.help(format!(
            "{} is included, so leave it out of the sources",
            mark::path(header.display())
        ));
    }
    problem.report();
}

fn warn(index: &Index, kept: &[usize]) {
    for (_, reference) in index.undeclared(kept) {
        let (how, code) = match reference.uses {
            Uses::Import => ("imported", "unknown-package"),
            _ => ("instantiated", "unknown-definition"),
        };
        let place = Place::Name(reference.location.at, reference.name.chars().count());
        term::warning(format!(
            "{} is {how} and declared in no file",
            mark::name(&reference.name)
        ))
        .at(code, &reference.location.path, place)
        .label(format!("{how} here"))
        .report();
    }
}

/// Where `declaration` names what it declares.
fn named(declaration: &Declaration) -> Place {
    Place::Name(declaration.location.at, declaration.name.chars().count())
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
