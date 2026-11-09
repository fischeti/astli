//! `astli build` subcommand.
//!
//! Checks the package's design from one top, as `check --top` does, and
//! stops at an error. Then the simulator builds the top into the package's
//! `.astli/build`, from the files it needs in the order `files` writes
//! them, under the profile the run chooses. A build made from what the last
//! one was, by its fingerprint, is not made again.

use std::ffi::OsString;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use astli_index::Summary;

use usage_rs::{Args, RunWith, ValueEnum};

use astli_package::{Simulator, mark};

use crate::cli::{BuildArgs, FeatureArgs, Sources};
use crate::cmd::{Ctx, check, files};
use crate::discovery::Plan;
use crate::error::{Error, Result};
use crate::json;
use crate::term::{self, status};
use crate::verilator::{Verilator, executable};

/// Build a top of the package with a simulator
#[derive(Args)]
pub struct Build {
    #[usage(flatten)]
    pub features: FeatureArgs,
    /// Build the specified top module or [top.NAME] table
    #[usage(long)]
    pub top: Option<String>,
    /// Build with the release profile, which compiles longer and simulates faster
    #[usage(long)]
    pub release: bool,
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
        let Some((package, _)) = plan.packages() else {
            return Err(Error::failed("a build is of a package's top"));
        };
        let (profile_name, profile) = match self.release {
            true => ("release", package.profiles.release.clone()),
            false => ("dev", package.profiles.dev.clone()),
        };
        let simulator = match self.sim {
            Some(Sim::Verilator) => Simulator::Verilator,
            None => package.simulator,
        };
        let root = package.root.clone();
        let [name] = plan.tops.names.as_slice() else {
            return Err(term::error("no top to build")
                .help(format!(
                    "name one with {}, or add a {} table to {}",
                    mark::code("--top"),
                    mark::code("[top.NAME]"),
                    mark::path("astli.toml")
                ))
                .into());
        };
        let name = name.clone();
        // Found before the check, so that a missing simulator fails at once.
        let verilator = match simulator {
            Simulator::Verilator => Verilator::find()?,
        };

        let (analysis, checked) = check::checked(out, &mut plan, run)?;
        checked?;
        let tops = analysis.top_names();
        let index = analysis
            .index
            .as_ref()
            .expect("check summarizes every unit");
        let mut kept = (index.reachable(&tops)).map_err(|err| Error::failed(err.to_string()))?;
        kept.sort_by_key(|&file| plan.group(file));
        let kept = index.ordered(&kept);
        let summaries = kept.iter().map(|&file| &index.summaries()[file]);
        let incdirs = files::used(&plan.incdirs, summaries.clone());
        if plan.is_package() {
            files::misfound(&incdirs, summaries.clone());
        }

        let local = astli_package::local(&root)?;
        let dir = (local.join("build"))
            .join(simulator.name())
            .join(profile_name)
            .join(&name);
        std::fs::create_dir_all(&dir).map_err(|err| Error::io(&dir, err))?;
        let list = dir.join("files.f");
        let summaries: Vec<&Summary> = summaries.collect();
        let mut text = Vec::new();
        for dir in &incdirs {
            writeln!(text, "+incdir+{}", dir.display())?;
        }
        for define in &plan.defines {
            writeln!(text, "+define+{define}")?;
        }
        for summary in &summaries {
            writeln!(text, "{}", summary.path.display())?;
        }
        let args = verilator.args(&list, &dir, &tops[0], &profile);
        let fingerprint = fingerprint(&verilator, &args, &text, &summaries)?;
        let stamp = dir.join("fingerprint");
        let built = std::fs::read_to_string(&stamp).is_ok_and(|it| it == fingerprint);
        let executable = executable(&dir, &tops[0]);
        if built && executable.is_file() {
            status!("Fresh", "{}, {profile_name} profile", mark::name(&name));
            return Ok(executable);
        }
        // Gone until the build succeeds, so that one stopped halfway is
        // never taken for current.
        match std::fs::remove_file(&stamp) {
            Err(err) if err.kind() != ErrorKind::NotFound => return Err(Error::io(&stamp, err)),
            _ => {}
        }
        std::fs::write(&list, text).map_err(|err| Error::io(&list, err))?;

        status!(
            "Building",
            "{} with Verilator {}, {profile_name} profile",
            mark::name(&name),
            mark::version(&verilator.version)
        );
        let started = Instant::now();
        verilator.build(&args, &name, run.jobs)?;
        std::fs::write(&stamp, &fingerprint).map_err(|err| Error::io(&stamp, err))?;
        status!(
            "Finished",
            "{} in {:.1}s",
            mark::name(&name),
            started.elapsed().as_secs_f64()
        );
        Ok(executable)
    }
}

/// What a build is made from: astli's version and Verilator's, the
/// arguments Verilator is given and the file list it reads, and the bytes
/// of every file and header in it. A build with the fingerprint of the
/// last one has nothing to do.
fn fingerprint(
    verilator: &Verilator,
    args: &[OsString],
    list: &[u8],
    summaries: &[&Summary],
) -> Result<String> {
    let mut paths: Vec<&Path> = summaries.iter().map(|it| it.path.as_path()).collect();
    let mut headers: Vec<&Path> = (summaries.iter())
        .flat_map(|it| &it.includes)
        .map(|include| include.path.as_path())
        .collect();
    headers.sort();
    headers.dedup();
    paths.extend(headers);
    let contents = (paths.iter())
        .map(|path| std::fs::read(path).map_err(|err| Error::io(path, err)))
        .collect::<Result<Vec<_>>>()?;
    let mut parts: Vec<&[u8]> = vec![
        env!("CARGO_PKG_VERSION").as_bytes(),
        verilator.version.as_bytes(),
        list,
    ];
    parts.extend(args.iter().map(|arg| arg.as_encoded_bytes()));
    for (path, content) in paths.iter().zip(&contents) {
        parts.push(path.as_os_str().as_encoded_bytes());
        parts.push(content);
    }
    Ok(json::hash(parts))
}
