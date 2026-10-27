//! What a run reads: the files a filelist or the command line names, or,
//! with neither, the package the current directory is in and its
//! dependencies.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use astli_index::Summary;
use astli_package::{Declared, Dependencies, Package, mark};
use astli_preproc::Build;

use crate::cli::{BuildArgs, FeatureArgs, RunArgs, Sources};
use crate::error::{Error, Result};
use crate::lockfile;
use crate::sources::{self, Resolved, split_define};
use crate::tops::Chosen;

/// The files of a run, each with the build that preprocesses it.
pub struct Plan {
    /// Each build a unit is preprocessed with: one for a filelist, one per
    /// package found.
    builds: Vec<Build>,
    /// The files compiled on their own.
    pub units: Vec<Unit>,
    /// The files whose findings a run reports, sorted for discovery: each
    /// one a filelist names, or every file the root package holds.
    pub own: Vec<PathBuf>,
    /// What a filelist written for a tool searches: a filelist's own, or
    /// every package's include directories and those its includes found by
    /// path suffix need.
    pub incdirs: Vec<PathBuf>,
    /// What a filelist written for a tool defines, as `NAME` or
    /// `NAME=VALUE`.
    pub defines: Vec<String>,
    /// Each unit's place in `units`, by path.
    by_path: HashMap<PathBuf, usize>,
    /// The package and its dependencies, when discovered; a unit's build is
    /// its package's place among them.
    packages: Option<(Package, Dependencies)>,
    /// The dependencies read as the root describes them, by name.
    pub described: Vec<String>,
    /// What the run elaborates from.
    pub tops: Chosen,
}

/// A file compiled on its own.
pub struct Unit {
    pub path: PathBuf,
    build: usize,
    /// Whether it is the root package's, whose findings are reported; a
    /// dependency's are its authors' to fix.
    pub reported: bool,
}

impl AsRef<Path> for Unit {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Plan {
    /// The files `sources` names, or the package's without any, for a run
    /// from the tops `tops` names, or from none when it takes no `--top`.
    ///
    /// The flags of `build` apply to a filelist's files as they always have,
    /// and to every package's build when there is none: a define is the
    /// consumer's, never a package's, though the root's tables say what its
    /// own tops need. `features` select from the package, so they have
    /// nothing to select from among files named.
    pub fn new(
        sources: &Sources,
        features: &FeatureArgs,
        build: &BuildArgs,
        tops: Option<&[String]>,
        run: &RunArgs,
    ) -> Result<Plan> {
        let named = !(sources.files.is_empty()
            && sources.filelist.is_empty()
            && sources.relative.is_empty());
        match named {
            true if features.given() => Err(Error::failed(format!(
                "{} and {} select from the package, not from files named",
                mark::code("--features"),
                mark::code("--no-features")
            ))),
            true => {
                let mut plan = Plan::listed(sources::resolve(sources, build)?);
                plan.tops = Chosen::named(tops.unwrap_or_default());
                Ok(plan)
            }
            false => Plan::discovered(features, build, tops, run),
        }
    }

    /// The build `unit` is preprocessed with.
    pub fn build(&self, unit: &Unit) -> &Build {
        &self.builds[unit.build]
    }

    /// The dependency called `name` of the package at `root`, if the plan
    /// discovered that package rather than reading files named.
    pub fn dependency(&self, root: &Path, name: &str) -> Option<&Package> {
        let (package, dependencies) = self.packages.as_ref()?;
        if package.root != root {
            return None;
        }
        (dependencies.packages.iter())
            .find(|it| it.name == name)
            .map(|it| &it.package)
    }

    /// Whether the files are a package's, found rather than named, so that
    /// their order is only that of their paths.
    pub fn is_package(&self) -> bool {
        self.packages.is_some()
    }

    /// The unit at `path`, which must be one of `units`.
    pub fn unit(&self, path: &Path) -> &Unit {
        &self.units[self.by_path[path]]
    }

    /// Whether a package of the plan overrides names in another, so that
    /// [`Plan::replace`] needs each unit's summary.
    pub fn overrides(&self) -> bool {
        self.packages.as_ref().is_some_and(|(root, dependencies)| {
            std::iter::once(root)
                .chain(dependencies.packages.iter().map(|it| &it.package))
                .any(|package| !package.overrides.is_empty())
        })
    }

    /// Drops the units an override replaces, given the summary of each, in
    /// the order of `units`; without [`Plan::overrides`] none is needed.
    /// Returns whether each was kept, for the caller to drop what it holds
    /// for the others.
    pub fn replace(&mut self, summaries: &[Summary]) -> Result<Vec<bool>> {
        let (root, dependencies) = match &self.packages {
            Some(packages) if self.overrides() => packages,
            _ => return Ok(vec![true; self.units.len()]),
        };
        let declared: Vec<Declared> = (self.units.iter().zip(summaries))
            .map(|(unit, summary)| Declared {
                package: unit.build,
                path: &unit.path,
                names: (summary.declarations.iter())
                    .map(|declaration| declaration.name.as_str())
                    .collect(),
            })
            .collect();
        let kept: Vec<bool> = (dependencies.replaced(root, &declared)?.into_iter())
            .map(|dropped| !dropped)
            .collect();
        self.units = kept_only(std::mem::take(&mut self.units), &kept);
        self.by_path = by_path(&self.units);
        Ok(kept)
    }

