//! `astli test` subcommand.
//!
//! A test is a module of the package the command acts on marked
//! `(* astli_test *)`, in any of its files, dev sources included: a
//! dependency's tests are its own CI's, and so are those of a sibling the
//! package requires by `path`. Each file is read as written, every branch
//! of a conditional, since a `[top.NAME]` table's defines may decide
//! whether a test is declared at all.
//!
//! A test is run once for each point its sweeps give, named for the values
//! of its swept parameters, `tb_fifo[Depth=16,Mode=Bypass]`, or for the
//! test alone when it sweeps nothing. The values are read once the design
//! is elaborated, so the tests are elaborated even to be listed. A filter
//! selects the points whose name holds it, or with `--exact` the one it
//! names, and one matching no point of any package is an error, so that a
//! misspelt name fails CI rather than passing it. `--list` writes the
//! points selected. `-G` sets a parameter in every selected test that
//! declares it, and is an error where none does, or on a swept parameter,
//! whose value a filter selects.
//!
//! Otherwise each point is a top as `build` builds one, under the
//! `[top.NAME]` table named as its test if there is one: checked once for
//! each set of defines the tables give, every design before any build,
//! then built and run in parallel, a point in a directory of its own in
//! its test's, named for its values' hash, once under the seed its name
//! gives or under each of several random seeds from one build. A run
//! starts in `run/<seed>/` in the point's build directory, told the root
//! of its package, and passes if the simulation exits with status 0. Its
//! output is kept as a log there, and shown for a failure, with the
//! command that repeats it. `--junit` writes how every run went as a JUnit
//! report too.

use std::collections::HashSet;
use std::fmt;
use std::fs::File;
use std::hash::{BuildHasher, RandomState};
use std::io::{ErrorKind, Write};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use astli_package::mark;
use usage_rs::{Args, RunWith};

use crate::analysis::{Analysis, Elaborated};
use crate::cli::{BuildArgs, FeatureArgs, Members, RunArgs, Sources};
use crate::cmd::build::{Builder, Listed, Sim, listed_tops};
use crate::cmd::{self, Ctx};
use crate::discovery::Plan;
use crate::error::{Error, Result};
use crate::points;
use crate::render::{self, Out, plural};
use crate::term::{self, status};
use crate::tops::{Flags, Top, grouped, point_of};

/// The largest seed Verilator takes.
const MAX_SEED: u32 = i32::MAX as u32;

/// How long a run goes before it is said to be still running.
const SLOW: Duration = Duration::from_secs(60);

/// Build and run the tests the package marks
#[derive(Args)]
pub struct Test {
    /// Select the tests whose name holds this, or with `--exact` the one it names
    #[usage(arg, value_name = "FILTER")]
    pub filter: Option<String>,
    /// Select only the test whose whole name is the filter
    #[usage(long)]
    pub exact: bool,
    /// List the tests selected, one per line, without building them
    #[usage(long)]
    pub list: bool,
    /// Build the tests selected, and run none
    #[usage(long)]
    pub no_run: bool,
    /// Show the output of every run, not only of one that fails
    #[usage(long)]
    pub nocapture: bool,
    /// Run each test with this seed, in place of the one its name gives
    #[usage(long, value_name = "SEED")]
    pub seed: Option<u32>,
    /// Run each test under this many random seeds, in place of the one its name gives
    #[usage(long, value_name = "N")]
    pub seeds: Option<u32>,
    /// Also write a JUnit report of the runs to this file
    #[usage(long, value_name = "PATH")]
    pub junit: Option<PathBuf>,
    #[usage(flatten)]
    pub members: Members,
    #[usage(flatten)]
    pub features: FeatureArgs,
    /// Build with the release profile, which compiles longer and simulates faster
    #[usage(long)]
    pub release: bool,
    /// Build so that the simulation can write waveforms, whatever the profile sets
    #[usage(long)]
    pub trace: bool,
    /// Simulator to build with, in place of the manifest's [build] simulator
    #[usage(long, value_enum)]
    pub sim: Option<Sim>,
    /// Add directory to the `include` search path (can be repeated)
    #[usage(short = 'I', long = "incdir")]
    pub incdir: Vec<PathBuf>,
    /// Define preprocessor macro as NAME or NAME=VALUE (can be repeated)
    #[usage(short = 'D', long = "define")]
    pub define: Vec<String>,
    /// Set a parameter as NAME=VALUE in each test that declares it, over what its [top.NAME] table sets (can be repeated)
    #[usage(short = 'G', long = "param")]
    pub param: Vec<String>,
}

