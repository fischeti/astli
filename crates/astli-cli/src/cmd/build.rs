//! `astli build` subcommand.
//!
//! Checks the package's design from one top, as `check --top` does, and
//! stops at an error. Then the simulator builds the top into the package's
//! `.astli/build`, or its workspace root's, from the files it needs in the
//! order `files` writes them, under the profile the run chooses. A build
//! made from what the last one was, by its fingerprint, is not made again.
//!
//! A caller building several tops checks once for each set of defines they
//! give, and then makes each top's list, fingerprint and build of its own.

use std::collections::HashSet;
use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Instant;

use astli_index::{Index, Summary};

use usage_rs::{Args, RunWith};

use astli_package::{LOCAL, Package, Profile, mark};

use crate::analysis::{Analysis, Designs, Elaborated};
use crate::cli::{FeatureArgs, PackageArg, SimArgs, Sources};
use crate::cmd::check::Found;
use crate::cmd::{self, Ctx, check};
use crate::discovery::Plan;
use crate::error::{Error, Result};
use crate::json;
use crate::listing::Listing;
use crate::render::Out;
use crate::run::Run;
use crate::sim::Backend;
use crate::term::{self, status};
use crate::tops::{Chosen, Flags, point_of};

/// Build a top of the package with a simulator
#[derive(Args)]
pub struct Build {
    #[usage(flatten)]
    pub package: PackageArg,
    #[usage(flatten)]
    pub features: FeatureArgs,
    /// Build the specified top module or [top.NAME] table
    #[usage(long)]
    pub top: Option<String>,
    #[usage(flatten)]
    pub sim: SimArgs,
    /// Set a parameter of each top as NAME=VALUE, over what its [top.NAME] table sets (can be repeated)
    #[usage(short = 'G', long = "param")]
    pub param: Vec<String>,
}

impl RunWith<Ctx<'_>> for Build {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        self.built(ctx).map(drop)
    }
}

impl Build {
    /// Builds the top, unless it is fresh, and hands back where its
    /// executable is, and what the profile gives every run of it.
    pub(crate) fn built(self, ctx: Ctx<'_>) -> Result<(PathBuf, Vec<OsString>)> {
        let Ctx { out, run } = ctx;
        let target = cmd::choose(self.package.package.as_deref(), false)?;
        let asked: Vec<String> = self.top.into_iter().collect();
        let build = self.sim.build_args();
        let plan = Plan::new(&target, &Sources::default(), &self.features, &build, run)?;
        let tops = Chosen::of(&plan, &asked)?;
        let flags = Flags::new(&self.param, std::slice::from_ref(&tops), true)?;
        if tops.tops.len() != 1 {
            return Err(term::error("no top to build")
                .help(format!(
                    "name one with {}, or add a {} table to {}",
                    mark::code("--top"),
                    mark::code("[top.NAME]"),
                    mark::path("astli.toml")
                ))
                .into());
        }
        // Found before the check, so that a missing simulator fails at once.
        let builder = Builder::new(&plan, &self.sim, &mut None)?;
        let mut designs = Designs {
            chosen: vec![tops],
            flags,
        };
        let mut built = built_tops(out, plan, &mut designs, &builder, run)?;
        let executable = built.pop().expect("one top built");
        Ok((executable, builder.run_args().collect()))
    }
}

/// Builds each top of the `designs` of `plan`, unless it is fresh, and
/// hands back where each executable is, in order. Every design is checked
/// first, as [`listed_tops`] checks them, and if any check fails, nothing
/// is built.
pub(crate) fn built_tops(
    out: &mut Out,
    plan: Plan,
    designs: &mut Designs,
    builder: &Builder,
    run: &Run,
) -> Result<Vec<PathBuf>> {
    let checked =
        |_: &Plan, analysis: &mut Analysis, flags: &mut Flags| flags.check(analysis).map(|()| true);
    (listed_tops(out, plan, designs, run, checked)?.iter())
        .map(|listed| builder.build(listed, run.args.jobs))
        .collect()
}

