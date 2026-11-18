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
//!
//! Without `--top`, a package with `[top.NAME]` tables is checked from
//! every one of them, and with `--tests` from every point of each test
//! besides: one design for each set of defines they give, each as `--top`
//! would check its tops together, and what they find is reported file by
//! file, a finding several designs meet once. `-G` then sets a parameter in
//! every top that declares it, and fails only where none does.

use std::collections::HashMap;
use std::path::PathBuf;

use astli_index::Summary;
use astli_sema::{Built, check, check_built};
use astli_text::Diagnostic;
use rayon::prelude::*;
use usage_rs::{Args, RunWith};

use crate::analysis::{self, Analysis};
use crate::cli::{BuildArgs, FeatureArgs, Members, Sources};
use crate::cmd::{self, Ctx, files};
use crate::discovery::Plan;
use crate::error::{Error, Result};
use crate::points;
use crate::render::Out;
use crate::run::Run;
use crate::tops::Flags;

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
    /// Also elaborate every point of every test the package marks
    #[usage(long)]
    pub tests: bool,
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
        points::asked(self.tests, self.sources.named())?;
        cmd::members(
            run,
            &self.members,
            self.sources.named(),
            out,
            |out, target| {
                let tops = (!self.top.is_empty()).then_some(self.top.as_slice());
                let plan = Plan::new(
                    target,
                    &self.sources,
                    &self.features,
                    &self.build,
                    tops,
                    run,
                )?;
                let tests = match self.tests {
                    true => points::names(points::marked(out, &plan, run)?),
                    false => Vec::new(),
                };
                let mut designs = plan.designs(&tests);
                let mut flags = Flags::default();
                match (tops.is_some() && tests.is_empty())
                    || designs.iter().all(|it| it.modules.is_empty())
                {
                    true => designs
                        .iter_mut()
                        .try_for_each(|tops| tops.set(&self.param))?,
                    false => flags = Flags::new(&self.param)?,
                }
                if designs.len() == 1 {
                    let mut plan = plan.for_tops(designs.remove(0));
                    let mut analysis = Analysis::new(out, &mut plan, run, true)?;
                    flags.set(&mut analysis, points::swept)?;
                    flags.undeclared("no top")?;
                    let (analysis, checked) = checked(out, run, &plan, analysis)?;
                    // Left to the process's exit, unless another package's run
                    // follows and would add its design to this one's.
                    match run.several_packages() {
                        true => drop(analysis),
                        false => analysis.leak(),
                    }
                    return checked;
                }
                // Each design tells only what none before it told, and what
                // they found is reported once, a file's findings together.
                run.tell_once();
                let mut all = Found::default();
                for tops in designs {
                    let mut plan = plan.for_tops(tops);
                    let mut analysis = Analysis::new(out, &mut plan, run, true)?;
                    flags.set(&mut analysis, points::swept)?;
                    let (analysis, found) = found(run, &plan, analysis)?;
                    listed(run, &plan, &analysis, &found.told);
                    all.add(found);
                }
                flags.undeclared("no top")?;
                all.report(out)?
            },
        )
    }
}

/// What the check of a design found, rendered, to be reported alone or
/// with what the checks of other designs of the package found.
#[derive(Default)]
pub(crate) struct Found {
    /// Each file told, in the order told.
    files: Vec<Told>,
    /// How many of the plan's files could not be read.
    unread: usize,
    /// Why a name the design uses is declared more than once, if one is.
    twice: Option<Error>,
    /// The units whose list `files` writes for the design: those its tops
    /// reach, or every one without a top.
    pub told: Vec<usize>,
}

/// A file a check tells: what was rendered of it and how many errors that
/// held, and whether it is a fetched dependency's that is short of a
/// header, which is told after the rest.
struct Told {
    path: PathBuf,
    said: Vec<u8>,
    errors: usize,
    short: bool,
}