    fn listed(resolved: Resolved) -> Plan {
        let units = (resolved.files.iter())
            .map(|path| Unit {
                path: path.clone(),
                build: 0,
                reported: true,
            })
            .collect();
        let builds = vec![resolved.build()];
        let mut plan = Plan::with(builds, units, resolved.files, resolved.incdir);
        plan.defines = resolved.define;
        plan
    }

    fn discovered(
        features: &FeatureArgs,
        flags: &BuildArgs,
        tops: Option<&[String]>,
        run: &RunArgs,
    ) -> Result<Plan> {
        let mut package = here()?;
        let tops = match tops {
            Some(asked) => Chosen::of(&package, asked)?,
            None => Chosen::default(),
        };
        let (mut dependencies, described) = lockfile::dependencies(&package, run.locked)?;
        dependencies.select(&mut package, &features.selection())?;
        // As a filelist names them: the flags' after the tables', so that a
        // flag wins.
        let defines: Vec<String> = (tops.defines.iter().cloned())
            .chain(define_flags(flags))
            .collect();
        let flagged = |build: Build| with_flags(build, flags, &defines);

        let files = package.files(&dependencies.include_dirs())?;
        let mut incdirs: Vec<PathBuf> = (package.search_dirs().into_iter())
            .chain(files.suffix_dirs.iter().cloned())
            .collect();
        let mut own: Vec<PathBuf> = files.walked.iter().chain(&files.headers).cloned().collect();
        own.sort();
        own.dedup();
        let mut builds = vec![flagged(files.build)];
        let mut units: Vec<Unit> = (files.units.into_iter())
            .map(|path| Unit {
                path,
                build: 0,
                reported: true,
            })
            .collect();

        for dependency in &dependencies.packages {
            let files = (dependency.package)
                .files(&dependencies.include_dirs_of(dependency))
                .map_err(|err| {
                    Error::failed(format!(
                        "{}: {}",
                        mark::name(&dependency.name),
                        err.message()
                    ))
                })?;
            incdirs.extend(dependency.package.search_dirs());
            incdirs.extend(files.suffix_dirs);
            builds.push(flagged(files.build));
            let build = builds.len() - 1;
            units.extend(files.units.into_iter().map(|path| Unit {
                path,
                build,
                reported: false,
            }));
        }
        // As a filelist names them: the flags' after what the packages need.
        incdirs.extend(incdir_flags(flags));
        let mut seen = HashSet::new();
        incdirs.retain(|dir| seen.insert(dir.clone()));
        let mut plan = Plan::with(builds, units, own, incdirs);
        plan.defines = defines;
        plan.packages = Some((package, dependencies));
        plan.described = described;
        plan.tops = tops;
        Ok(plan)
    }

    fn with(
        builds: Vec<Build>,
        units: Vec<Unit>,
        own: Vec<PathBuf>,
        incdirs: Vec<PathBuf>,
    ) -> Plan {
        Plan {
            builds,
            by_path: by_path(&units),
            units,
            own,
            incdirs,
            defines: Vec::new(),
            packages: None,
            described: Vec::new(),
            tops: Chosen::default(),
        }
    }
}

fn by_path(units: &[Unit]) -> HashMap<PathBuf, usize> {
    (units.iter().enumerate())
        .map(|(at, unit)| (unit.path.clone(), at))
        .collect()
}

/// `values` less those `kept` says were not, in order.
pub fn kept_only<T>(values: Vec<T>, kept: &[bool]) -> Vec<T> {
    (values.into_iter().zip(kept))
        .filter(|(_, kept)| **kept)
        .map(|(value, _)| value)
        .collect()
}

/// `build` with the include directories the flags give, and `defines`.
fn with_flags(build: Build, flags: &BuildArgs, defines: &[String]) -> Build {
    let build = incdir_flags(flags).fold(build, Build::include_dir);
    defines.iter().fold(build, |build, define| {
        let (name, body) = split_define(define);
        build.define(name, body)
    })
}

/// The include directories the flags give, `+incdir+`s first.
fn incdir_flags(flags: &BuildArgs) -> impl Iterator<Item = PathBuf> + '_ {
    (flags.incdir_plus.iter())
        .flat_map(|arg| crate::filelist::plus(arg))
        .map(PathBuf::from)
        .chain(flags.incdir.iter().cloned())
}

/// The definitions the flags give, as written, `+define+`s first.
fn define_flags(flags: &BuildArgs) -> Vec<String> {
    (flags.define_plus.iter())
        .flat_map(|arg| crate::filelist::plus(arg))
        .map(String::from)
        .chain(flags.define.iter().cloned())
        .collect()
}

/// The files `sources` names, or every file the package holds without any:
/// what `fmt` reads, which needs no dependency and no build.
pub fn own_files(sources: &Sources) -> Result<Vec<PathBuf>> {
    let named =
        !(sources.files.is_empty() && sources.filelist.is_empty() && sources.relative.is_empty());
    if named {
        return Ok(sources::resolve(sources, &BuildArgs::default())?.files);
    }
    let files = here()?.files(&[])?;
    let mut own: Vec<PathBuf> = files.walked.into_iter().chain(files.headers).collect();
    own.sort();
    own.dedup();
    Ok(own)
}

/// The package the current directory is in.
fn here() -> Result<Package> {
    let here = std::env::current_dir().map_err(|err| Error::io(".", err))?;
    Package::find(&here)?.ok_or_else(|| {
        Error::failed(
            "no input files, and no package here: name a file, point -f at a filelist, \
             or run inside a git repository or beside an astli.toml",
        )
    })
}
