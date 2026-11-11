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

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use astli_index::{Declaration, Include, Index, Summary, Uses, summarize};
use astli_text::{Diagnostic, clean};
use rayon::prelude::*;
use usage_rs::{Args, RunWith, ValueEnum};

use astli_package::{Dependency, mark};

use crate::cli::{BuildArgs, FeatureArgs, PackageArg, Sources};
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
        cmd::choose(self.package.package.as_deref(), self.sources.named())?;
        // Listing the candidates is not a run from any of them.
        let asked = (self.emit != Emit::Tops).then_some(self.top.as_slice());
        let mut plan = Plan::new(&self.sources, &self.features, &self.build, asked, run)?;
        let tops = std::mem::take(&mut plan.tops.modules);
        lockfile::described(&plan.described);

        // Nothing is printed per file: the output is about all of them.
        let outcome = cmd::each(out, &plan.units, run.jobs, None, |sink, file| {
            let unit = plan.unit(file);
            match unit.reported {
                true => Ok((one(sink, file, plan.build(unit), true)?, Vec::new(), 0)),
                false => dependency(file, plan.build(unit), plan.dependency_of(unit)),
            }
        })?;
        // A region that did not parse may hold an instance, so the list could
        // be short: a file with errors fails the run as it does `check`.
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

        plan.tops
            .unambiguous(|name| index.declaration(name).is_some())?;
        let reported: Vec<usize> = (plan.units.iter().enumerate())
            .filter(|(_, unit)| unit.reported)
            .map(|(at, _)| at)
            .collect();
        let mut kept = kept(&index, &tops, &reported)?;
        if !tops.is_empty() {
            foreign_binds(&plan, &index, &kept);
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
            // Sorted by package first, so that each package's files stay
            // together and the list reads as the packages depend on each
            // other: a file still follows what it uses wherever that is.
            kept.sort_by_key(|&file| plan.group(file));
            kept = index.ordered(&kept);
        }
        let summaries = kept.iter().map(|&file| &index.summaries()[file]);
        let incdirs = match tops.is_empty() {
            true => plan.incdirs.clone(),
            false => used(&plan.incdirs, summaries.clone()),
        };

        if plan.is_package() {
            misfound(&incdirs, summaries.clone());
        }
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

/// A dependency's file summarized, with what it says of an `` `include ``
/// that found no one header, rendered, for the run to tell if it lists the
/// file, and how many errors that is.
fn dependency(
    path: &Path,
    build: &astli_preproc::Build,
    of: Option<&Dependency>,
) -> Result<(Summary, Vec<u8>, usize)> {
    let mut opened = session::open(path, build)?;
    let (summary, diagnostics) = summarize(&mut opened.session, opened.file);
    let diagnostics: Vec<Diagnostic> = (diagnostics.into_iter())
        .filter(unresolved_include)
        .map(|diagnostic| in_dependency(diagnostic, of))
        .collect();
    let mut said = Vec::new();
    let errors = render::diagnostics(&mut said, opened.session.origins(), &diagnostics)?;
    Ok((summary, said, errors))
}

/// Whether `diagnostic` says an `` `include `` found no one header. It is
/// told in a dependency's files too, whose other findings are its authors'
/// to fix: the list a tool is given then lacks what the file needs, and
/// the include directories astli chose for a dependency without a
/// manifest are the root's to correct.
pub(crate) fn unresolved_include(diagnostic: &Diagnostic) -> bool {
    matches!(
        diagnostic.code.as_str(),
        "include-not-found" | "include-ambiguous"
    )
}

