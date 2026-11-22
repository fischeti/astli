//! What a run reads: the files a filelist or the command line names, or,
//! with neither, the package the current directory is in and its
//! dependencies.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use astli_index::Summary;
use astli_package::{
    Base, Declared, Dependencies, Dependency, Files, Package, Patches, Root, Selection, Walk, mark,
};
use astli_preproc::Build;
use astli_resolve::Loaded;

use crate::cli::{BuildArgs, FeatureArgs, Sources};
use crate::error::{Error, Result};
use crate::run::Run;
use crate::settle;
use crate::sources::{self, Resolved, defined};
use crate::target::{self, Target};
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
    let files = || package.files(walk, dependencies, run.reader());
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
    /// The files left out as [`Plan::unread`], which another selection of
    /// the package leaves out too.
    unread: Vec<PathBuf>,
    /// What [`Plan::for_tops`] makes another plan from, when discovered.
    again: Option<Again>,
}

/// What a discovered plan was made from, but for the features it selects:
/// what another selection of the same package is made from.
#[derive(Clone)]
struct Again {
    here: target::Here,
    features: FeatureArgs,
    selection: Selection,
    /// What the flags add to every package's build: its defines and its
    /// include directories.
    defines: Vec<String>,
    incdirs: Vec<PathBuf>,
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

/// Whose files a command reports the findings of, as [`Plan::shown`] says.
#[derive(Clone, Copy)]
pub enum Whose {
    /// The root package's, or every file named.
    Own,
    /// The root package's, and each dependency's a `path` requirement
    /// reads from disk.
    OnDisk,
}

impl Whose {
    /// Whether the findings of `unit` are reported.
    pub fn takes(self, unit: &Unit) -> bool {
        match self {
            Whose::Own => unit.reported,
            Whose::OnDisk => unit.reported || unit.on_disk,
        }
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
        match sources.named() {
            true if features.given() => Err(Error::failed(format!(
                "{} and {} select from the package, not from files named",
                mark::code("--features"),
                mark::code("--no-features")
            ))),
            true => Ok(Plan::listed(sources::resolve(run, sources, build)?)),
            false => {
                let again = Again {
                    here: target.here_of(run, package(target)?)?,
                    features: features.clone(),
                    selection: features.selection(),
                    defines: build.defines().collect(),
                    incdirs: build.incdirs().collect(),
                };
                Plan::discovered(again, run)
            }
        }
    }