/// What a build of each top of the `designs` of `plan` reads, in order,
/// once every design is checked, a finding several of them meet told once.
/// Fails if any check does. Each design is given to `chosen`, with the plan
/// that defines it and the flags, to keep the tops to check and build, and
/// refuse the parameters they cannot take, before its check: one it leaves
/// no top, or says `false` of, is not checked.
pub(crate) fn listed_tops(
    out: &mut Out,
    plan: Plan,
    designs: &mut Designs,
    run: &Run,
    mut chosen: impl FnMut(&Plan, &mut Analysis, &mut Flags) -> Result<bool>,
) -> Result<Vec<Listed>> {
    let mut found = Found::default();
    let checked = designs.each(out, plan, run, true, false, |_, plan, analysis, flags| {
        if !chosen(&plan, analysis, flags)? || analysis.tops.is_empty() {
            return Ok(None);
        }
        found.add(check::found(run, &plan, analysis)?);
        // What lists the tops, kept past the analysis.
        let index = analysis.index.take().expect("check summarizes every unit");
        Ok(Some((plan, index, std::mem::take(&mut analysis.tops))))
    })?;
    found.report(out)?.finish()?;
    let mut lists = Vec::new();
    for (plan, index, tops) in checked.into_iter().flatten() {
        // A list is what its top's module reaches under the design's
        // defines, so the points of a test read one list, warned of once.
        let mut warned = HashSet::new();
        for elaborated in tops {
            let warn = warned.insert(elaborated.top.module.clone());
            lists.push(Listed::of(run, &plan, &index, elaborated, warn)?);
        }
    }
    Ok(lists)
}

/// What builds a package's tops: the simulator, under the profile the run
/// chooses, into the package's directory of builds.
pub(crate) struct Builder {
    pub(crate) backend: Backend,
    profile_name: &'static str,
    profile: Profile,
    /// Where the workspace's builds are, or the package's outside one, and
    /// the directory the simulator runs in, whose manifest set the profile.
    root: PathBuf,
    /// The files the profile's `args` name, whose bytes the build is made
    /// from as much as the design's.
    named: Vec<PathBuf>,
    /// What names the package's builds among the workspace's.
    package_dir: String,
    /// The package's name, shown before each top's where a run builds the
    /// tops of several packages, which may each hold one of a name.
    pub(crate) shown_package: Option<String>,
}

impl Builder {
    /// The builder of the package `plan` discovered, with the profile,
    /// tracing and simulator `sim` asks for, which it finds unless `found`
    /// holds it from another package's builder, and leaves there.
    pub(crate) fn new(plan: &Plan, sim: &SimArgs, found: &mut Option<Backend>) -> Result<Builder> {
        let (Some((package, _)), Some(shared)) = (plan.packages(), plan.shared()) else {
            return Err(Error::failed("a build is of a package's top"));
        };
        let (profile_name, mut profile) = match sim.release {
            true => ("release", shared.tables.profiles.release.clone()),
            false => ("dev", shared.tables.profiles.dev.clone()),
        };
        profile.trace |= sim.trace;
        let root = shared.package.root.clone();
        let simulator = sim.sim.unwrap_or(shared.tables.simulator);
        let named = named(profile.args_for(simulator), &root);
        let backend = match found.take() {
            Some(backend) if backend.simulator() == simulator => backend,
            _ => Backend::find(simulator)?,
        };
        *found = Some(backend.clone());
        Ok(Builder {
            backend,
            profile_name,
            profile,
            root,
            named,
            package_dir: dir_name(&package.package),
            shown_package: None,
        })
    }