/// One build and run of a test: the test, and the value of each parameter
/// it sweeps, in declaration order. Named `tb_fifo[Depth=16,Mode=Bypass]`,
/// which is what a filter matches, or the test's name alone when it sweeps
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    pub test: String,
    pub values: Vec<(String, String)>,
}

impl Point {
    /// The point `top` is.
    fn of(top: &Elaborated) -> Point {
        Point {
            test: point_of(&top.name).unwrap_or(&top.name).to_string(),
            values: (top.values.iter())
                .map(|(name, value)| (name.clone(), value.short()))
                .collect(),
        }
    }

    /// The seed a run of the point takes without `--seed`: one its name
    /// gives, so that a rerun is the same run. FNV-1a, since `std`'s hasher
    /// may change between builds, kept to what Verilator takes, below 2^31,
    /// and never 0.
    fn seed(&self) -> u32 {
        let hash = (self.to_string().bytes()).fold(0x811c_9dc5_u32, |hash, byte| {
            (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
        });
        (hash & MAX_SEED).max(1)
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.test)?;
        if self.values.is_empty() {
            return Ok(());
        }
        let values: Vec<String> = (self.values.iter())
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        write!(f, "[{}]", values.join(","))
    }
}

/// The points one package's run selects, and once checked, what builds
/// them.
struct Selected {
    /// What the package is called, or its directory's path without a name.
    package: String,
    /// The package's root, which a run is told of.
    root: PathBuf,
    points: Vec<Point>,
    /// For each point, the entries of its test's `astli_plusargs` whose
    /// run the filter selects, or `None` alone without one.
    entries: Vec<Vec<Option<String>>>,
    /// How many random seeds the package's `[test]` table runs each point
    /// under.
    seeds: Option<NonZeroU32>,
    /// What builds the points, once checked; `None` under `--list`, with
    /// no point selected, or if the check failed.
    built: Option<Built>,
    /// What `-G` sets, and whether a selected test declares each.
    flags: Flags,
}

/// What builds a package's points, and for each, in order, what its build
/// reads and how long a run may take.
struct Built {
    builder: Builder,
    lists: Vec<(Listed, Option<Duration>)>,
}

/// One point to build, and run with each of its entries under each of its
/// seeds.
struct Case<'a> {
    /// The point as the run shows it: with its package's name before it
    /// where the run reads several.
    shown: String,
    point: &'a Point,
    package: &'a Selected,
    builder: &'a Builder,
    listed: &'a Listed,
    timeout: Option<Duration>,
    entries: &'a [Option<String>],
    seeds: Vec<u32>,
    /// Whether the seeds were drawn at random, and so a run's name says
    /// which it took.
    random: bool,
}

impl Case<'_> {
    /// How many runs the case makes.
    fn runs(&self) -> usize {
        self.entries.len() * self.seeds.len()
    }

    /// What the run with `entry` under `seed` is shown as.
    fn run(&self, entry: Option<&str>, seed: u32) -> String {
        let mut shown = with_entry(&self.shown, entry);
        if self.random {
            shown.push_str(&format!(" seed {seed}"));
        }
        shown
    }
}

/// The name of the run of `point` with `entry`, which is what a filter
/// matches.
fn with_entry(point: &str, entry: Option<&str>) -> String {
    match entry {
        Some(entry) => format!("{point} {entry}"),
        None => point.to_string(),
    }
}

/// How a case went: why its build failed, or how each of its runs did.
enum Tested {
    Unbuilt(String),
    Built(Vec<Ran>),
}

/// How one run went.
struct Ran {
    entry: Option<String>,
    seed: u32,
    took: Duration,
    /// Why it failed, and what it wrote, if it did; `None` if it passed or
    /// was not run.
    failed: Option<(String, Vec<u8>)>,
}

