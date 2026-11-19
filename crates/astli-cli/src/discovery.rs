//! What a run reads: the files a filelist or the command line names, or,
//! with neither, the package the current directory is in and its
//! dependencies.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use astli_index::Summary;
use astli_package::{
    Declared, Dependencies, Dependency, Files, Package, Patches, Root, Walk, mark,
};
use astli_preproc::Build;

use crate::cli::{BuildArgs, FeatureArgs, Sources};
use crate::error::{Error, Result};
use crate::lockfile::{self, Target};
use crate::run::Run;
use crate::sources::{self, Resolved, split_define};
use crate::tops::Chosen;

/// What a build is made from, for a command run for each package: the walk
/// whose build it extends, by its place among the run's, and the run's
/// defines. The flags are the same for every package of a run.
pub type Made = (usize, Vec<String>);

/// `package`'s files, walked as `walk` says and searching `dependencies`'
/// include directories after its own, and for a command run for each
/// package, the walk's place among the run's. The designs of a workspace
/// hold mostly the same packages alike, so each is walked once for all of
/// them.
fn files_of(
    run: &Run,
    package: &Package,
    walk: &Walk,
    dependencies: &[PathBuf],
) -> std::result::Result<(Option<usize>, Files), astli_package::Error> {
    let files = || package.files(walk, dependencies);
    if !run.several_packages() {
        return Ok((None, files()?));
    }
    let (at, files) = run.walked(package, walk, dependencies, files)?;
    Ok((Some(at), files))
}

/// The files of a run, each with the build that preprocesses it.
#[derive(Clone)]
pub struct Plan {
    /// Each build a unit is preprocessed with: one for a filelist, one per
    /// package found.
    builds: Vec<Build>,
    /// Each package's build before any define, in the order of `builds`,
    /// for [`Plan::define`] to define what the tops need; empty for files
    /// named.
    bare: Vec<Build>,
    /// What the flags define, as `defines` holds it, after what the tops
    /// do.
    flagged: Vec<String>,
    /// What each build is made from, in the order of `builds`, for a
    /// command run for each package; empty otherwise.
    made: Vec<Made>,
    /// The files compiled on their own.
    pub units: Vec<Unit>,
    /// The files whose findings a run reports, sorted for discovery: each
    /// one a filelist names, or every file the root package holds.
    pub own: Vec<PathBuf>,
    /// What a filelist written for a tool searches: a filelist's own, or
    /// every package's include directories and those its includes found by
    /// path suffix need.
    pub incdirs: Vec<PathBuf>,
    /// The directories the root package's includes found only by path
    /// suffix are in, when discovered.
    pub suffix_dirs: Vec<PathBuf>,
    /// What a filelist written for a tool defines, as `NAME` or
    /// `NAME=VALUE`.
    pub defines: Vec<String>,
    /// Each unit's place in `units`, by path.
    by_path: HashMap<PathBuf, usize>,
    /// The package and its dependencies, when discovered; a unit's build is
    /// its package's place among them.
    packages: Option<(Root, Dependencies)>,
    /// The package holding what a workspace shares, if discovered: the
    /// workspace's root, or the package itself outside one.
    shared: Option<Root>,
    /// Whether the package is in a workspace, whose root's manifest then
    /// holds what the workspace shares.
    in_workspace: bool,
    /// The dependencies read as the root describes them, by name.
    pub described: Vec<String>,
}

/// A file compiled on its own.
#[derive(Clone)]
pub struct Unit {
    pub path: PathBuf,
    build: usize,
    /// Whether it is the root package's, whose findings are reported; a
    /// dependency's are its authors' to fix.
    pub reported: bool,
    /// Whether it is a dependency's read from a directory on disk by a
    /// `path` requirement, whose authors are likely the root's: `check`
    /// reports its findings as it does the root's, since only a dependent
    /// elaborates it with the values it is given.
    pub on_disk: bool,
}

impl AsRef<Path> for Unit {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Plan {
    /// The files `sources` names, or without any those of the package
    /// `target` chose, built with no top's defines until
    /// [`Plan::define`] gives them.
    ///
    /// The flags of `build` apply to a filelist's files as they always have,
    /// and to every package's build when there is none: a define is the
    /// consumer's, never a package's, though the root's tables say what its
    /// own tops need. `features` select from the package, so they have
    /// nothing to select from among files named.
    pub fn new(
        target: &Target,
        sources: &Sources,
        features: &FeatureArgs,
        build: &BuildArgs,
        run: &Run,
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
            true => Ok(Plan::listed(sources::resolve(run, sources, build)?)),
            false => Plan::discovered(target, features, build, run),
        }
    }

