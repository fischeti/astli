//! The list of a design's files a tool is given: the units its tops reach,
//! what choosing them warns of, and the list itself, which `files` writes
//! and `build` gives its simulator. `check` warns of what `files` would, and
//! `pickle` chooses its files as `files` does.

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use astli_index::{Declaration, Include, Index, Summary, Uses, summarize};
use astli_package::{Dependency, mark};
use astli_text::{Diagnostic, Reader, clean};
use rayon::prelude::*;

use crate::discovery::Plan;
use crate::error::{Error, Result};
use crate::filelist::quoted;
use crate::render::Report;
use crate::run::Run;
use crate::session;
use crate::term::{self, Place};

/// The list of a design's files, as `files` writes it and `build` gives
/// its simulator.
pub(crate) struct Listing {
    /// The units listed, in the order written.
    kept: Vec<usize>,
    /// The units left out of the list as stubs.
    stubs: Vec<usize>,
    /// The include directories a tool searches, in order.
    incdirs: Vec<PathBuf>,
    /// What the list defines, as `NAME` or `NAME=VALUE`.
    defines: Vec<String>,
}

impl Listing {
    /// The list of `reached`, the units of `plan` the tops reach, or of
    /// every unit for `None`, given what `index` says of each, ordered if
    /// `order`.
    ///
    /// A stub, a file of `extern` declarations alone, is left out: a tool
    /// given the list compiles each instance against the library the stub
    /// stands for, which a list of sources leaves to it, and not every tool
    /// reads a bare `extern`.
    pub(crate) fn of(
        plan: &Plan,
        index: &Index,
        reached: Option<Vec<usize>>,
        order: bool,
    ) -> Listing {
        let top = reached.is_some();
        let reached = reached.unwrap_or_else(|| (0..index.summaries().len()).collect());
        let (stubs, mut kept): (Vec<usize>, Vec<usize>) =
            reached.into_iter().partition(|&file| index.stub(file));
        if order {
            // Sorted by package first, so that each package's files stay
            // together and the list reads as the packages depend on each
            // other: a file still follows what it uses wherever that is.
            kept.sort_by_key(|&file| plan.group(file));
            kept = index.ordered(&kept);
        }
        let incdirs = incdirs(plan, index, top.then_some(&kept));
        Listing {
            kept,
            stubs,
            incdirs,
            defines: plan.defines.clone(),
        }
    }

    /// What `index` says of each unit listed, in order.
    pub(crate) fn summaries<'a>(
        &'a self,
        index: &'a Index,
    ) -> impl Iterator<Item = &'a Summary> + Clone {
        self.kept.iter().map(|&file| &index.summaries()[file])
    }

    /// Notes each stub left out of the list.
    pub(crate) fn note_stubs(&self, run: &Run, index: &Index) {
        for &file in &self.stubs {
            term::note(format!(
                "{}: `extern` declarations alone, so it is left out of the list",
                index.summaries()[file].path.display()
            ))
            .report(run);
        }
    }

    /// Warns, for a package, of each header a tool given the list finds
    /// otherwise than astli did, as [`misfound`] tells it.
    pub(crate) fn misfound(&self, run: &Run, plan: &Plan, index: &Index) {
        if plan.is_package() {
            misfound(run, &self.incdirs, self.summaries(index));
        }
    }

    /// Writes the list: its include directories, its defines and its files.
    pub(crate) fn write(&self, out: &mut dyn Write, index: &Index) -> io::Result<()> {
        self.write_incdirs(out)?;
        for define in &self.defines {
            writeln!(out, "{}", quoted(&format!("+define+{define}")))?;
        }
        self.write_files(out, index)
    }

    /// Writes the list's include directories alone.
    pub(crate) fn write_incdirs(&self, out: &mut dyn Write) -> io::Result<()> {
        for dir in &self.incdirs {
            writeln!(out, "{}", quoted(&format!("+incdir+{}", dir.display())))?;
        }
        Ok(())
    }

    /// Writes the list's files alone.
    pub(crate) fn write_files(&self, out: &mut dyn Write, index: &Index) -> io::Result<()> {
        for summary in self.summaries(index) {
            writeln!(out, "{}", quoted(&summary.path.display().to_string()))?;
        }
        Ok(())
    }
}