impl RunWith<Ctx<'_>> for Test {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        if let Some(seed) = self.seed.filter(|it| *it > MAX_SEED) {
            return Err(Error::failed(format!(
                "{} is larger than the largest seed a simulator takes, {MAX_SEED}",
                mark::code(format!("--seed {seed}"))
            )));
        }
        if self.exact && self.filter.is_none() {
            return Err(Error::failed(format!(
                "{} takes the name of a test",
                mark::code("--exact")
            )));
        }
        match (self.seed, self.seeds) {
            (Some(_), Some(_)) => {
                return Err(Error::failed(format!(
                    "{} runs each test under one seed, and {} under several",
                    mark::code("--seed"),
                    mark::code("--seeds")
                )));
            }
            (_, Some(0)) => {
                return Err(Error::failed(format!(
                    "{} takes a number of seeds, more than none",
                    mark::code("--seeds")
                )));
            }
            _ => {}
        }

        let mut selected: Vec<Selected> = Vec::new();
        let checked = cmd::members(&self.members, false, out, |out| {
            let mut unchecked = None;
            let found = self.selected(out, run, &mut unchecked);
            selected.extend(unchecked);
            selected.push(found?);
            Ok(())
        });
        if let Some(filter) = &self.filter
            && selected.iter().all(|it| it.points.is_empty())
        {
            checked?;
            let how = if self.exact { "is" } else { "is part of" };
            return Err(
                term::error(format!("no test's name {how} {}", mark::code(filter)))
                    .help(format!(
                        "{} lists every test",
                        mark::code("astli test --list")
                    ))
                    .into(),
            );
        }
        // Under `--workspace`, a parameter need only be declared in one
        // package.
        let mut flags = Flags::new(&self.param)?;
        selected.iter().for_each(|it| flags.add(&it.flags));
        if checked.is_ok() && selected.iter().any(|it| !it.points.is_empty()) {
            flags.undeclared("no test selected")?;
        }
        let several = selected.len() > 1;
        selected.sort_by(|a, b| a.package.cmp(&b.package));

        if self.list {
            for package in &selected {
                for (point, entries) in package.points.iter().zip(&package.entries) {
                    for entry in entries {
                        let run = with_entry(&point.to_string(), entry.as_deref());
                        match several {
                            true => writeln!(out, "{} {run}", package.package)?,
                            false => writeln!(out, "{run}")?,
                        }
                    }
                }
            }
            return checked;
        }
        if several {
            for package in &mut selected {
                if let Some(built) = &mut package.built {
                    built.builder.shown_package = Some(package.package.clone());
                }
            }
        }

        let mut cases = Vec::new();
        for package in &selected {
            let Some(Built { builder, lists }) = &package.built else {
                continue;
            };
            let points = (package.points.iter()).zip(&package.entries).zip(lists);
            for ((point, entries), (listed, timeout)) in points {
                let count = self.seeds.and_then(NonZeroU32::new).or(package.seeds);
                let (seeds, random) = match (self.seed, count) {
                    (Some(seed), _) => (vec![seed], false),
                    (None, Some(count)) => (random_seeds(count), true),
                    (None, None) => (vec![point.seed()], false),
                };
                cases.push(Case {
                    shown: match several {
                        true => format!("{} {point}", package.package),
                        false => point.to_string(),
                    },
                    point,
                    package,
                    builder,
                    listed,
                    timeout: *timeout,
                    entries,
                    seeds,
                    random,
                });
            }
        }
        if cases.is_empty() {
            if checked.is_ok() {
                term::note("no module of the package is marked `astli_test`").report();
            }
            self.reported(&selected, &[], &[])?;
            return checked;
        }
        out.flush()?;
        let tested = self.tested(&cases, run);
        self.reported(&selected, &cases, &tested)?;

        // A point that did not build is named once, and is each of its runs
        // failed.
        let mut failures = Vec::new();
        let mut count = 0;
        for (case, tested) in cases.iter().zip(&tested) {
            match tested {
                Tested::Unbuilt(_) => {
                    failures.push(case.shown.clone());
                    count += case.runs();
                }
                Tested::Built(runs) => {
                    for ran in runs.iter().filter(|ran| ran.failed.is_some()) {
                        failures.push(case.run(ran.entry.as_deref(), ran.seed));
                        count += 1;
                    }
                }
            }
        }
        let tally = (!failures.is_empty()).then(|| {
            let names: Vec<String> = failures.iter().map(mark::name).collect();
            format!(
                "{count} of {} failed: {}",
                counted(&cases),
                names.join(", ")
            )
        });
        match (checked, tally) {
            (Ok(()), None) => Ok(()),
            (Ok(()), Some(tally)) => Err(Error::tally(tally)),
            (Err(err), tally) => {
                let owned = render::FORMAT.get().is_some_and(|it| it.owns_stderr());
                if let Some(tally) = tally.filter(|_| !owned) {
                    term::error(tally).report();
                }
                Err(err)
            }
        }
    }
}