    /// Builds a package's files with what the tables of `tops` define, in
    /// place of what any tops before them did; files named are built as
    /// the flags say.
    pub fn define(&mut self, tops: &Chosen) {
        if !self.is_package() {
            return;
        }
        self.defines = (tops.defines.iter().chain(&self.flagged))
            .cloned()
            .collect();
        self.builds = (self.bare.iter())
            .map(|build| defined(build.clone(), &self.defines))
            .collect();
        for (_, defines) in &mut self.made {
            defines.clone_from(&self.defines);
        }
    }

    /// The plan of the same files for `tops`, as [`Plan::define`] makes it.
    pub fn for_tops(&self, tops: &Chosen) -> Plan {
        let mut plan = self.clone();
        plan.define(tops);
        plan
    }

    /// The build `unit` is preprocessed with.
    pub fn build(&self, unit: &Unit) -> &Build {
        &self.builds[unit.build]
    }

    /// The dependency called `name` of the package at `root`, if the plan
    /// discovered that package rather than reading files named.
    pub fn dependency(&self, root: &Path, name: &str) -> Option<&Package> {
        let (package, dependencies) = self.packages.as_ref()?;
        if package.package.root != root {
            return None;
        }
        (dependencies.packages.iter())
            .find(|it| it.name == name)
            .map(|it| &it.package)
    }

    /// The package and its dependencies, if the plan discovered them rather
    /// than reading files named.
    pub fn packages(&self) -> Option<&(Root, Dependencies)> {
        self.packages.as_ref()
    }

    /// The package holding what a workspace shares, such as the build
    /// directory and the profiles, if the plan discovered the packages: the
    /// workspace's root, or the package itself outside one.
    pub fn shared(&self) -> Option<&Root> {
        self.shared.as_ref()
    }

    /// Whether the package is in a workspace, whose root's manifest then
    /// holds what the workspace shares.
    pub fn in_workspace(&self) -> bool {
        self.in_workspace
    }

    /// Whether the files are a package's, found rather than named, so that
    /// their order is only that of their paths.
    pub fn is_package(&self) -> bool {
        self.packages.is_some()
    }

    /// Where the unit at `at` in `units` comes in a list grouped by package:
    /// the dependencies first, each after those it requires, and the root
    /// last. Every file named is in the one group.
    pub fn group(&self, at: usize) -> usize {
        match self.units[at].build {
            0 => self.builds.len(),
            build => build,
        }
    }

    /// The dependency `unit` is a file of, if the plan discovered the
    /// packages and it is not the root's.
    pub fn dependency_of(&self, unit: &Unit) -> Option<&Dependency> {
        let (_, dependencies) = self.packages.as_ref()?;
        let build = unit.build.checked_sub(1)?;
        dependencies.packages.get(build)
    }

    /// What `unit`'s build is made from, for a command run for each
    /// package.
    pub fn made(&self, unit: &Unit) -> Option<&Made> {
        self.made.get(unit.build)
    }

    /// The unit at `path`, which must be one of `units`.
    pub fn unit(&self, path: &Path) -> &Unit {
        &self.units[self.by_path[path]]
    }

