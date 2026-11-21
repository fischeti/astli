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

use astli_index::{Index, Summary};
use astli_sema::{Built, check, check_built};
use astli_text::Diagnostic;
use rayon::prelude::*;
use usage_rs::{Args, RunWith};

use crate::analysis::{self, Analysis, Designs};
use crate::cli::{BuildArgs, FeatureArgs, Members, Sources};
use crate::cmd::{self, Ctx};
use crate::discovery::Plan;
use crate::error::{Error, Result};
use crate::listing;
use crate::points;
use crate::render::Out;
use crate::run::Run;

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
    /// Set a parameter as NAME=VALUE in each top that declares it, over what its [top.NAME] table sets (can be repeated)
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
                let plan = Plan::new(target, &self.sources, &self.features, &self.build, run)?;
                let mut designs = Designs::of(out, &plan, &self.top, self.tests, &self.param, run)?;
                if let [_] = designs.chosen.as_slice() {
                    let checked = designs.each(
                        out,
                        plan,
                        run,
                        true,
                        true,
                        |out, plan, analysis, flags| {
                            flags.check(analysis)?;
                            flags.undeclared("no top")?;
                            checked(out, run, &plan, analysis)
                        },
                    )?;
                    return checked.into_iter().next().expect("one design checked");
                }
                // What the designs found is reported once, a file's findings
                // together.
                let mut all = Found::default();
                designs.each(out, plan, run, true, true, |_, plan, analysis, flags| {
                    flags.check(analysis)?;
                    let found = found(run, &plan, analysis)?;
                    listed(run, &plan, index(analysis), found.told.as_deref());
                    all.add(found);
                    Ok(())
                })?;
                designs.flags.undeclared("no top")?;
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
    /// reach, or `None` without a top, every one.
    pub told: Option<Vec<usize>>,
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
/// any, reporting what it finds. Hands back whether the check passed.
fn checked(out: &mut Out, run: &Run, plan: &Plan, analysis: &mut Analysis) -> Result<Result> {
    let mut found = found(run, plan, analysis)?;
    let told = found.told.take();
    let passed = found.report(out)?;
    listed(run, plan, index(analysis), told.as_deref());
    Ok(passed)
}

/// Checks the list `files` writes for the design `plan` holds, as `files`
/// checks it, given what `index` says of its units and those `told` it
/// lists, or `None` for every one.
pub(crate) fn listed(run: &Run, plan: &Plan, index: &Index, told: Option<&[usize]>) {
    if !plan.is_package() {
        return;
    }
    let listed: Vec<&Summary> = match told {
        None => index.summaries().iter().collect(),
        Some(told) => told.iter().map(|&file| &index.summaries()[file]).collect(),
    };
    let incdirs = listing::incdirs(plan, index, told);
    listing::misfound(run, &incdirs, listed.into_iter());
}

/// What `analysis`, which a check summarizes, says of each unit.
pub(crate) fn index(analysis: &Analysis) -> &Index {
    (analysis.index.as_ref()).expect("check summarizes every unit")
}

/// Checks `analysis`, the design `plan` holds, from its tops if it has
/// any, and renders what it finds, taking what its expansion and parser
/// reported. Hands back what was found.
pub(crate) fn found(run: &Run, plan: &Plan, analysis: &mut Analysis) -> Result<Found> {
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
    let twice = listing::redeclared(run, index, &told, &tops);
    if !tops.is_empty() {
        listing::foreign_binds(run, plan, index, &told);
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
        .filter(|&file| !shows(file) && parsed[file].iter().any(listing::unresolved_include))
        .collect();
    for &file in &short {
        let of = plan.dependency_of(&plan.units[file]);
        parsed[file] = listing::short_of_a_header(std::mem::take(&mut parsed[file]), of);
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
    Ok(Found {
        files,
        unread: analysis.unread,
        twice: twice.err(),
        told: (!tops.is_empty()).then_some(told),
    })
}