impl Test {
    /// The points the package the run acts on selects, and unless the run
    /// only lists them, what builds them once every design of theirs is
    /// checked. If the check fails, what it selected is left in
    /// `unchecked`, so that the report can name its points.
    fn selected(
        &self,
        out: &mut Out,
        run: &RunArgs,
        unchecked: &mut Option<Selected>,
    ) -> Result<Selected> {
        let build = BuildArgs {
            incdir: self.incdir.clone(),
            define: self.define.clone(),
            ..BuildArgs::default()
        };
        let plan = Plan::new(&Sources::default(), &self.features, &build, None, run)?;
        let Some((package, _)) = plan.packages() else {
            return Err(Error::failed("a test is a package's"));
        };
        let mut selected = Selected {
            package: (package.name.clone()).unwrap_or_else(|| package.root.display().to_string()),
            root: package.root.clone(),
            points: Vec::new(),
            entries: Vec::new(),
            seeds: package.tests.seeds,
            built: None,
            flags: Flags::new(&self.param)?,
        };
        let tests = points::marked(out, &plan, run)?;
        if tests.is_empty() {
            return Ok(selected);
        }
        let tops = (tests.iter())
            .map(|test| Top::test(package, &test.name))
            .collect();
        let marked = |point: &Point| {
            (tests.iter())
                .find(|it| it.name == point.test)
                .expect("a point is of a test marked")
        };
        let designs = grouped(tops).into_iter().map(|tops| plan.for_tops(tops));

        let mut points = Vec::new();
        let mut chosen = |analysis: &mut Analysis| {
            (analysis.tops).retain(|top| {
                let point = Point::of(top);
                !self.runs(&point, &marked(&point).plusargs).is_empty()
            });
            selected.flags.set(analysis, |_, name, value| {
                format!(
                    "select the points of one value with a filter, as in {}",
                    mark::code(format!("astli test '{name}={value}'"))
                )
            })?;
            points.extend(analysis.tops.iter().map(Point::of));
            Ok(true)
        };
        let lists = match self.list {
            true => {
                for mut plan in designs {
                    let mut analysis = Analysis::new(out, &mut plan, run.jobs, true)?;
                    chosen(&mut analysis)?;
                    analysis.leak();
                }
                None
            }
            false => Some(listed_tops(out, designs.collect(), run, chosen)),
        };
        // The points of one test stay in the order its sweeps give them.
        points.sort_by(|a, b| a.test.cmp(&b.test));
        selected.entries = (points.iter())
            .map(|point| self.runs(point, &marked(point).plusargs))
            .collect();
        selected.points = points;
        let lists = match lists.transpose() {
            Ok(lists) => lists,
            Err(err) => {
                *unchecked = Some(selected);
                return Err(err);
            }
        };
        let Some(mut lists) = lists.filter(|_| !selected.points.is_empty()) else {
            return Ok(selected);
        };

        let builder = Builder::new(&plan, self.release, self.trace, self.sim)?;
        let lists = (selected.points.iter())
            .map(|point| {
                let name = point.to_string();
                let at =
                    (lists.iter().position(|it| it.name == name)).expect("every point is listed");
                let timeout = (package.tops.get(&point.test))
                    .and_then(|table| table.timeout)
                    .or(package.tests.timeout);
                (lists.swap_remove(at), timeout)
            })
            .collect();
        selected.built = Some(Built { builder, lists });
        Ok(selected)
    }