    /// Whether a package of the plan overrides names in another, so that
    /// [`Plan::replace`] needs each unit's summary.
    pub fn overrides(&self) -> bool {
        self.packages.as_ref().is_some_and(|(root, dependencies)| {
            std::iter::once(&root.package)
                .chain(dependencies.packages.iter().map(|it| &it.package))
                .any(|package| !package.manifest.overrides.is_empty())
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
        let kept: Vec<bool> = (dependencies.replaced(&root.package, &declared)?.into_iter())
            .map(|dropped| !dropped)
            .collect();
        self.keep(&kept);
        Ok(kept)
    }

    /// Leaves out the files at `paths`, which could not be read: the rest
    /// is analysed without what they declare, as it is without what a file
    /// holds that did not parse.
    pub fn unread(&mut self, paths: &[PathBuf]) {
        if paths.is_empty() {
            return;
        }
        self.own.retain(|path| !paths.contains(path));
        let kept: Vec<bool> = (self.units.iter())
            .map(|unit| !paths.contains(&unit.path))
            .collect();
        self.keep(&kept);
    }

    /// Drops the units `kept` says were not kept, in the order of `units`.
    fn keep(&mut self, kept: &[bool]) {
        self.units = kept_only(std::mem::take(&mut self.units), kept);
        self.by_path = by_path(&self.units);
    }

    fn listed(resolved: Resolved) -> Plan {
        let units = (resolved.files.iter())
            .map(|path| Unit {
                path: path.clone(),
                build: 0,
                reported: true,
                on_disk: false,
            })
            .collect();
        let builds = vec![resolved.build()];
        let mut plan = Plan::with(builds, units, resolved.files, resolved.incdir);
        plan.defines = resolved.define;
        plan
    }

    fn discovered(
        target: &Target,
        features: &FeatureArgs,
        flags: &BuildArgs,
        run: &Run,
    ) -> Result<Plan> {
        let here = target.here_of(run, package(target)?)?;
        let (mut dependencies, described, on_disk) =
            lockfile::dependencies(run, &here, run.args.locked)?;
        let lockfile::Here {
            package,
            workspace,
            in_workspace,
        } = here;
        let walk = Walk {
            enabled: dependencies.select(&package.package, &features.selection())?,
            held: None,
            patched: workspace.patches.0.values().cloned().collect(),
        };
        // As a filelist names them: the flags' after the tables', so that a
        // flag wins.
        let flagged = define_flags(flags);
        let defines = flagged.clone();
        let mut bare = Vec::new();
        let mut with_flags = |build: Build| {
            let build = incdir_flags(flags).fold(build, Build::include_dir);
            bare.push(build.clone());
            defined(build, &defines)
        };

        let mut made = Vec::new();
        let after = dependencies.include_dirs();
        let (at, files) = files_of(run, &package.package, &walk, &after)?;
        made.extend(at.map(|at| (at, defines.clone())));
        let mut incdirs: Vec<PathBuf> = (package.package.search_dirs(&walk.enabled).into_iter())
            .chain(files.suffix_dirs.iter().cloned())
            .collect();
        let suffix_dirs = files.suffix_dirs.clone();
        let mut own: Vec<PathBuf> = files.walked.iter().chain(&files.headers).cloned().collect();
        own.sort();
        own.dedup();
        let mut builds = vec![with_flags(files.build)];
        let mut units: Vec<Unit> = (files.units.into_iter())
            .map(|path| Unit {
                path,
                build: 0,
                reported: true,
                on_disk: false,
            })
            .collect();

        for dependency in &dependencies.packages {
            let (at, files) = files_of(
                run,
                &dependency.package,
                &dependency.walk,
                &dependencies.include_dirs_of(dependency),
            )
            .map_err(|err| {
                Error::failed(format!(
                    "{}: {}",
                    mark::name(&dependency.name),
                    err.message()
                ))
            })?;
            incdirs.extend(dependency.search_dirs());
            incdirs.extend(files.suffix_dirs);
            builds.push(with_flags(files.build));
            made.extend(at.map(|at| (at, defines.clone())));
            let build = builds.len() - 1;
            let on_disk = on_disk.contains(&dependency.name);
            units.extend(files.units.into_iter().map(|path| Unit {
                path,
                build,
                reported: false,
                on_disk,
            }));
        }
        // As a filelist names them: the flags' after what the packages need.
        incdirs.extend(incdir_flags(flags));
        let mut seen = HashSet::new();
        incdirs.retain(|dir| seen.insert(dir.clone()));
        let mut plan = Plan::with(builds, units, own, incdirs);
        plan.bare = bare;
        plan.flagged = flagged;
        plan.made = made;
        plan.defines = defines;
        plan.suffix_dirs = suffix_dirs;
        plan.packages = Some((package, dependencies));
        plan.shared = Some(workspace.root);
        plan.in_workspace = in_workspace;
        plan.described = described;
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
            bare: Vec::new(),
            flagged: Vec::new(),
            made: Vec::new(),
            by_path: by_path(&units),
            units,
            own,
            incdirs,
            suffix_dirs: Vec::new(),
            defines: Vec::new(),
            packages: None,
            shared: None,
            in_workspace: false,
            described: Vec::new(),
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

/// `build` with `defines`.
fn defined(build: Build, defines: &[String]) -> Build {
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

/// The files `sources` names, or every file the walk of the package `target`
/// chose finds without any: what `fmt` reads, which needs no dependency and
/// no build.
///
/// A header the walk does not find, of another extension or outside the
/// sources, is left out rather than found by reading every file for the
/// `` `include``s that name it.
pub fn own_files(run: &Run, target: &Target, sources: &Sources) -> Result<Vec<PathBuf>> {
    let named =
        !(sources.files.is_empty() && sources.filelist.is_empty() && sources.relative.is_empty());
    if named {
        return Ok(sources::resolve(run, sources, &BuildArgs::default())?.files);
    }
    // Its own patches, since only a run of the package itself reaches here.
    let package = package(target)?.package;
    let patches = Patches::read(&package.root)?;
    Ok(package.walk(&Walk::own(&package, &patches))?)
}

/// The package `target` chose, for a run that names no file.
fn package(target: &Target) -> Result<Root> {
    target.package()?.ok_or_else(|| {
        Error::failed(
            "no input files, and no package here: name a file, point -f at a filelist, \
             or run inside a git repository or beside an astli.toml",
        )
    })
}
