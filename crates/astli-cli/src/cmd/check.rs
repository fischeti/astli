//! `astli check` subcommand.
//!
//! Lowers every file of the design, each expanded as it is compiled and in
//! parallel, then checks each against what all of them declare: its names,
//! the packages and definitions it uses, and each instance's connections
//! and overrides. What a file's expansion and parse report is printed before
//! what its check finds, and counts as an error too.
//!
//! With no file named, the design is the package the current directory is
//! in and its dependencies, and only the files of the package and of the
//! dependencies a `path` requirement reads from disk are reported: a
//! fetched dependency's are its authors' to fix, but one on disk is likely
//! the package's authors' too, and only a dependent elaborates it with the
//! values it is given. `-p` checks another package of the workspace, and
//! `--workspace` every one of them, telling a finding in a sibling's file
//! once though each member requiring the sibling finds it.
//!
//! A variable driven twice is a warning, compared at its definition's
//! defaults. With `--top`, it elaborates the design from each top first: an
//! instance in a generate block is checked as what the tops build makes it,
//! and a variable driven twice in what is built is an error. Only the files
//! the tops reach are reported, and a name declared more than once is told
//! for every one of them, a fetched dependency's too, since which
//! declaration the build means is a guess whoever's files hold them.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use astli_index::Summary;
use astli_sema::{Built, check};
use astli_text::Diagnostic;
use rayon::prelude::*;
use usage_rs::{Args, RunWith};

use crate::analysis::{self, Analysis};
use crate::cli::{BuildArgs, FeatureArgs, Members, RunArgs, Sources};
use crate::cmd::{self, Ctx, files};
use crate::discovery::Plan;
use crate::error::{Error, Result};
use crate::render::Out;

/// Verify design semantics, port connections, parameter overrides, and undeclared identifiers
#[derive(Args)]
pub struct Check {
    #[usage(flatten)]
    pub sources: Sources,
    #[usage(flatten)]
    pub members: Members,
    #[usage(flatten)]
    pub features: FeatureArgs,
    /// Elaborate hierarchy starting from the specified root module or [top.NAME] table (can be repeated)
    #[usage(long)]
    pub top: Vec<String>,
    /// Set a parameter of each top as NAME=VALUE, over what its [top.NAME] table sets (can be repeated)
    #[usage(short = 'G', long = "param")]
    pub param: Vec<String>,
    #[usage(flatten)]
    pub build: BuildArgs,
}

impl RunWith<Ctx<'_>> for Check {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        cmd::members(&self.members, self.sources.named(), out, |out| {
            let mut plan = Plan::new(
                &self.sources,
                &self.features,
                &self.build,
                Some(&self.top),
                run,
            )?;
            plan.tops.set(&self.param)?;
            let (analysis, checked) = checked(out, &mut plan, run)?;
            // Left to the process's exit, unless another package's run
            // follows and would add its design to this one's.
            match cmd::several() {
                true => drop(analysis),
                false => analysis.leak(),
            }
            checked
        })
    }
}

/// Whether `diagnostic` in the file at `path` is told for the first time
/// among the packages the command runs on: the run of each member
/// depending on a sibling also reports the sibling's files, which say the
/// same again unless what the member builds them with changes what they
/// hold.
fn first_told(path: &Path, diagnostic: &Diagnostic) -> bool {
    static TOLD: Mutex<Option<HashSet<(PathBuf, String)>>> = Mutex::new(None);
    let mut told = TOLD.lock().unwrap_or_else(|it| it.into_inner());
    let key = (path.to_path_buf(), format!("{diagnostic:?}"));
    told.get_or_insert_default().insert(key)
}

