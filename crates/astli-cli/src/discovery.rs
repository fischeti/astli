//! What a run reads: the files a filelist or the command line names, or,
//! with neither, the package the current directory is in and its
//! dependencies.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use astli_package::Package;
use astli_preproc::Build;

use crate::cli::{BuildArgs, Sources};
use crate::error::{Error, Result};
use crate::sources::{self, Resolved, split_define};

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
    /// The files `sources` names, or the package's without any.
    ///
    /// The flags of `build` apply to a filelist's files as they always have,
    /// and to every package's build when there is none: a define is the
    /// consumer's, never a package's.
    pub fn new(sources: &Sources, build: &BuildArgs) -> Result<Plan> {
        let named = !(sources.files.is_empty()
            && sources.filelist.is_empty()
            && sources.relative.is_empty());
        match named {
            true => Ok(Plan::listed(sources::resolve(sources, build)?)),
            false => Plan::discovered(build),
        }
    }

    /// The build `unit` is preprocessed with.
    pub fn build(&self, unit: &Unit) -> &Build {
        &self.builds[unit.build]
    }

    /// The unit at `path`, which must be one of `units`.
    pub fn unit(&self, path: &Path) -> &Unit {
        &self.units[self.by_path[path]]
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
        Plan::with(
            builds,
            units,
            resolved.files,
            resolved.incdir,
            resolved.define,
        )
    }

    fn discovered(flags: &BuildArgs) -> Result<Plan> {
        let package = here()?;
        let dependencies = package.bender()?.unwrap_or_default();
        let flagged = |build: Build| with_flags(build, flags);

        let files = package.files(&dependencies.include_dirs())?;
        let mut incdirs: Vec<PathBuf> = (package.include_dirs.iter())
            .chain(&files.suffix_dirs)
            .cloned()
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
                .map_err(|err| Error::failed(format!("{}: {err}", dependency.name)))?;
            incdirs.extend(dependency.package.include_dirs.iter().cloned());
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
        Ok(Plan::with(builds, units, own, incdirs, define_flags(flags)))
    }

    fn with(
        builds: Vec<Build>,
        units: Vec<Unit>,
        own: Vec<PathBuf>,
        incdirs: Vec<PathBuf>,
        defines: Vec<String>,
    ) -> Plan {
        let by_path = (units.iter().enumerate())
            .map(|(at, unit)| (unit.path.clone(), at))
            .collect();
        Plan {
            builds,
            units,
            own,
            incdirs,
            defines,
            by_path,
        }
    }
}

/// `build` with the include directories and definitions the flags give.
fn with_flags(build: Build, flags: &BuildArgs) -> Build {
    let build = incdir_flags(flags).fold(build, Build::include_dir);
    define_flags(flags).iter().fold(build, |build, define| {
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