/// Expands and summarizes one file, read with what it includes through
/// `reader`, rendering what its expansion and parser reported that `told`
/// keeps of it, and counting the errors that holds.
pub(crate) fn summarized(
    path: &Path,
    build: &astli_preproc::Build,
    reader: &dyn Reader,
    told: impl FnOnce(Vec<Diagnostic>) -> Vec<Diagnostic>,
) -> Result<(Summary, Report)> {
    let mut opened = session::open(path, build, reader)?;
    let (summary, diagnostics) = summarize(&mut opened.session, opened.file);
    let said = Report::of(opened.session.origins(), &told(diagnostics))?;
    Ok((summary, said))
}

/// Of what a dependency's file says, what says an `` `include `` found no
/// one header, with where its include directories are named.
pub(crate) fn short_of_a_header(
    diagnostics: Vec<Diagnostic>,
    of: Option<&Dependency>,
) -> Vec<Diagnostic> {
    (diagnostics.into_iter())
        .filter(unresolved_include)
        .map(|diagnostic| in_dependency(diagnostic, of))
        .collect()
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
fn in_dependency(diagnostic: Diagnostic, dependency: Option<&Dependency>) -> Diagnostic {
    let Some(dependency) = dependency else {
        return diagnostic;
    };
    let name = &dependency.name;
    match dependency.package.manifest_path {
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
pub(crate) fn kept(
    run: &Run,
    index: &Index,
    tops: &[String],
    reported: &[usize],
) -> Result<Vec<usize>> {
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
            .report(run);
        }
    }
    warn(run, index, &told);
    redeclared(run, index, &told, tops)?;
    Ok(kept)
}

/// Reports each name declared more than once that `kept` declares or uses:
/// an error if it is used or a top, and a warning otherwise.
pub(crate) fn redeclared(run: &Run, index: &Index, kept: &[usize], tops: &[String]) -> Result {
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
        problem.report(run);
    }
    match refused {
        0 => Ok(()),
        1 => Err(Error::failed("1 name declared more than once is used")),
        n => Err(Error::failed(format!(
            "{n} names declared more than once are used"
        ))),
    }
}

fn warn(run: &Run, index: &Index, kept: &[usize]) {
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
        .report(run);
    }
}

/// Where `declaration` names what it declares.
fn named(declaration: &Declaration) -> Place {
    Place::Name(declaration.location.at, declaration.name.chars().count())
}

/// The include directories a tool given the list of `listed`, units of
/// `plan`, searches: those that hold a header one of them reads, or for
/// `None`, a list of every unit, every one of the plan's.
fn incdirs(plan: &Plan, index: &Index, listed: Option<&[usize]>) -> Vec<PathBuf> {
    match listed {
        None => plan.incdirs.clone(),
        Some(listed) => used(
            &plan.incdirs,
            listed.iter().map(|&file| &index.summaries()[file]),
        ),
    }
}

/// The include directories that hold a header one of `summaries` read.
fn used<'a>(incdirs: &[PathBuf], summaries: impl Iterator<Item = &'a Summary>) -> Vec<PathBuf> {
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
pub(crate) fn foreign_binds(run: &Run, plan: &Plan, index: &Index, kept: &[usize]) {
    for &file in kept {
        let Some(dependency) = plan.dependency_of(&plan.units[file]) else {
            continue;
        };
        let name = &dependency.name;
        if dependency.package.manifest_path.is_some() || plan.described.contains(name) {
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
        .report(run);
    }
}

/// Warns of each header of `summaries` that a tool given `incdirs`, which
/// searches them alone and in their order, would find otherwise than the
/// preprocessor did: in another directory, or in none. The preprocessor
/// searches each package with its own include directories, and a list has
/// one search path for every file in it.
fn misfound<'a>(run: &Run, incdirs: &[PathBuf], summaries: impl Iterator<Item = &'a Summary>) {
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
            .report(run);
    }
}