    /// The runs of `point`, whose test lists `plusargs`, that the filter
    /// selects: each entry's, or one with `None` where it lists none.
    fn runs(&self, point: &Point, plusargs: &[String]) -> Vec<Option<String>> {
        let point = point.to_string();
        let entries = match plusargs.is_empty() {
            true => vec![None],
            false => plusargs.iter().cloned().map(Some).collect(),
        };
        (entries.into_iter())
            .filter(|entry| self.selects(&with_entry(&point, entry.as_deref())))
            .collect()
    }

    /// Whether the filter selects the run called `name`: every run
    /// without one.
    fn selects(&self, name: &str) -> bool {
        let Some(filter) = &self.filter else {
            return true;
        };
        match self.exact {
            true => name == *filter,
            false => name.contains(filter.as_str()),
        }
    }

    /// Builds every case, and unless `--no-run` runs every one that built
    /// with each of its entries under each of its seeds, whatever failed
    /// before it, and hands back
    /// how each case went. Builds share the run's jobs with each Verilator
    /// build, and runs take one each.
    fn tested(&self, cases: &[Case], run: &RunArgs) -> Vec<Tested> {
        let started = Instant::now();
        let jobs = match run.jobs {
            0 => std::thread::available_parallelism().map_or(1, |it| it.get()),
            jobs => jobs,
        };
        let builds = jobs.min(cases.len());
        let built = parallel(cases, builds, |case| {
            case.builder
                .build(case.listed, (jobs / builds).max(1))
                .map_err(|err| {
                    let mut problem = err.problem();
                    let why = mark::plain(&problem.message);
                    problem.message = format!("{}: {}", case.shown, problem.message);
                    problem.report();
                    why
                })
        });
        let unbuilt = built.iter().any(|it| it.is_err());
        if self.no_run {
            if !unbuilt {
                status!(
                    "Finished",
                    "building {} in {}",
                    plural(cases.len(), "test"),
                    render::duration(started.elapsed())
                );
            }
            return (built.into_iter())
                .map(|built| match built {
                    Ok(_) => Tested::Built(Vec::new()),
                    Err(why) => Tested::Unbuilt(why),
                })
                .collect();
        }

        let mut runs: Vec<(&Case, &PathBuf, usize, u32)> = Vec::new();
        for (case, built) in cases.iter().zip(&built) {
            let Ok(executable) = built else {
                continue;
            };
            for at in 0..case.entries.len() {
                for seed in &case.seeds {
                    runs.push((case, executable, at, *seed));
                }
            }
        }
        let mut ran = parallel(
            &runs,
            jobs.min(runs.len()).max(1),
            |(case, executable, at, seed)| self.ran(case, executable, *at, *seed),
        )
        .into_iter();
        let tested: Vec<Tested> = (cases.iter().zip(built))
            .map(|(case, built)| match built {
                Ok(_) => Tested::Built(ran.by_ref().take(case.runs()).collect()),
                Err(why) => Tested::Unbuilt(why),
            })
            .collect();
        let passed = tested.iter().all(|it| match it {
            Tested::Unbuilt(_) => false,
            Tested::Built(runs) => runs.iter().all(|ran| ran.failed.is_none()),
        });
        if passed {
            status!(
                "Finished",
                "{} in {}, all passed",
                counted(cases),
                render::duration(started.elapsed())
            );
        }
        tested
    }