    /// What the profile gives every run of a top this builds, before the
    /// run's own arguments.
    pub(crate) fn run_args(&self) -> impl Iterator<Item = OsString> + '_ {
        (self.profile.run_args_for(self.backend.simulator()).iter()).map(OsString::from)
    }

    /// Builds the top `listed` lists, unless it is fresh, on `jobs`
    /// threads, and hands back where its executable is.
    pub(crate) fn build(&self, listed: &Listed, jobs: usize) -> Result<PathBuf> {
        let Builder {
            backend,
            profile_name,
            ..
        } = self;
        let top = &listed.elaborated.top;
        astli_package::local(&self.root)?;
        let dir = builds_dir(&self.root)
            .join(backend.simulator().name())
            .join(profile_name)
            .join(&self.package_dir)
            .join(&listed.dir);
        std::fs::create_dir_all(&dir).map_err(|err| Error::io(&dir, err))?;
        let args = backend.args(listed, &dir, &self.profile);
        let fingerprint = fingerprint(backend, &args, &self.inputs(listed), listed)?;
        let stamp = dir.join("fingerprint");
        let built = std::fs::read_to_string(&stamp).is_ok_and(|it| it == fingerprint);
        let executable = backend.executable(&dir, &top.module);
        let shown = match &self.shown_package {
            Some(package) => mark::name(format!("{package} {}", top.name)),
            None => mark::name(&top.name),
        };
        // A build with nothing to do says nothing, as it changes nothing.
        if built && executable.is_file() {
            return Ok(executable);
        }
        // Gone until the build succeeds, so that one stopped halfway is
        // never taken for current.
        match std::fs::remove_file(&stamp) {
            Err(err) if err.kind() != ErrorKind::NotFound => return Err(Error::io(&stamp, err)),
            _ => {}
        }
        let how = format!(
            "with {} {}, {profile_name} profile",
            backend.name(),
            mark::version(backend.version())
        );
        let spinner = term::spin("Building", &format!("{shown} {how}"));
        // A log says what it waits on before the build, which may be long.
        if !spinner.drawn() {
            status!("Building", "{shown} {how}");
        }
        let started = Instant::now();
        backend.build(listed, &dir, &args, &self.root, jobs)?;
        drop(spinner);
        std::fs::write(&stamp, &fingerprint).map_err(|err| Error::io(&stamp, err))?;
        status!(
            "Built",
            "{shown} in {:.1}s {how}",
            started.elapsed().as_secs_f64()
        );
        Ok(executable)
    }

    /// Every file a build of the top `listed` lists reads, whose bytes it
    /// is made from as much as from its arguments: the design's files and
    /// the headers they include, then the files the profile's `args` name.
    fn inputs<'a>(&'a self, listed: &'a Listed) -> Vec<&'a Path> {
        (listed.read.iter().chain(&self.named))
            .map(PathBuf::as_path)
            .collect()
    }
}

/// Where the builds of the package rooted at `root` are, every
/// simulator's and every profile's, and in a workspace every member's.
pub(crate) fn builds_dir(root: &Path) -> PathBuf {
    root.join(LOCAL).join("build")
}

/// What a build of one top reads: the file list, and what is in it.
pub(crate) struct Listed {
    pub(crate) elaborated: Elaborated,
    /// Where it builds in the package's directory of builds: in one named
    /// for it, or a point in one named for its values' hash in its test's,
    /// since `[`, `=` and `,` are fragile in a simulator's makefile.
    dir: PathBuf,
    /// The file list: include directories, defines and files, in the order
    /// `files` writes them.
    pub(crate) text: Vec<u8>,
    /// Every file in the list, then every header they include, by path.
    read: Vec<PathBuf>,
}

impl Listed {
    /// What a build of `elaborated`, a top of `plan`, reads, given what
    /// `index` says of each unit, warning of what the list holds if `warn`.
    pub(crate) fn of(
        run: &Run,
        plan: &Plan,
        index: &Index,
        elaborated: Elaborated,
        warn: bool,
    ) -> Result<Listed> {
        let top = &elaborated.top;
        let kept = (index.reachable(std::slice::from_ref(&top.module)))
            .map_err(|err| Error::failed(err.to_string()))?;
        // The list `files` writes for the top, without a note on every
        // build of each stub it leaves out.
        let listing = Listing::of(plan, index, Some(kept), true);
        if warn {
            listing.misfound(run, plan, index);
        }
        let mut text = Vec::new();
        listing.write(&mut text, index)?;
        let summaries: Vec<&Summary> = listing.summaries(index).collect();
        let mut read: Vec<PathBuf> = summaries.iter().map(|it| it.path.clone()).collect();
        let mut headers: Vec<&Path> = (summaries.iter())
            .flat_map(|it| &it.includes)
            .map(|include| include.path.as_path())
            .collect();
        headers.sort();
        headers.dedup();
        read.extend(headers.into_iter().map(Path::to_path_buf));
        let dir = match point_of(&top.name) {
            Some(test) => {
                let values = &top.name[test.len()..];
                Path::new(test).join(json::hash([values.as_bytes()]))
            }
            None => PathBuf::from(&top.name),
        };
        Ok(Listed {
            elaborated,
            dir,
            text,
            read,
        })
    }
}

