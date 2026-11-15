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

use std::ffi::OsString;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use astli_index::Summary;

use usage_rs::{Args, RunWith, ValueEnum};

use astli_package::{Package, Profile, Simulator, mark};

use crate::analysis::Analysis;
use crate::cli::{BuildArgs, FeatureArgs, PackageArg, RunArgs, Sources};
use crate::cmd::check::Found;
use crate::cmd::{self, Ctx, check, files};
use crate::discovery::Plan;
use crate::error::{Error, Result};
use crate::filelist::quoted;
use crate::json;
use crate::render::Out;
use crate::term::{self, status};
use crate::verilator::{Verilator, after_last_build, executable};

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
    /// Build with the release profile, which compiles longer and simulates faster
    #[usage(long)]
    pub release: bool,
    /// Build so that the simulation can write waveforms, whatever the profile sets
    #[usage(long)]
    pub trace: bool,
    /// Simulator to build with, in place of the manifest's [build] simulator
    #[usage(long, value_enum)]
    pub sim: Option<Sim>,
    // Without `+incdir+` and `+define+`, a filelist's spelling, which would
    // take a simulation's plusarg after `astli run --` for its own.
    /// Add directory to the `include` search path (can be repeated)
    #[usage(short = 'I', long = "incdir")]
    pub incdir: Vec<PathBuf>,
    /// Define preprocessor macro as NAME or NAME=VALUE (can be repeated)
    #[usage(short = 'D', long = "define")]
    pub define: Vec<String>,
    /// Set a parameter of each top as NAME=VALUE, over what its [top.NAME] table sets (can be repeated)
    #[usage(short = 'G', long = "param")]
    pub param: Vec<String>,
}

/// The simulators a build can drive.
#[derive(ValueEnum, Clone, Copy, PartialEq, Eq)]
pub enum Sim {
    /// Verilator 5 or later
    Verilator,
}

impl RunWith<Ctx<'_>> for Build {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        self.built(ctx).map(drop)
    }
}

impl Build {
    /// Builds the top, unless it is fresh, and hands back where its
    /// executable is.
    pub(crate) fn built(self, ctx: Ctx<'_>) -> Result<PathBuf> {
        let Ctx { out, run } = ctx;
        cmd::choose(self.package.package.as_deref(), false)?;
        let asked: Vec<String> = self.top.into_iter().collect();
        let build = BuildArgs {
            incdir: self.incdir,
            define: self.define,
            ..BuildArgs::default()
        };
        let mut plan = Plan::new(
            &Sources::default(),
            &self.features,
            &build,
            Some(&asked),
            run,
        )?;
        plan.tops.set(&self.param)?;
        if plan.tops.names.len() != 1 {
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
        let builder = Builder::new(&plan, self.release, self.trace, self.sim)?;
        let mut built = built_tops(out, vec![plan], &builder, run)?;
        Ok(built.pop().expect("one top built"))
    }
}

/// Builds each top of `designs`, each the plan of one set of defines its
/// tops give, unless it is fresh, and hands back where each executable is,
/// in order. Every design is checked first, as [`listed_tops`] checks
/// them, and if any check fails, nothing is built.
pub(crate) fn built_tops(
    out: &mut Out,
    designs: Vec<Plan>,
    builder: &Builder,
    run: &RunArgs,
) -> Result<Vec<PathBuf>> {
    (listed_tops(out, designs, run)?.iter())
        .map(|listed| builder.build(listed, run.jobs))
        .collect()
}

/// What a build of each top of `designs` reads, in order, once every
/// design is checked, a finding several of them meet told once. Fails if
/// any check does.
pub(crate) fn listed_tops(out: &mut Out, designs: Vec<Plan>, run: &RunArgs) -> Result<Vec<Listed>> {
    if designs.len() > 1 {
        cmd::several_designs();
    }
    let mut found = Found::default();
    let mut checked = Vec::with_capacity(designs.len());
    for mut plan in designs {
        let (analysis, mut of) = check::found(out, &mut plan, run)?;
        let told = std::mem::take(&mut of.told);
        found.add(of);
        checked.push((plan, analysis, told));
    }
    found.report(out)??;
    let mut lists = Vec::new();
    for (plan, analysis, told) in checked {
        check::listed(&plan, &analysis, &told);
        for at in 0..analysis.tops.len() {
            lists.push(Listed::of(&plan, &analysis, at)?);
        }
    }
    Ok(lists)
}

/// What builds a package's tops: the simulator, under the profile the run
/// chooses, into the package's directory of builds.
pub(crate) struct Builder {
    verilator: Verilator,
    simulator: Simulator,
    profile_name: &'static str,
    profile: Profile,
    /// Where the workspace's builds are, or the package's outside one.
    root: PathBuf,
    /// What names the package's builds among the workspace's.
    package_dir: String,
}

impl Builder {
    /// The builder of the package `plan` discovered, with the release
    /// profile if `release`, tracing if `trace`, and with `sim` in place of
    /// the manifest's simulator, which it finds.
    pub(crate) fn new(
        plan: &Plan,
        release: bool,
        trace: bool,
        sim: Option<Sim>,
    ) -> Result<Builder> {
        let (Some((package, _)), Some(shared)) = (plan.packages(), plan.shared()) else {
            return Err(Error::failed("a build is of a package's top"));
        };
        let (profile_name, mut profile) = match release {
            true => ("release", shared.profiles.release.clone()),
            false => ("dev", shared.profiles.dev.clone()),
        };
        profile.trace |= trace;
        let simulator = match sim {
            Some(Sim::Verilator) => Simulator::Verilator,
            None => shared.simulator,
        };
        let verilator = match simulator {
            Simulator::Verilator => Verilator::find()?,
        };
        Ok(Builder {
            verilator,
            simulator,
            profile_name,
            profile,
            root: shared.root.clone(),
            package_dir: dir_name(package),
        })
    }

