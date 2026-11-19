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
//! command that repeats it. What follows `--` is passed to every run after
//! what astli gives it, as `run` passes it. `--junit` writes how every run
//! went as a JUnit report too.

use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::fmt;
use std::hash::{BuildHasher, RandomState};
use std::io::Write;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use astli_package::{Root, Simulator, mark};
use usage_rs::{Args, RunWith};

use crate::analysis::{Analysis, Elaborated};
use crate::cli::{BuildArgs, FeatureArgs, Members, Sources};
use crate::cmd::build::{Builder, Listed, listed_tops};
use crate::cmd::{self, Ctx};
use crate::discovery::Plan;
use crate::error::{Error, Result};
use crate::lockfile::Target;
use crate::points;
use crate::render::{self, Out, plural};
use crate::run::Run;
use crate::sim::{Backend, Ended, SLOW, ended_by, shown_output, simulated};
use crate::term::{self, status};
use crate::tops::{Flags, Top, grouped, point_of};

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
    pub sim: Option<Simulator>,
    /// Add directory to the `include` search path (can be repeated)
    #[usage(short = 'I', long = "incdir")]
    pub incdir: Vec<PathBuf>,
    /// Define preprocessor macro as NAME or NAME=VALUE (can be repeated)
    #[usage(short = 'D', long = "define")]
    pub define: Vec<String>,
    /// Set a parameter as NAME=VALUE in each test that declares it, over what its [top.NAME] table sets (can be repeated)
    #[usage(short = 'G', long = "param")]
    pub param: Vec<String>,
    /// Arguments passed to every run's simulation, such as plusargs, after `--`
    #[usage(arg, value_name = "ARGS", double_dash = "required")]
    pub args: Vec<OsString>,
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
    fn of(elaborated: &Elaborated) -> Point {
        let name = &elaborated.top.name;
        Point {
            test: point_of(name).unwrap_or(name).to_string(),
            values: (elaborated.values.iter())
                .map(|(name, value)| (name.clone(), value.short()))
                .collect(),
        }
    }

    /// The seed a run of the point takes without `--seed`: one its name
    /// gives, so that a rerun is the same run. FNV-1a, since `std`'s hasher
    /// may change between builds, kept to what the simulator takes, at most
    /// `max`, and never 0.
    fn seed(&self, max: u32) -> u32 {
        let hash = (self.to_string().bytes()).fold(0x811c_9dc5_u32, |hash, byte| {
            (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
        });
        (hash & max).max(1)
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
    /// How long a run may take, from the package's `[test]` table.
    timeout: Option<Duration>,
    /// How many random seeds the package's `[test]` table runs each point
    /// under.
    seeds: Option<NonZeroU32>,
    /// The text each test `astli_should_fail` marks passes only by failing
    /// with in its output, by the test's name.
    should_fail: BTreeMap<String, String>,
    /// What builds the points, once checked; `None` under `--list`, with
    /// no point selected, or if the check failed.
    built: Option<Built>,
    /// What `-G` sets, and whether a selected test declares each.
    flags: Flags,
}

/// What builds a package's points, and for each, in order, what its build
/// reads.
struct Built {
    builder: Builder,
    lists: Vec<Listed>,
}

/// One point to build, and run under each of its seeds.
struct Case<'a> {
    /// The point as the run shows it: with its package's name before it
    /// where the run reads several.
    shown: String,
    point: &'a Point,
    package: &'a Selected,
    builder: &'a Builder,
    listed: &'a Listed,
    seeds: Vec<u32>,
    /// Whether the seeds were drawn at random, and so a run's name says
    /// which it took.
    random: bool,
}

impl Case<'_> {
    /// How many runs the case makes.
    fn runs(&self) -> usize {
        self.seeds.len()
    }

    /// What the run under `seed` is shown as.
    fn run(&self, seed: u32) -> String {
        match self.random {
            true => format!("{} seed {seed}", self.shown),
            false => self.shown.clone(),
        }
    }
}

/// How a case went: why its build failed, or how each of its runs did.
enum Tested {
    Unbuilt(String),
    Built(Vec<Ran>),
}

/// How one run went.
struct Ran {
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
        // Found once, for the first package with a test to build.
        let mut backend = None;
        let checked = cmd::members(run, &self.members, false, out, |out, target| {
            let mut unchecked = None;
            let found = self.selected(out, target, run, &mut backend, &mut unchecked);
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
                for point in &package.points {
                    match several {
                        true => writeln!(out, "{} {point}", package.package)?,
                        false => writeln!(out, "{point}")?,
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
            let max = builder.backend.max_seed();
            for (point, listed) in package.points.iter().zip(lists) {
                let count = self.seeds.and_then(NonZeroU32::new).or(package.seeds);
                let (seeds, random) = match (self.seed, count) {
                    (Some(seed), _) => (vec![seed], false),
                    (None, Some(count)) => (random_seeds(count, max), true),
                    (None, None) => (vec![point.seed(max)], false),
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
                    seeds,
                    random,
                });
            }
        }
        if cases.is_empty() {
            if checked.is_ok() {
                term::note("no module of the package is marked `astli_test`").report(run);
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
                        failures.push(case.run(ran.seed));
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
                    term::error(tally).report(run);
                }
                Err(err)
            }
        }
    }
}