/// What names the package's builds among a workspace's: its name, or
/// without one its directory's, since two members may each have a
/// `tb_top`.
fn dir_name(package: &Package) -> String {
    match (&package.manifest.name, package.root.file_name()) {
        (Some(name), _) => name.clone(),
        (None, Some(dir)) => dir.to_string_lossy().into_owned(),
        (None, None) => "root".to_string(),
    }
}

/// The files `args` name relative to `root`, where the simulator runs. Only
/// what exists is taken for a path, since an arg may be a flag's value; a
/// path inside a flag, such as `-CFLAGS -Isrc`, is not found.
fn named(args: &[String], root: &Path) -> Vec<PathBuf> {
    (args.iter())
        .map(|arg| root.join(arg))
        .filter(|path| path.is_file())
        .collect()
}

/// What a build is made from: astli's version and the simulator's, the
/// arguments the simulator is given and the file list it reads, and the
/// path and bytes of every file in `inputs`. A build with the fingerprint
/// of the last one has nothing to do.
fn fingerprint(
    backend: &Backend,
    args: &[OsString],
    inputs: &[&Path],
    listed: &Listed,
) -> Result<String> {
    let contents = (inputs.iter())
        .map(|path| std::fs::read(path).map_err(|err| Error::io(path, err)))
        .collect::<Result<Vec<_>>>()?;
    let mut parts: Vec<&[u8]> = vec![
        env!("CARGO_PKG_VERSION").as_bytes(),
        backend.version().as_bytes(),
        &listed.text,
    ];
    parts.extend(args.iter().map(|arg| arg.as_encoded_bytes()));
    for (path, content) in inputs.iter().zip(&contents) {
        parts.push(path.as_os_str().as_encoded_bytes());
        parts.push(content);
    }
    Ok(json::hash(parts))
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::cli::RunArgs;
    use crate::target::Target;

    // No command builds several tops yet, so the function is run here.
    #[test]
    fn verilator_builds_several_tops_after_a_check_for_each_set_of_defines() {
        let dir = std::env::temp_dir().join(format!("astli-build-tops-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = |name: &str, text: &str| {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        // Two tables that define the same, and one that defines `W` another
        // way, each setting the bench's parameter.
        file(
            "astli.toml",
            "[package]\nname = 'tops'\n\n\
             [top.narrow]\ntop = 'tb'\ndefines = { W = 4 }\nparams = { V = 3 }\n\n\
             [top.narrow_max]\ntop = 'tb'\ndefines = { W = 4 }\nparams = { V = 15 }\n\n\
             [top.wide]\ntop = 'tb'\ndefines = { W = 8 }\nparams = { V = 255 }\n",
        );
        file(
            "tb/tb.sv",
            "module tb #(parameter int V = 0);\n\
             \x20 logic [`W-1:0] x;\n\
             \x20 initial begin\n\
             \x20   x = V;\n\
             \x20   $display(\"%0d in %0d bits\", x, `W);\n\
             \x20   $finish;\n\
             \x20 end\n\
             endmodule\n",
        );
        let target = Target::new(dir.clone());
        let run = Run::new(RunArgs {
            jobs: 2,
            ..RunArgs::default()
        });
        let features = FeatureArgs {
            all_features: true,
            ..FeatureArgs::default()
        };
        let build = crate::cli::BuildArgs::default();
        let plan = Plan::new(&target, &Sources::default(), &features, &build, &run).unwrap();
        let chosen = Chosen::designs(&plan, &[], &[]).unwrap();
        assert_eq!(chosen.len(), 2, "one design for each set of defines");
        let builder = Builder::new(&plan, &SimArgs::default(), &mut None).unwrap();
        let mut out = Out::new(std::io::stdout());

        let flags = Flags::new(&[], &[], false).unwrap();
        let mut designs = Designs { chosen, flags };
        let built = built_tops(&mut out, plan, &mut designs, &builder, &run).unwrap();

        let said: Vec<String> = (built.iter())
            .map(|executable| {
                let ran = Command::new(executable).output().unwrap();
                let said = String::from_utf8_lossy(&ran.stdout);
                said.lines().next().unwrap_or_default().to_string()
            })
            .collect();
        assert_eq!(said, ["3 in 4 bits", "15 in 4 bits", "255 in 8 bits"]);
        let builds = dir.join(".astli/build/verilator/dev/tops");
        for (top, executable) in ["narrow", "narrow_max", "wide"].iter().zip(&built) {
            assert_eq!(executable, &builds.join(top).join("Vtb"));
        }
    }
}