/// `diagnostic`, about an include in a file of `dependency`, with where its
/// include directories are named, which is what fixes it.
pub(crate) fn in_dependency(diagnostic: Diagnostic, dependency: Option<&Dependency>) -> Diagnostic {
    let Some(dependency) = dependency else {
        return diagnostic;
    };
    let name = &dependency.name;
    match dependency.package.manifest {
        Some(_) => diagnostic.note(format!(
            "the file is `{name}`'s, whose astli.toml names its include directories"
        )),
        None => diagnostic.help(format!(
            "`{name}` has no astli.toml: name its include directories with \
             `[described.{name}] include-dirs` in the root's"
        )),
    }
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
        let message = format!(
            "{} is declared {} times and {how}",
            mark::name(redeclared.name),
            redeclared.declarations.len()
        );
        let mut problem = match used {
            true => term::error(message),
            false => term::warning(message),
        };
        let [first, rest @ ..] = redeclared.declarations.as_slice() else {
            continue;
        };
        problem = (problem.at("declared-twice", &first.location.path, named(first)))
            .label("declared here");
        for declaration in rest {
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
pub(crate) fn used<'a>(
    incdirs: &[PathBuf],
    summaries: impl Iterator<Item = &'a Summary>,
) -> Vec<PathBuf> {
    let headers: Vec<PathBuf> = summaries
        .flat_map(|summary| &summary.includes)
        .map(|header| clean(&header.path))
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

/// Notes each of `kept` that binds into the design from a dependency with
/// neither an astli.toml nor a description. Every file of such a dependency
/// is read, and a `bind` applies wherever its file is compiled, so a formal
/// property file its own build leaves out joins the list unasked.
pub(crate) fn foreign_binds(plan: &Plan, index: &Index, kept: &[usize]) {
    for &file in kept {
        let Some(dependency) = plan.dependency_of(&plan.units[file]) else {
            continue;
        };
        let name = &dependency.name;
        if dependency.package.manifest.is_some() || plan.described.contains(name) {
            continue;
        }
        let summary = &index.summaries()[file];
        let unheld = Uses::Bind { held: false };
        let Some(bind) = (summary.references.iter()).find(|it| it.uses == unheld) else {
            continue;
        };
        term::note(format!(
            "{} binds into {}, and `{name}` has no astli.toml, so every file of it is read",
            mark::path(summary.path.display()),
            mark::name(&bind.name)
        ))
        .help(format!(
            "if its own build leaves the file out, so can this one, with `[described.{name}] exclude`"
        ))
        .report();
    }
}

/// Warns of each header of `summaries` that a tool given `incdirs`, which
/// searches them alone and in their order, would find otherwise than the
/// preprocessor did: in another directory, or in none. The preprocessor
/// searches each package with its own include directories, and a list has
/// one search path for every file in it.
pub(crate) fn misfound<'a>(incdirs: &[PathBuf], summaries: impl Iterator<Item = &'a Summary>) {
    let includes: Vec<(&str, &Include)> = (summaries.flat_map(|summary| &summary.includes))
        .filter_map(|include| Some((include.name.as_deref()?, include)))
        .filter(|(name, _)| !Path::new(name).is_absolute())
        .collect();
    // Each name is searched for once, in parallel: units include the same
    // headers over and over, and a design has many include directories, so
    // the search is most of a stat for each directory a name is not in.
    let names: HashSet<&str> = includes.iter().map(|(name, _)| *name).collect();
    let lists: HashMap<&str, Option<PathBuf>> = (names.into_par_iter())
        .map(|name| {
            let listed = (incdirs.iter())
                .map(|dir| clean(&dir.join(name)))
                .find(|path| path.is_file());
            (name, listed)
        })
        .collect();
    let mut told = HashSet::new();
    for (name, include) in includes {
        let found = clean(&include.path);
        let listed = &lists[name];
        if listed.as_ref() == Some(&found) || !told.insert(include.at.to_string()) {
            continue;
        }
        let problem = match &listed {
            Some(other) => term::warning(format!(
                "a tool given the list reads {} for `{name}`",
                mark::path(other.display())
            ))
            .note(format!("astli read {}", mark::path(found.display())))
            .note("a tool searches the list's include directories in order, for every file in it"),
            None => term::warning(format!("no include directory of the list holds `{name}`")).note(
                format!(
                    "astli read {}, which a tool given the list cannot find",
                    mark::path(found.display())
                ),
            ),
        };
        let place = Place::Line(include.at.at.line as usize);
        (problem.at("include-differs", &include.at.path, place))
            .label("included here")
            .report();
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