    /// What `check` with no argument reads for the source file at `path`:
    /// the package it is in, found at or above its directory, with that
    /// package's default features and no flag; or, outside any package, the
    /// file alone, as `check` reads a file named.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "for a caller that is no command, which none is yet"
        )
    )]
    pub fn load(path: &Path, run: &Run) -> Result<Plan> {
        let dir = (path.parent())
            .filter(|it| !it.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let target = Target::new(dir.to_path_buf());
        let sources = match target.package()? {
            Some(_) => Sources::default(),
            None => Sources {
                files: vec![path.to_path_buf()],
                ..Sources::default()
            },
        };
        let (features, build) = (FeatureArgs::default(), BuildArgs::default());
        Plan::new(&target, &sources, &features, &build, run)
    }

    /// The plan of every one of the package's own features, alternatives
    /// together, for what reads each file as written; files named are read
    /// as they are.
    pub fn every_feature(self, run: &Run) -> Result<Plan> {
        let selection = Selection {
            base: Base::All,
            features: Vec::new(),
        };
        self.selecting(selection, run)
    }

    /// The plan for the tops `chosen`, which enables the features their
    /// tables name: this one, if it enables them already, or else the
    /// package discovered again with them. Files named are built as they
    /// are.
    pub fn for_tops(self, chosen: &Chosen, run: &Run) -> Result<Plan> {
        let Some(again) = &self.again else {
            return Ok(self);
        };
        let selection = (again.features).for_tops(!chosen.replaced, &chosen.features);
        self.selecting(selection, run)
    }

    /// The plan of the package as `selection` selects its features: this
    /// one, if it enables the same of the package's own already.
    fn selecting(self, selection: Selection, run: &Run) -> Result<Plan> {
        let (Some(again), Some((root, _))) = (&self.again, &self.packages) else {
            return Ok(self);
        };
        let package = &root.package;
        if selection.own(package) == again.selection.own(package) {
            return Ok(self);
        }
        let again = Again {
            selection,
            ..again.clone()
        };
        let mut plan = Plan::discovered(again, run)?;
        plan.unread(&self.unread);
        Ok(plan)
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

    /// The units whose findings a command reports, in order: those `whose`
    /// takes, of the units the tops reach if `reached` gives them, or else
    /// of every unit. The commands differ on purpose:
    ///
    /// - `check` takes what the tops reach, [`Whose::OnDisk`]: an error in
    ///   a `path` dependency's file breaks the build, its authors are
    ///   likely the root's, and only a dependent elaborates it with the
    ///   values it is given. A file no top builds is another top's to
    ///   check.
    /// - `lint`'s design rules take what the tops reach, [`Whose::Own`]: a
    ///   dependency's style is its authors' to fix, on disk or not, and a
    ///   file no top builds would be judged at defaults a root's modules
    ///   rarely build at.
    /// - `elab` takes every file, reached or not, [`Whose::Own`]: it prints
    ///   what their expansion and parser say beside a tree built from all
    ///   of them, and fails for none of it.
    /// - `files` takes every file without a top, [`Whose::Own`], as the
    ///   roots of the list. With tops it reports every file the list
    ///   holds, a dependency's too, since a tool given the list cannot
    ///   build it either; of a dependency's file only what leaves it short
    ///   of a header, as `check` reports a reached dependency's.
    pub fn shown(&self, reached: Option<&[usize]>, whose: Whose) -> Vec<usize> {
        let takes = |&at: &usize| whose.takes(&self.units[at]);
        match reached {
            Some(reached) => reached.iter().copied().filter(takes).collect(),
            None => (0..self.units.len()).filter(takes).collect(),
        }
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
        self.unread.extend(paths.iter().cloned());
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

    fn discovered(again: Again, run: &Run) -> Result<Plan> {
        let (here, loaded) = settle::loaded(run, again.here.clone())?;
        let Loaded {
            mut dependencies,
            described,
            on_disk,
        } = loaded;
        let target::Here {
            package,
            workspace,
            in_workspace,
        } = here;
        let walk = Walk {
            enabled: dependencies.select(&package.package, &again.selection)?,
            held: None,
            patched: workspace.patches.0.values().cloned().collect(),
        };
        // As a filelist names them: the flags' after the tables', so that a
        // flag wins.
        let flagged = again.defines.clone();
        let defines = flagged.clone();
        let mut bare = Vec::new();
        let mut with_flags = |build: Build| {
            let build = (again.incdirs.iter().cloned()).fold(build, Build::include_dir);
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
        incdirs.extend(again.incdirs.iter().cloned());
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
        plan.again = Some(again);
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
            unread: Vec::new(),
            again: None,
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

/// The files `sources` names, or every file the walk of the package `target`
/// chose finds without any: what `fmt` reads, which needs no dependency and
/// no build.
///
/// A header the walk does not find, of another extension or outside the
/// sources, is left out rather than found by reading every file for the
/// `` `include``s that name it.
pub fn own_files(run: &Run, target: &Target, sources: &Sources) -> Result<Vec<PathBuf>> {
    if sources.named() {
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io;
    use std::path::{Path, PathBuf};

    use astli_package::mark;
    use astli_resolve::Store;
    use astli_testing::Fixture;
    use astli_text::{Disk, Reader};

    use super::Plan;
    use crate::analysis::Designs;
    use crate::render::Out;
    use crate::run::Run;

    /// Buffers in front of the disk, as an editor holds them.
    struct Buffers(HashMap<PathBuf, String>);

    impl Reader for Buffers {
        fn exists(&self, path: &Path) -> bool {
            self.0.contains_key(path) || Disk.exists(path)
        }

        fn read(&self, path: &Path) -> io::Result<String> {
            match self.0.get(path) {
                Some(text) => Ok(text.clone()),
                None => Disk.read(path),
            }
        }
    }

    /// The units `plan` compiles, by file name, and which of `names` its
    /// design defines.
    fn loaded(plan: Plan, run: &Run, names: &[&str]) -> (Vec<String>, Vec<String>) {
        let units = (plan.units.iter())
            .map(|unit| {
                unit.path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        let mut out = Out::new(io::stdout());
        let mut designs = Designs::of(&mut out, &plan, &[], false, &[], run).unwrap();
        let defined = designs
            .each(&mut out, plan, run, true, false, |_, _, analysis, _| {
                let design = analysis.elaboration.design();
                Ok((names.iter())
                    .filter(|name| design.definition(name).is_some())
                    .map(|name| name.to_string())
                    .collect::<Vec<_>>())
            })
            .unwrap();
        (units, defined.concat())
    }

    #[test]
    fn a_buffer_is_read_in_place_of_its_file_and_of_a_header_it_includes() {
        let fixture = Fixture::new("cli-load-buffers");
        fixture.file("astli.toml", "");
        let top = fixture.file("top.sv", "module top;\nendmodule\n");
        let extra = fixture.file("extra.sv", "module on_disk;\nendmodule\n");
        let names = ["top", "on_disk", "in_buffer"];

        let run = Run::loading(&Disk);
        let on_disk = loaded(Plan::load(&top, &run).unwrap(), &run, &names);
        let buffers = Buffers(HashMap::from([
            (
                top.clone(),
                "`include \"extra.sv\"\nmodule top;\nendmodule\n".into(),
            ),
            (extra, "module in_buffer;\nendmodule\n".into()),
        ]));
        let run = Run::loading(&buffers);
        let in_buffers = loaded(Plan::load(&top, &run).unwrap(), &run, &names);

        assert_eq!(
            on_disk,
            (
                vec!["extra.sv".into(), "top.sv".into()],
                vec!["top".into(), "on_disk".into()]
            )
        );
        // The buffer includes `extra.sv`, which makes it a header, and the
        // header is the buffer's too.
        assert_eq!(
            in_buffers,
            (
                vec!["top.sv".into()],
                vec!["top".into(), "in_buffer".into()]
            )
        );
        assert!(run.problems().is_empty());
    }

    #[test]
    fn a_dependency_that_cannot_be_read_is_a_problem_and_the_package_is_read_alone() {
        let fixture = Fixture::new("cli-load-unread");
        fixture.file(
            "astli.toml",
            "[dependencies]\ncells = { git = 'https://example.invalid/cells.git', version = '1' }\n",
        );
        let top = fixture.file("top.sv", "module top;\n  cells u_cells ();\nendmodule\n");
        let store = Store::at(fixture.dir("cache")).offline(true);

        let run = Run::loading(&Disk).storing(store);
        let plan = Plan::load(&top, &run).unwrap();
        let problems = run.problems();
        let design = loaded(plan, &run, &["top", "cells"]);

        let said: Vec<String> = problems.iter().map(|it| mark::plain(&it.message)).collect();
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(
            said[0].contains("`cells`") && said[0].contains("not in the cache"),
            "{said:?}"
        );
        assert_eq!(design, (vec!["top.sv".into()], vec!["top".into()]));
        assert!(!fixture.path().join("astli.lock").exists());
    }

    #[test]
    fn a_file_outside_any_package_is_read_alone() {
        let fixture = Fixture::new("cli-load-alone");
        let alone = fixture.file("alone.sv", "module on_disk;\nendmodule\n");
        let buffers = Buffers(HashMap::from([(
            alone.clone(),
            "module in_buffer;\nendmodule\n".into(),
        )]));

        let run = Run::loading(&buffers);
        let plan = Plan::load(&alone, &run).unwrap();

        assert_eq!(
            loaded(plan, &run, &["on_disk", "in_buffer"]),
            (vec!["alone.sv".into()], vec!["in_buffer".into()])
        );
    }
}