impl Found {
    /// Adds what the check of another design found: a file both told is
    /// told once, with what each found in it.
    pub(crate) fn add(&mut self, other: Found) {
        // Each design reads every file, so each fails to read the same.
        self.unread = self.unread.max(other.unread);
        self.twice = self.twice.take().or(other.twice);
        if self.files.is_empty() {
            self.files = other.files;
            return;
        }
        let mut at: HashMap<PathBuf, usize> = (self.files.iter().enumerate())
            .map(|(at, file)| (file.path.clone(), at))
            .collect();
        for file in other.files {
            match at.get(&file.path) {
                Some(&before) => {
                    self.files[before].said.extend(file.said);
                    self.files[before].errors += file.errors;
                }
                None => {
                    at.insert(file.path.clone(), self.files.len());
                    self.files.push(file);
                }
            }
        }
        (self.files).sort_by(|a, b| (a.short, &a.path).cmp(&(b.short, &b.path)));
    }

    /// Writes what was found, and hands back whether the check passed.
    pub(crate) fn report(self, out: &mut Out) -> Result<Result> {
        let rendered = (self.files.into_iter())
            .map(|file| (file.said, file.errors))
            .collect();
        let mut reported = analysis::write(out, rendered)?;
        reported.failed = self.unread;
        let reported = reported.finish();
        Ok(match self.twice {
            Some(err) => Err(err),
            None => reported,
        })
    }
}

/// Checks `analysis`, the design `plan` holds, from its tops if it has
/// any, reporting what it finds. Hands back the analysis, whose index of
/// the files and tops a build of them lists, and whether the check passed.
fn checked(
    out: &mut Out,
    run: &Run,
    plan: &Plan,
    analysis: Analysis,
) -> Result<(Analysis, Result)> {
    let (analysis, mut found) = found(run, plan, analysis)?;
    let told = std::mem::take(&mut found.told);
    let passed = found.report(out)?;
    listed(run, plan, &analysis, &told);
    Ok((analysis, passed))
}

/// Checks the list `files` writes for the design `plan` holds, given the
/// units `told` it lists, as `files` checks it.
pub(crate) fn listed(run: &Run, plan: &Plan, analysis: &Analysis, told: &[usize]) {
    if !plan.is_package() {
        return;
    }
    let index = analysis
        .index
        .as_ref()
        .expect("check summarizes every unit");
    let listed: Vec<&Summary> = match analysis.tops.is_empty() {
        true => index.summaries().iter().collect(),
        false => told.iter().map(|&file| &index.summaries()[file]).collect(),
    };
    let incdirs = match analysis.tops.is_empty() {
        true => plan.incdirs.clone(),
        false => files::used(&plan.incdirs, listed.iter().copied()),
    };
    files::misfound(run, &incdirs, listed.into_iter());
}

/// Checks `analysis`, the design `plan` holds, from its tops if it has
/// any, and renders what it finds. Hands back the analysis, whose index of
/// the files and tops a build of them lists, and what was found.
pub(crate) fn found(run: &Run, plan: &Plan, mut analysis: Analysis) -> Result<(Analysis, Found)> {
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
            let of_built = check_built(elaboration, &built);
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
    let twice = files::redeclared(run, index, &told, &tops);
    if !tops.is_empty() {
        files::foreign_binds(run, plan, index, &told);
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
            // An error a design checked before told still fails this one,
            // so that each package that has it is counted.
            let mut told_before = 0;
            if run.tells_once() {
                all.retain(|diagnostic| {
                    let first = run.first_found(&plan.units[file].path, diagnostic);
                    told_before += usize::from(!first && diagnostic.is_error());
                    first
                });
            }
            (Some(&*analysis.origins[file]), Vec::new(), told_before, all)
        })
        .collect();
    let rendered = analysis::render(told_files)?;
    let files = (shown.iter().map(|&file| (file, false)))
        .chain(short.iter().map(|&file| (file, true)))
        .zip(rendered)
        .map(|((file, short), (said, errors))| Told {
            path: plan.units[file].path.clone(),
            said,
            errors,
            short,
        })
        .collect();
    let found = Found {
        files,
        unread: analysis.unread,
        twice: twice.err(),
        told,
    };
    Ok((analysis, found))
}