    /// Runs `case`, built as `executable`, with its entry at `at` under
    /// `seed`, and says how it went: a status if it passed, an error with
    /// the command that repeats it if not, and its output if it failed or
    /// `--nocapture` asks.
    fn ran(&self, case: &Case, executable: &Path, at: usize, seed: u32) -> Ran {
        let entry = case.entries[at].as_deref();
        let shown = case.run(entry, seed);
        let mut dir = (executable.parent().unwrap_or(Path::new("."))).join("run");
        if let Some(entry) = entry {
            dir.push(entry_dir(at, entry));
        }
        dir.push(seed.to_string());
        let log = dir.join("output.log");
        let started = Instant::now();
        let slow = || {
            status!(
                "Running",
                "{}, for over {}s",
                mark::name(&shown),
                SLOW.as_secs()
            )
        };
        let mut args = vec![
            format!("+astli_root={}", case.package.root.display()),
            format!("+verilator+seed+{seed}"),
        ];
        args.extend(
            entry
                .into_iter()
                .flat_map(str::split_whitespace)
                .map(str::to_string),
        );
        let ended = simulated(executable, &dir, &log, &args, case.timeout, slow);
        let took = started.elapsed();
        let output = std::fs::read(&log).unwrap_or_default();
        let shown_took = render::duration(took);
        let why = match ended {
            Ok(Ended::Status(status)) if status.success() => {
                status!("Passed", "{} in {shown_took}", mark::name(&shown));
                if self.nocapture {
                    shown_output(&output);
                }
                return Ran {
                    entry: entry.map(str::to_string),
                    seed,
                    took,
                    failed: None,
                };
            }
            Ok(Ended::Status(status)) => format!("{}, after {shown_took}", ended_by(status)),
            Ok(Ended::TimedOut(after)) => {
                format!("it ran past its timeout of {}s", after.as_secs())
            }
            Err(err) => {
                let mut problem = err.problem();
                let why = mark::plain(&problem.message);
                problem.message = format!("{shown}: {}", problem.message);
                problem.report();
                return Ran {
                    entry: entry.map(str::to_string),
                    seed,
                    took,
                    failed: Some((why, output)),
                };
            }
        };
        let mut repeat = format!(
            "astli test --exact {} --seed {seed} --trace",
            quoted(&with_entry(&case.point.to_string(), entry))
        );
        if case.shown != case.point.to_string() {
            repeat.push_str(&format!(" -p {}", quoted(&case.package.package)));
        }
        if self.release {
            repeat.push_str(" --release");
        }
        let here = std::env::current_dir().unwrap_or_default();
        let shown_log = log.strip_prefix(&here).unwrap_or(&log);
        term::error(format!("{} failed: {why}", mark::name(&shown)))
            .note(format!(
                "its output is in {}",
                mark::path(shown_log.display())
            ))
            .help(format!("{} repeats it", mark::code(repeat)))
            .report();
        shown_output(&output);
        Ran {
            entry: entry.map(str::to_string),
            seed,
            took,
            failed: Some((why, output)),
        }
    }

    /// Writes the JUnit report `--junit` asks for, if it does, of how each
    /// of `cases` went: a test suite for each package with a point
    /// selected, and a test case for each run. Under `--no-run` a point
    /// built is a case skipped, and one whose package's check failed is a
    /// case in error, so that the report names every point selected.
    fn reported(&self, selected: &[Selected], cases: &[Case], tested: &[Tested]) -> Result {
        let Some(path) = &self.junit else {
            return Ok(());
        };
        let mut suites = Vec::new();
        for package in selected.iter().filter(|it| !it.points.is_empty()) {
            let mut suite = Suite {
                name: package.package.clone(),
                ..Suite::default()
            };
            let ours =
                (cases.iter().zip(tested)).filter(|(case, _)| std::ptr::eq(case.package, package));
            for (case, tested) in ours {
                match tested {
                    Tested::Unbuilt(why) => {
                        suite.case(
                            &case.point.to_string(),
                            Duration::ZERO,
                            Verdict::Failed(why, b""),
                        );
                    }
                    Tested::Built(runs) if runs.is_empty() => {
                        suite.case(
                            &case.point.to_string(),
                            Duration::ZERO,
                            Verdict::Skipped("built, and not run"),
                        );
                    }
                    Tested::Built(runs) => {
                        for ran in runs {
                            let mut name =
                                with_entry(&case.point.to_string(), ran.entry.as_deref());
                            if case.random {
                                name.push_str(&format!(" seed {}", ran.seed));
                            }
                            let verdict = match &ran.failed {
                                Some((why, output)) => Verdict::Failed(why, output),
                                None => Verdict::Passed,
                            };
                            suite.case(&name, ran.took, verdict);
                        }
                    }
                }
            }
            if package.built.is_none() {
                for (point, entries) in package.points.iter().zip(&package.entries) {
                    for entry in entries {
                        suite.case(
                            &with_entry(&point.to_string(), entry.as_deref()),
                            Duration::ZERO,
                            Verdict::Error("its package's check failed"),
                        );
                    }
                }
            }
            suites.push(suite);
        }
        let text = junit(&suites);
        std::fs::write(path, text).map_err(|err| Error::io(path, err))
    }
}

