//! What a run reads: the files a filelist or the command line names, or,
//! with neither, the package the current directory is in and its
//! dependencies.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use astli_index::Summary;
use astli_package::{Declared, Dependencies, Dependency, Files, Package, Workspace, mark};
use astli_preproc::Build;

use crate::cli::{BuildArgs, FeatureArgs, RunArgs, Sources};
use crate::cmd;
use crate::error::{Error, Result};
use crate::lockfile;
use crate::sources::{self, Resolved, split_define};
use crate::tops::Chosen;

/// What a package's walk depends on that differs between the designs of a
/// workspace: the package, the features enabled, what it excludes, which
/// as a dependency holds its `dev` files too, and the include directories
/// its dependencies export.
pub type Walk = (PathBuf, BTreeSet<String>, Vec<String>, Vec<PathBuf>);

/// Each package's files as walked for a design, by what the walk read, for
/// a command run for each package.
static WALKED: LazyLock<Mutex<HashMap<Walk, Files>>> = LazyLock::new(Mutex::default);

/// What `package`'s walk, searching `dependencies`' include directories,
/// depends on.
fn walk(package: &Package, dependencies: &[PathBuf]) -> Walk {
    (
        package.root.clone(),
        package.enabled.clone(),
        package.exclude.clone(),
        dependencies.to_vec(),
    )
}

/// What a build is made from, for a command run for each package: the walk
/// whose build it extends and the run's defines, since the flags are the
/// same for every package.
pub type Made = (Walk, Vec<String>);

/// `package`'s files, searching `dependencies`' include directories after
/// its own. Run for each package of a workspace, the designs hold mostly
/// the same packages alike, so each is walked once for all of them.
fn files_of(
    package: &Package,
    dependencies: &[PathBuf],
) -> std::result::Result<Files, astli_package::Error> {
    if !cmd::several() {
        return package.files(dependencies);
    }
    let walk = walk(package, dependencies);
    let walked = WALKED.lock().unwrap_or_else(|it| it.into_inner());
    if let Some(files) = walked.get(&walk) {
        return Ok(files.clone());
    }
    drop(walked);
    let files = package.files(dependencies)?;
    let mut walked = WALKED.lock().unwrap_or_else(|it| it.into_inner());
    walked.insert(walk, files.clone());
    Ok(files)
}

/// The files of a run, each with the build that preprocesses it.
pub struct Plan {
    /// Each build a unit is preprocessed with: one for a filelist, one per
    /// package found.
    builds: Vec<Build>,
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
    packages: Option<(Package, Dependencies)>,
    /// The workspace the package is in, if discovered in one.
    workspace: Option<Workspace>,
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

    /// The package and its dependencies, if the plan discovered them rather
    /// than reading files named.
    pub fn packages(&self) -> Option<&(Package, Dependencies)> {
        self.packages.as_ref()
    }

    /// The package holding what a workspace shares, such as the build
    /// directory and the profiles, if the plan discovered the packages: the
    /// workspace's root, or the package itself outside one.
    pub fn shared(&self) -> Option<&Package> {
        match &self.workspace {
            Some(workspace) => Some(&workspace.root),
            None => self.packages.as_ref().map(|(package, _)| package),
        }
    }

    /// Whether the package is in a workspace, whose root's manifest then
    /// holds what the workspace shares.
    pub fn in_workspace(&self) -> bool {
        self.workspace.is_some()
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
                on_disk: false,
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
        let here = lockfile::Here::of(here()?)?;
        let tops = match tops {
            Some(asked) => Chosen::of(&here.package, asked)?,
            None => Chosen::default(),
        };
        let (mut dependencies, described, on_disk) = lockfile::dependencies(&here, run.locked)?;
        let lockfile::Here {
            mut package,
            workspace,
        } = here;
        dependencies.select(&mut package, &features.selection())?;
        // As a filelist names them: the flags' after the tables', so that a
        // flag wins.
        let defines: Vec<String> = (tops.defines.iter().cloned())
            .chain(define_flags(flags))
            .collect();
        let flagged = |build: Build| with_flags(build, flags, &defines);

        let several = cmd::several();
        let mut made = Vec::new();
        if several {
            made.push((
                walk(&package, &dependencies.include_dirs()),
                defines.clone(),
            ));
        }
        let files = files_of(&package, &dependencies.include_dirs())?;
        let mut incdirs: Vec<PathBuf> = (package.search_dirs().into_iter())
            .chain(files.suffix_dirs.iter().cloned())
            .collect();
        let suffix_dirs = files.suffix_dirs.clone();
        let mut own: Vec<PathBuf> = files.walked.iter().chain(&files.headers).cloned().collect();
        own.sort();
        own.dedup();
        let mut builds = vec![flagged(files.build)];
        let mut units: Vec<Unit> = (files.units.into_iter())
            .map(|path| Unit {
                path,
                build: 0,
                reported: true,
                on_disk: false,
            })
            .collect();

        for dependency in &dependencies.packages {
            let files = files_of(
                &dependency.package,
                &dependencies.include_dirs_of(dependency),
            )
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
            if several {
                let searched = dependencies.include_dirs_of(dependency);
                made.push((walk(&dependency.package, &searched), defines.clone()));
            }
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
        plan.made = made;
        plan.defines = defines;
        plan.suffix_dirs = suffix_dirs;
        plan.packages = Some((package, dependencies));
        plan.workspace = workspace;
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
            made: Vec::new(),
            by_path: by_path(&units),
            units,
            own,
            incdirs,
            suffix_dirs: Vec::new(),
            defines: Vec::new(),
            packages: None,
            workspace: None,
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

/// The files `sources` names, or every file the package's walk finds without
/// any: what `fmt` reads, which needs no dependency and no build.
///
/// A header the walk does not find, of another extension or outside the
/// sources, is left out rather than found by reading every file for the
/// `` `include``s that name it.
pub fn own_files(sources: &Sources) -> Result<Vec<PathBuf>> {
    let named =
        !(sources.files.is_empty() && sources.filelist.is_empty() && sources.relative.is_empty());
    if named {
        return Ok(sources::resolve(sources, &BuildArgs::default())?.files);
    }
    Ok(here()?.walk()?)
}

/// The package the current directory is in, or the one `-p` chose.
fn here() -> Result<Package> {
    Package::find(&lockfile::dir()?)?.ok_or_else(|| {
        Error::failed(
            "no input files, and no package here: name a file, point -f at a filelist, \
             or run inside a git repository or beside an astli.toml",
        )
    })
}