    /// Builds the top `listed` lists, unless it is fresh, on `jobs`
    /// threads, and hands back where its executable is.
    pub(crate) fn build(&self, listed: &Listed, jobs: usize) -> Result<PathBuf> {
        let Builder {
            verilator,
            profile_name,
            ..
        } = self;
        let name = &listed.name;
        let local = astli_package::local(&self.root)?;
        let dir = (local.join("build"))
            .join(self.simulator.name())
            .join(profile_name)
            .join(&self.package_dir)
            .join(name);
        std::fs::create_dir_all(&dir).map_err(|err| Error::io(&dir, err))?;
        let list = dir.join("files.f");
        let args = verilator.args(&list, &dir, &listed.module, &listed.params, &self.profile);
        let fingerprint = fingerprint(verilator, &args, listed)?;
        let stamp = dir.join("fingerprint");
        let built = std::fs::read_to_string(&stamp).is_ok_and(|it| it == fingerprint);
        let executable = executable(&dir, &listed.module);
        if built && executable.is_file() {
            status!("Fresh", "{}, {profile_name} profile", mark::name(name));
            return Ok(executable);
        }
        // Gone until the build succeeds, so that one stopped halfway is
        // never taken for current.
        match std::fs::remove_file(&stamp) {
            Err(err) if err.kind() != ErrorKind::NotFound => return Err(Error::io(&stamp, err)),
            _ => {}
        }
        // Before anything of this build is written, which would be newest.
        after_last_build(&dir);
        std::fs::write(&list, &listed.text).map_err(|err| Error::io(&list, err))?;

        status!(
            "Building",
            "{} with Verilator {}, {profile_name} profile",
            mark::name(name),
            mark::version(&verilator.version)
        );
        let started = Instant::now();
        verilator.build(&args, name, jobs)?;
        std::fs::write(&stamp, &fingerprint).map_err(|err| Error::io(&stamp, err))?;
        status!(
            "Finished",
            "{} in {:.1}s",
            mark::name(name),
            started.elapsed().as_secs_f64()
        );
        Ok(executable)
    }
}

/// What a build of one top reads: the file list, and what is in it.
pub(crate) struct Listed {
    /// What the top is called.
    pub(crate) name: String,
    /// What is elaborated from.
    module: String,
    /// What its parameters are set to, by name.
    params: Vec<(String, String)>,
    /// The file list: include directories, defines and files, in the order
    /// `files` writes them.
    text: Vec<u8>,
    /// Every file in the list, then every header they include, by path.
    read: Vec<PathBuf>,
}

impl Listed {
    /// What a build of the top at `at` among the tops of `plan` reads, from
    /// the analysis that checked it.
    pub(crate) fn of(plan: &Plan, analysis: &Analysis, at: usize) -> Result<Listed> {
        let index = analysis
            .index
            .as_ref()
            .expect("check summarizes every unit");
        let module = analysis.tops[at].0.clone();
        let kept = (index.reachable(std::slice::from_ref(&module)))
            .map_err(|err| Error::failed(err.to_string()))?;
        // Left out as `files` leaves it out, without a note on every build.
        let mut kept = files::without_stubs(index, kept, false);
        kept.sort_by_key(|&file| plan.group(file));
        let kept = index.ordered(&kept);
        let summaries: Vec<&Summary> = kept.iter().map(|&file| &index.summaries()[file]).collect();
        let incdirs = files::used(&plan.incdirs, summaries.iter().copied());
        if plan.is_package() {
            files::misfound(&incdirs, summaries.iter().copied());
        }
        let mut text = Vec::new();
        for dir in &incdirs {
            writeln!(text, "{}", quoted(&format!("+incdir+{}", dir.display())))?;
        }
        for define in &plan.defines {
            writeln!(text, "{}", quoted(&format!("+define+{define}")))?;
        }
        for summary in &summaries {
            writeln!(text, "{}", quoted(&summary.path.display().to_string()))?;
        }
        let mut read: Vec<PathBuf> = summaries.iter().map(|it| it.path.clone()).collect();
        let mut headers: Vec<&Path> = (summaries.iter())
            .flat_map(|it| &it.includes)
            .map(|include| include.path.as_path())
            .collect();
        headers.sort();
        headers.dedup();
        read.extend(headers.into_iter().map(Path::to_path_buf));
        Ok(Listed {
            name: plan.tops.names[at].clone(),
            module,
            params: plan.tops.params[at].clone(),
            text,
            read,
        })
    }
}

/// What names the package's builds among a workspace's: its name, or
/// without one its directory's, since two members may each have a
/// `tb_top`.
fn dir_name(package: &Package) -> String {
    match (&package.name, package.root.file_name()) {
        (Some(name), _) => name.clone(),
        (None, Some(dir)) => dir.to_string_lossy().into_owned(),
        (None, None) => "root".to_string(),
    }
}

/// What a build is made from: astli's version and Verilator's, the
/// arguments Verilator is given and the file list it reads, and the bytes
/// of every file and header in it. A build with the fingerprint of the
/// last one has nothing to do.
fn fingerprint(verilator: &Verilator, args: &[OsString], listed: &Listed) -> Result<String> {
    let contents = (listed.read.iter())
        .map(|path| std::fs::read(path).map_err(|err| Error::io(path, err)))
        .collect::<Result<Vec<_>>>()?;
    let mut parts: Vec<&[u8]> = vec![
        env!("CARGO_PKG_VERSION").as_bytes(),
        verilator.version.as_bytes(),
        &listed.text,
    ];
    parts.extend(args.iter().map(|arg| arg.as_encoded_bytes()));
    for (path, content) in listed.read.iter().zip(&contents) {
        parts.push(path.as_os_str().as_encoded_bytes());
        parts.push(content);
    }
    Ok(json::hash(parts))
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::lockfile;

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
        lockfile::choose(&dir);
        let run = RunArgs {
            jobs: 2,
            ..RunArgs::default()
        };
        let features = FeatureArgs {
            all_features: true,
            ..FeatureArgs::default()
        };
        let build = BuildArgs::default();
        let plan = Plan::new(&Sources::default(), &features, &build, None, &run).unwrap();
        let designs = plan.designs();
        assert_eq!(designs.len(), 2, "one design for each set of defines");
        let builder = Builder::new(&plan, false, false, None).unwrap();
        let designs = designs.into_iter().map(|tops| plan.for_tops(tops));
        let mut out = Out::new(std::io::stdout());

        let built = built_tops(&mut out, designs.collect(), &builder, &run).unwrap();

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