/// Checks the design `plan` holds, from its tops if it has any, reporting
/// what it finds. Hands back the analysis, whose index of the files and
/// tops a build of them lists, and whether the check passed.
pub(crate) fn checked(out: &mut Out, plan: &mut Plan, run: &RunArgs) -> Result<(Analysis, Result)> {
    let mut analysis = Analysis::new(out, plan, run.jobs, true)?;
    let tops = analysis.top_names();
    let elaboration = &analysis.elaboration;
    let index = analysis
        .index
        .as_ref()
        .expect("check summarizes every unit");
    let files = &analysis.files;
    // What only what the tops build can get wrong: a variable driven
    // twice, a type that does not convert, an `$error` the design runs
    // while elaborating, a parameter without a default left without a
    // value.
    let (built, of_built) = match tops.is_empty() {
        true => (None, Vec::new()),
        false => {
            let built = Built::new(elaboration, &analysis.bodies());
            let mut of_built = built.multiple_drivers(elaboration);
            of_built.extend(built.mistyped(elaboration));
            of_built.extend(built.elaboration_tasks(elaboration));
            of_built.extend(built.unset_parameters(elaboration));
            (Some(built), of_built)
        }
    };
    let shows = |file: usize| plan.units[file].reported || plan.units[file].on_disk;
    let reported: Vec<usize> = (0..plan.units.len()).filter(|&file| shows(file)).collect();
    // What is declared twice is told as `files` tells it, so that the
    // filelist it writes for the tops and the check of the same tops
    // agree. The rest is told for the files of the package and of its
    // dependencies on disk that the tops reach: a file no top builds is
    // another top's to check.
    let (told, shown) = match tops.is_empty() {
        true => (reported.clone(), reported),
        false => {
            let reached = (index.reachable(&tops)).map_err(|err| Error::failed(err.to_string()))?;
            let shown = (reached.iter().copied())
                .filter(|&file| shows(file))
                .collect();
            (reached, shown)
        }
    };
    let twice = files::redeclared(index, &told, &tops);
    if !tops.is_empty() {
        files::foreign_binds(plan, index, &told);
    }
    let mut found: Vec<Vec<Diagnostic>> = vec![Vec::new(); files.len()];
    let checked: Vec<_> = (shown.par_iter())
        .map(|&file| check(elaboration, files[file], built.as_ref()))
        .collect();
    for (&file, checked) in shown.iter().zip(checked) {
        found[file] = checked;
    }
    for (file, diagnostic) in of_built {
        found[file.index()].push(diagnostic);
    }

    // Of a fetched dependency's file the tops reach, what leaves it short
    // of a header is told, as `files` tells it: the list it writes would
    // not build.
    let parsed = &mut analysis.parsed;
    let short: Vec<usize> = (told.iter().copied())
        .filter(|&file| !shows(file) && parsed[file].iter().any(files::unresolved_include))
        .collect();
    for &file in &short {
        let of = plan.dependency_of(&plan.units[file]);
        parsed[file] = (std::mem::take(&mut parsed[file]).into_iter())
            .filter(files::unresolved_include)
            .map(|diagnostic| files::in_dependency(diagnostic, of))
            .collect();
        found[file].clear();
    }

    // A package's by path, so that a root's own files and those of the
    // members it depends on read as one tree; files named keep their
    // order. What leaves a fetched dependency short of a header follows,
    // its files being in the cache.
    let mut shown = shown;
    if plan.is_package() {
        shown.sort_by(|&a, &b| plan.units[a].path.cmp(&plan.units[b].path));
    }
    let told_files: Vec<_> = (shown.iter().chain(&short))
        .map(|&file| {
            let mut checked = std::mem::take(&mut found[file]);
            checked.sort_by_key(|diagnostic| (diagnostic.at.src_id, diagnostic.at.start));
            // What the expansion and the parser said comes first, as they
            // found it.
            let mut all = std::mem::take(&mut parsed[file]);
            all.extend(checked);
            // An error an earlier package's run told still fails this one,
            // so that each package that has it is counted.
            let mut told_before = 0;
            if cmd::several() {
                all.retain(|diagnostic| {
                    let first = first_told(&plan.units[file].path, diagnostic);
                    told_before += usize::from(!first && diagnostic.is_error());
                    first
                });
            }
            (Some(&analysis.origins[file]), Vec::new(), told_before, all)
        })
        .collect();
    let reported = analysis::report(out, told_files)?.finish();
    // The list `files` writes for the run, checked as `files` checks it.
    let index = analysis
        .index
        .as_ref()
        .expect("check summarizes every unit");
    if plan.is_package() {
        let listed: Vec<&Summary> = match tops.is_empty() {
            true => index.summaries().iter().collect(),
            false => told.iter().map(|&file| &index.summaries()[file]).collect(),
        };
        let incdirs = match tops.is_empty() {
            true => plan.incdirs.clone(),
            false => files::used(&plan.incdirs, listed.iter().copied()),
        };
        files::misfound(&incdirs, listed.into_iter());
    }
    let passed = twice.and(reported);
    Ok((analysis, passed))
}