impl Test {
    /// The points the package `target` chose selects, and unless the run
    /// only lists them, what builds them once every design of theirs is
    /// checked, with the simulator `backend` holds or one it finds and
    /// keeps there. If the check fails, what it selected is left in
    /// `unchecked`, so that the report can name its points.
    fn selected(
        &self,
        out: &mut Out,
        target: &Target,
        run: &Run,
        backend: &mut Option<Backend>,
        unchecked: &mut Option<Selected>,
    ) -> Result<Selected> {
        let build = BuildArgs {
            incdir: self.incdir.clone(),
            define: self.define.clone(),
            ..BuildArgs::default()
        };
        let plan = Plan::new(target, &Sources::default(), &self.features, &build, run)?;
        let Some((package, _)) = plan.packages() else {
            return Err(Error::failed("a test is a package's"));
        };
        let Root {
            package, tables, ..
        } = package;
        let mut selected = Selected {
            package: (package.name.clone()).unwrap_or_else(|| package.root.display().to_string()),
            root: package.root.clone(),
            points: Vec::new(),
            timeout: tables.tests.timeout,
            seeds: tables.tests.seeds,
            should_fail: BTreeMap::new(),
            built: None,
            flags: Flags::new(&self.param)?,
        };
        let marked = points::marked(out, &plan, run)?;
        selected.should_fail = (marked.iter())
            .filter_map(|it| Some((it.name.clone(), it.should_fail.clone()?)))
            .collect();
        let tests = points::names(marked);
        if tests.is_empty() {
            return Ok(selected);
        }
        let tops = tests.iter().map(|test| Top::test(tables, test)).collect();
        let designs = grouped(tops)
            .into_iter()
            .map(|tops| (plan.for_tops(&tops), tops));

        let mut points = Vec::new();
        let mut builder = None;
        let mut chosen = |analysis: &mut Analysis| {
            analysis.tops.retain(|top| self.selects(&Point::of(top)));
            // Found before the check, as `build` finds it, once a point is
            // to be built.
            if !self.list && builder.is_none() && !analysis.tops.is_empty() {
                let found = Builder::new(&plan, self.release, self.trace, self.sim, backend)?;
                let max = found.backend.max_seed();
                if let Some(seed) = self.seed.filter(|it| *it > max) {
                    return Err(Error::failed(format!(
                        "{} is larger than the largest seed a simulator takes, {max}",
                        mark::code(format!("--seed {seed}"))
                    )));
                }
                builder = Some(found);
            }
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
                for (mut plan, tops) in designs {
                    let mut analysis = Analysis::new(out, &mut plan, &tops, run, true)?;
                    chosen(&mut analysis)?;
                    analysis.leak();
                }
                None
            }
            false => Some(listed_tops(out, designs.collect(), run, chosen)),
        };
        // The points of one test stay in the order its sweeps give them.
        points.sort_by(|a, b| a.test.cmp(&b.test));
        selected.points = points;
        let lists = match lists.transpose() {
            Ok(lists) => lists,
            Err(err) => {
                *unchecked = Some(selected);
                return Err(err);
            }
        };
        let (Some(builder), Some(mut lists)) =
            (builder, lists.filter(|_| !selected.points.is_empty()))
        else {
            return Ok(selected);
        };
        let lists = (selected.points.iter())
            .map(|point| {
                let name = point.to_string();
                let at = (lists.iter().position(|it| it.elaborated.top.name == name))
                    .expect("every point is listed");
                lists.swap_remove(at)
            })
            .collect();
        selected.built = Some(Built { builder, lists });
        Ok(selected)
    }

    /// Whether the filter selects `point`: every point without one.
    fn selects(&self, point: &Point) -> bool {
        let Some(filter) = &self.filter else {
            return true;
        };
        let name = point.to_string();
        match self.exact {
            true => name == *filter,
            false => name.contains(filter.as_str()),
        }
    }

    /// Builds every case, and unless `--no-run` runs every one that built
    /// under each of its seeds, whatever failed before it, and hands back
    /// how each case went. Builds share the run's jobs with each simulator
    /// build, and runs take one each.
    fn tested(&self, cases: &[Case], run: &Run) -> Vec<Tested> {
        let started = Instant::now();
        let jobs = match run.args.jobs {
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
                    problem.report(run);
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

        let mut runs: Vec<(&Case, &PathBuf, u32)> = Vec::new();
        for (case, built) in cases.iter().zip(&built) {
            let Ok(executable) = built else {
                continue;
            };
            for seed in &case.seeds {
                runs.push((case, executable, *seed));
            }
        }
        let mut ran = parallel(
            &runs,
            jobs.min(runs.len()).max(1),
            |(case, executable, seed)| self.ran(run, case, executable, *seed),
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

    /// Runs `case`, built as `executable`, under `seed`, and says how it
    /// went: a status if it passed, an error with the command that repeats
    /// it if not, and its output if it failed or `--nocapture` asks.
    fn ran(&self, run: &Run, case: &Case, executable: &Path, seed: u32) -> Ran {
        let shown = case.run(seed);
        let dir = (executable.parent().unwrap_or(Path::new(".")))
            .join("run")
            .join(seed.to_string());
        let log = dir.join("output.log");
        let started = Instant::now();
        let spinner = term::spin("Running", &mark::name(&shown));
        // Without a spinner, a log says which run it waits on only once one
        // is slow, as a line for each would double a long run's.
        let slow = || {
            if !spinner.drawn() {
                status!(
                    "Running",
                    "{}, for over {}s",
                    mark::name(&shown),
                    SLOW.as_secs()
                )
            }
        };
        let mut args = case.builder.backend.run_args(&case.package.root, seed);
        args.extend(case.builder.run_args());
        args.extend(self.args.iter().cloned());
        let ended = simulated(executable, &dir, &log, &args, case.package.timeout, slow);
        drop(spinner);
        let took = started.elapsed();
        let output = std::fs::read(&log).unwrap_or_default();
        let shown_took = render::duration(took);
        let should_fail = case.package.should_fail.get(&case.point.test);
        let held = |text: &str| String::from_utf8_lossy(&output).contains(text);
        let passed = match (&ended, should_fail) {
            (Ok(Ended::Status(status)), None) => status.success(),
            (Ok(Ended::Status(status)), Some(text)) => !status.success() && held(text),
            _ => false,
        };
        if passed {
            let how = match should_fail {
                Some(_) => ", failing as it should",
                None => "",
            };
            status!("Passed", "{} in {shown_took}{how}", mark::name(&shown));
            if self.nocapture {
                shown_output(&output);
            }
            return Ran {
                seed,
                took,
                failed: None,
            };
        }
        let why = match ended {
            Ok(Ended::Status(status)) => match should_fail {
                None => format!("{}, after {shown_took}", ended_by(status)),
                Some(text) if status.success() => format!(
                    "it passed, and should fail with {} in its output",
                    mark::code(text)
                ),
                Some(text) => format!(
                    "{}, after {shown_took}, without {} in its output",
                    ended_by(status),
                    mark::code(text)
                ),
            },
            Ok(Ended::TimedOut(after)) => {
                format!("it ran past its timeout of {}s", after.as_secs())
            }
            Err(err) => {
                let mut problem = err.problem();
                let why = mark::plain(&problem.message);
                problem.message = format!("{shown}: {}", problem.message);
                problem.report(run);
                return Ran {
                    seed,
                    took,
                    failed: Some((why, output)),
                };
            }
        };
        let mut repeat = format!(
            "astli test --exact {} --seed {seed} --trace",
            quoted(&case.point.to_string())
        );
        if case.shown != case.point.to_string() {
            repeat.push_str(&format!(" -p {}", quoted(&case.package.package)));
        }
        if self.release {
            repeat.push_str(" --release");
        }
        if !self.args.is_empty() {
            repeat.push_str(" --");
            for arg in &self.args {
                repeat.push(' ');
                repeat.push_str(&quoted(&arg.to_string_lossy()));
            }
        }
        let here = std::env::current_dir().unwrap_or_default();
        let shown_log = log.strip_prefix(&here).unwrap_or(&log);
        term::error(format!("{} failed: {why}", mark::name(&shown)))
            .note(format!(
                "its output is in {}",
                mark::path(shown_log.display())
            ))
            .help(format!("{} repeats it", mark::code(repeat)))
            .report(run);
        shown_output(&output);
        Ran {
            seed,
            took,
            failed: Some((mark::plain(&why), output)),
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
                            let name = match case.random {
                                true => format!("{} seed {}", case.point, ran.seed),
                                false => case.point.to_string(),
                            };
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
                for point in &package.points {
                    suite.case(
                        &point.to_string(),
                        Duration::ZERO,
                        Verdict::Error("its package's check failed"),
                    );
                }
            }
            suites.push(suite);
        }
        let text = junit(&suites);
        std::fs::write(path, text).map_err(|err| Error::io(path, err))
    }
}

/// `count` seeds, each different, drawn at random up to `max`, the largest
/// the simulator takes, and never 0.
fn random_seeds(count: NonZeroU32, max: u32) -> Vec<u32> {
    // Keyed at random for each run, and so a source of random numbers
    // without a dependency for them.
    let state = RandomState::new();
    let mut seeds = Vec::new();
    let mut taken = HashSet::new();
    for at in 0_u64.. {
        if seeds.len() == count.get() as usize {
            break;
        }
        let seed = ((state.hash_one(at) as u32) & max).max(1);
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

/// `text` as a shell reads it back as one word: in single quotes unless it
/// holds only what no shell treats as special. A point's name holds `[`,
/// which zsh takes for a glob.
fn quoted(text: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "_-./=,:+".contains(c);
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