/// The directory of the runs of the entry `entry`, at `at` among its
/// test's: numbered, so that two entries whose names differ only in what
/// a path cannot hold never share it.
fn entry_dir(at: usize, entry: &str) -> String {
    let plain: String = (entry.chars())
        .map(|c| match c.is_ascii_alphanumeric() || "+=-_.".contains(c) {
            true => c,
            false => '_',
        })
        .collect();
    format!("{}-{}", at + 1, plain.trim_start_matches('+'))
}

/// `count` seeds, each different, drawn at random below 2^31, which is what
/// Verilator takes, and never 0.
fn random_seeds(count: NonZeroU32) -> Vec<u32> {
    // Keyed at random for each run, and so a source of random numbers
    // without a dependency for them.
    let state = RandomState::new();
    let mut seeds = Vec::new();
    let mut taken = HashSet::new();
    for at in 0_u64.. {
        if seeds.len() == count.get() as usize {
            break;
        }
        let seed = ((state.hash_one(at) as u32) & MAX_SEED).max(1);
        if taken.insert(seed) {
            seeds.push(seed);
        }
    }
    seeds
}

/// How many runs `cases` make, called tests where each point runs once.
fn counted(cases: &[Case]) -> String {
    let runs = cases.iter().map(Case::runs).sum();
    match runs == cases.len() {
        true => plural(runs, "test"),
        false => plural(runs, "run"),
    }
}

/// The test cases of one package's test suite in a JUnit report, written.
#[derive(Default)]
struct Suite {
    /// The package's name, which is also each case's class: how most CI
    /// views group the cases of a report.
    name: String,
    cases: String,
    tests: usize,
    failures: usize,
    errors: usize,
    skipped: usize,
    time: Duration,
}

/// How a test case of a JUnit report went.
enum Verdict<'a> {
    Passed,
    /// It failed for this reason, and wrote this.
    Failed(&'a str, &'a [u8]),
    /// It was never built, for this reason.
    Error(&'a str),
    Skipped(&'a str),
}

impl Suite {
    fn case(&mut self, name: &str, time: Duration, verdict: Verdict) {
        self.tests += 1;
        self.time += time;
        let opened = format!(
            "    <testcase name=\"{}\" classname=\"{}\" time=\"{:.3}\"",
            escaped(name),
            escaped(&self.name),
            time.as_secs_f64()
        );
        let body = match verdict {
            Verdict::Passed => {
                self.cases.push_str(&format!("{opened}/>\n"));
                return;
            }
            Verdict::Failed(why, output) => {
                self.failures += 1;
                let output = String::from_utf8_lossy(output);
                format!(
                    "      <failure message=\"{}\">{}</failure>\n",
                    escaped(why),
                    escaped(&output)
                )
            }
            Verdict::Error(why) => {
                self.errors += 1;
                format!("      <error message=\"{}\"/>\n", escaped(why))
            }
            Verdict::Skipped(why) => {
                self.skipped += 1;
                format!("      <skipped message=\"{}\"/>\n", escaped(why))
            }
        };
        self.cases
            .push_str(&format!("{opened}>\n{body}    </testcase>\n"));
    }
}

/// A JUnit report of `suites`. Written by hand, as it is a few elements,
/// and their text and attributes escaped.
fn junit(suites: &[Suite]) -> String {
    let sum = |of: fn(&Suite) -> usize| suites.iter().map(of).sum::<usize>();
    let time: Duration = suites.iter().map(|suite| suite.time).sum();
    let mut text = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <testsuites name=\"astli test\" tests=\"{}\" failures=\"{}\" errors=\"{}\" \
         skipped=\"{}\" time=\"{:.3}\">\n",
        sum(|it| it.tests),
        sum(|it| it.failures),
        sum(|it| it.errors),
        sum(|it| it.skipped),
        time.as_secs_f64()
    );
    for suite in suites {
        text.push_str(&format!(
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" errors=\"{}\" \
             skipped=\"{}\" time=\"{:.3}\">\n{}  </testsuite>\n",
            escaped(&suite.name),
            suite.tests,
            suite.failures,
            suite.errors,
            suite.skipped,
            suite.time.as_secs_f64(),
            suite.cases
        ));
    }
    text.push_str("</testsuites>\n");
    text
}

