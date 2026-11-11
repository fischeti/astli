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
//! values it is given.
//!
//! A variable driven twice is a warning, compared at its definition's
//! defaults. With `--top`, it elaborates the design from each top first: an
//! instance in a generate block is checked as what the tops build makes it,
//! and a variable driven twice in what is built is an error. Only the files
//! the tops reach are reported, and a name declared more than once is told
//! for every one of them, a fetched dependency's too, since which
//! declaration the build means is a guess whoever's files hold them.

use astli_index::Summary;
use astli_sema::{Built, check};
use astli_text::Diagnostic;
use rayon::prelude::*;
use usage_rs::{Args, RunWith};

use crate::analysis::{self, Analysis};
use crate::cli::{BuildArgs, FeatureArgs, RunArgs, Sources};
use crate::cmd::{Ctx, files};
use crate::discovery::Plan;
use crate::error::{Error, Result};
use crate::render::Out;

/// Verify design semantics, port connections, parameter overrides, and undeclared identifiers
#[derive(Args)]
pub struct Check {
    #[usage(flatten)]
    pub sources: Sources,
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
        let mut plan = Plan::new(
            &self.sources,
            &self.features,
            &self.build,
            Some(&self.top),
            run,
        )?;
        plan.tops.set(&self.param)?;
        let (analysis, checked) = checked(out, &mut plan, run)?;
        analysis.leak();
        checked
    }
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

    let told_files: Vec<_> = (shown.iter().chain(&short))
        .map(|&file| {
            let mut checked = std::mem::take(&mut found[file]);
            checked.sort_by_key(|diagnostic| (diagnostic.at.src_id, diagnostic.at.start));
            // What the expansion and the parser said comes first, as they
            // found it.
            let mut all = std::mem::take(&mut parsed[file]);
            all.extend(checked);
            (Some(&analysis.origins[file]), Vec::new(), 0, all)
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