/// `text` as XML text or an attribute's value: what would end either
/// escaped, and what XML 1.0 cannot hold at all, as a simulation's output
/// may, replaced.
fn escaped(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            '\t' | '\n' | '\r' => escaped.push(c),
            c if c < ' ' || c == '\u{fffe}' || c == '\u{ffff}' => escaped.push('\u{fffd}'),
            c => escaped.push(c),
        }
    }
    escaped
}

/// How a run ended.
enum Ended {
    Status(ExitStatus),
    /// It ran for so long and was killed.
    TimedOut(Duration),
}

/// Runs the simulation `executable` in `dir`, emptied first, with its
/// output in `log`, given `args`, and killed past `timeout`; `slow` is
/// called once it has run for longer than [`SLOW`].
fn simulated(
    executable: &Path,
    dir: &Path,
    log: &Path,
    args: &[String],
    timeout: Option<Duration>,
    slow: impl Fn(),
) -> Result<Ended> {
    match std::fs::remove_dir_all(dir) {
        Err(err) if err.kind() != ErrorKind::NotFound => return Err(Error::io(dir, err)),
        _ => {}
    }
    std::fs::create_dir_all(dir).map_err(|err| Error::io(dir, err))?;
    let file = File::create(log).map_err(|err| Error::io(log, err))?;
    let copy = file.try_clone().map_err(|err| Error::io(log, err))?;
    let mut child = Command::new(executable)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(copy)
        .stderr(file)
        .spawn()
        .map_err(|err| Error::io(executable, err))?;
    let started = Instant::now();
    let mut told = false;
    loop {
        if let Some(status) = child.try_wait().map_err(|err| Error::io(executable, err))? {
            return Ok(Ended::Status(status));
        }
        let elapsed = started.elapsed();
        if let Some(timeout) = timeout.filter(|it| elapsed >= *it) {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(Ended::TimedOut(timeout));
        }
        if !told && elapsed >= SLOW {
            told = true;
            slow();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Why a simulation that ended with `status` failed.
fn ended_by(status: ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("the simulation exited with status {code}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("the simulation was ended by signal {signal}");
        }
    }
    format!("the simulation ended without a status: {status}")
}

/// Writes a run's output to stdout, where a simulation's goes, in one
/// piece, so that runs in parallel do not interleave.
fn shown_output(output: &[u8]) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(output);
    let _ = stdout.flush();
}

/// `text` as a shell reads it back as one word: in single quotes unless it
/// holds only what no shell treats as special. A point's name holds `[`,
/// which zsh takes for a glob.
fn quoted(text: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "_-./=,:".contains(c);
    match !text.is_empty() && text.chars().all(plain) {
        true => text.to_string(),
        false => format!("'{}'", text.replace('\'', r"'\''")),
    }
}

/// `work` on each of `items` on `workers` threads, each taking the next
/// item as it finishes one, and what it gave for each, in order.
fn parallel<T: Sync, R: Send>(
    items: &[T],
    workers: usize,
    work: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..workers.max(1) {
            scope.spawn(|| {
                loop {
                    let at = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(at) else {
                        break;
                    };
                    let result = work(item);
                    done.lock().unwrap_or_else(|it| it.into_inner())[at] = Some(result);
                }
            });
        }
    });
    (done
        .into_inner()
        .unwrap_or_else(|it| it.into_inner())
        .into_iter())
    .map(|it| it.expect("every item is worked on"))
    .collect()
}
