//! `astli test` subcommand.
//!
//! A test is a module of the package the command acts on marked
//! `(* astli_test *)`, in any of its files, dev sources included: a
//! dependency's tests are its own CI's, and so are those of a sibling the
//! package requires by `path`. Each file is read as written, every branch
//! of a conditional, since a `[top.NAME]` table's defines may decide
//! whether a test is declared at all.
//!
//! Each test is one point for now, named after the test. A filter selects
//! the points whose name holds it, or with `--exact` the one it names, and
//! one matching no point of any package is an error, so that a misspelt
//! name fails CI rather than passing it. `--list` writes the points
//! selected.
//!
//! Otherwise each point is a top as `build` builds one, under the
//! `[top.NAME]` table named as its test if there is one: checked once for
//! each set of defines the tables give, every design before any build,
//! then built and run in parallel. A run starts in `run/<seed>/` in the
//! point's build directory, told the root of its package, and passes if
//! the simulation exits with status 0. Its output is kept as a log there,
//! and shown for a failure, with the command that repeats it.

use std::fmt;
use std::fs::File;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use astli_index::{Declares, Summary};
use astli_package::mark;
use astli_preproc::MacroTable;
use usage_rs::{Args, RunWith};

use crate::cli::{BuildArgs, FeatureArgs, Members, RunArgs, Sources};
use crate::cmd::build::{Builder, Listed, Sim, listed_tops};
use crate::cmd::{self, Ctx};
use crate::discovery::Plan;
use crate::error::{Error, Result};
use crate::render::{self, Out, plural};
use crate::session;
use crate::term::{self, Place, status};
use crate::tops::{Top, grouped};

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
    /// The points of the test `test`. Nothing is swept yet, so it is one.
    fn of(test: &str) -> Vec<Point> {
        vec![Point {
            test: test.to_string(),
            values: Vec::new(),
        }]
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
    /// What builds the points; `None` under `--list`.
    built: Option<Built>,
}

/// What builds a package's points, and for each, in order, what its build
/// reads and how long a run may take.
struct Built {
    builder: Builder,
    lists: Vec<(Listed, Option<Duration>)>,
}

/// One point to build and run.
struct Case<'a> {
    /// The point as the run shows it: with its package's name before it
    /// where the run reads several.
    shown: String,
    point: &'a Point,
    package: &'a Selected,
    builder: &'a Builder,
    listed: &'a Listed,
    timeout: Option<Duration>,
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
        let mut selected: Vec<Selected> = Vec::new();
        let checked = cmd::members(&self.members, false, out, |out| {
            selected.push(self.selected(out, run)?);
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

        let mut cases = Vec::new();
        for package in &selected {
            let Some(Built { builder, lists }) = &package.built else {
                continue;
            };
            for (point, (listed, timeout)) in package.points.iter().zip(lists) {
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
                });
            }
        }
        if cases.is_empty() {
            if checked.is_ok() {
                term::note("no module of the package is marked `astli_test`").report();
            }
            return checked;
        }
        out.flush()?;
        let failed = self.tested(&cases, run, several);
        let tally = (!failed.is_empty()).then(|| {
            let names: Vec<String> = failed.iter().map(mark::name).collect();
            format!(
                "{} of {} failed: {}",
                failed.len(),
                plural(cases.len(), "test"),
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
    /// checked.
    fn selected(&self, out: &mut Out, run: &RunArgs) -> Result<Selected> {
        let build = BuildArgs {
            incdir: self.incdir.clone(),
            define: self.define.clone(),
            ..BuildArgs::default()
        };
        let plan = Plan::new(&Sources::default(), &self.features, &build, None, run)?;
        let Some((package, _)) = plan.packages() else {
            return Err(Error::failed("a test is a package's"));
        };
        let points: Vec<Point> = (points(out, &plan, run)?.into_iter())
            .filter(|point| self.selects(point))
            .collect();
        let mut selected = Selected {
            package: (package.name.clone()).unwrap_or_else(|| package.root.display().to_string()),
            root: package.root.clone(),
            points,
            built: None,
        };
        if self.list || selected.points.is_empty() {
            return Ok(selected);
        }

        let builder = Builder::new(&plan, self.release, self.trace, self.sim)?;
        let tops = (selected.points.iter())
            .map(|point| {
                let mut top = Top::of(package, &point.test);
                top.name = point.to_string();
                top
            })
            .collect();
        let designs = grouped(tops).into_iter().map(|tops| plan.for_tops(tops));
        let mut lists = listed_tops(out, designs.collect(), run)?;
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

    /// Builds every case, and unless `--no-run` runs every one that built,
    /// whatever failed before it, and hands back the cases that failed, as
    /// shown. Builds share the run's jobs with each Verilator build, and runs
    /// take one each.
    fn tested(&self, cases: &[Case], run: &RunArgs, several: bool) -> Vec<String> {
        let started = Instant::now();
        let jobs = match run.jobs {
            0 => std::thread::available_parallelism().map_or(1, |it| it.get()),
            jobs => jobs,
        };
        let builds = jobs.min(cases.len());
        let built = parallel(cases, builds, |case| {
            match case.builder.build(case.listed, (jobs / builds).max(1)) {
                Ok(executable) => Some(executable),
                Err(err) => {
                    let mut problem = err.problem();
                    problem.message = format!("{}: {}", case.shown, problem.message);
                    problem.report();
                    None
                }
            }
        });
        let mut failed: Vec<String> = (cases.iter().zip(&built))
            .filter(|(_, built)| built.is_none())
            .map(|(case, _)| case.shown.clone())
            .collect();
        if self.no_run {
            if failed.is_empty() {
                status!(
                    "Finished",
                    "building {} in {}",
                    plural(cases.len(), "test"),
                    render::duration(started.elapsed())
                );
            }
            return failed;
        }

        let ran: Vec<(&Case, &PathBuf)> = (cases.iter().zip(&built))
            .filter_map(|(case, built)| Some((case, built.as_ref()?)))
            .collect();
        let passed = parallel(&ran, jobs.min(ran.len()).max(1), |(case, executable)| {
            self.ran(case, executable, several)
        });
        failed.extend(
            (ran.iter().zip(passed))
                .filter(|(_, passed)| !passed)
                .map(|((case, _), _)| case.shown.clone()),
        );
        if failed.is_empty() {
            status!(
                "Finished",
                "{} in {}, all passed",
                plural(cases.len(), "test"),
                render::duration(started.elapsed())
            );
        }
        failed
    }

    /// Runs `case`, built as `executable`, and says how it went: a status if
    /// it passed, an error with the command that repeats it if not, and its
    /// output if it failed or `--nocapture` asks. Hands back whether it
    /// passed.
    fn ran(&self, case: &Case, executable: &Path, several: bool) -> bool {
        let seed = self.seed.unwrap_or_else(|| case.point.seed());
        let dir = (executable.parent().unwrap_or(Path::new(".")))
            .join("run")
            .join(seed.to_string());
        let log = dir.join("output.log");
        let started = Instant::now();
        let slow = || {
            status!(
                "Running",
                "{}, for over {}s",
                mark::name(&case.shown),
                SLOW.as_secs()
            )
        };
        let ended = simulated(
            executable,
            &dir,
            &log,
            &case.package.root,
            seed,
            case.timeout,
            slow,
        );
        let took = render::duration(started.elapsed());
        let output = std::fs::read(&log).unwrap_or_default();
        let why = match ended {
            Ok(Ended::Status(status)) if status.success() => {
                status!("Passed", "{} in {took}", mark::name(&case.shown));
                if self.nocapture {
                    shown_output(&output);
                }
                return true;
            }
            Ok(Ended::Status(status)) => format!("{}, after {took}", ended_by(status)),
            Ok(Ended::TimedOut(after)) => {
                format!("it ran past its timeout of {}s", after.as_secs())
            }
            Err(err) => {
                let mut problem = err.problem();
                problem.message = format!("{}: {}", case.shown, problem.message);
                problem.report();
                return false;
            }
        };
        let mut repeat = format!(
            "astli test --exact {} --seed {seed} --trace",
            quoted(&case.point.to_string())
        );
        if several {
            repeat.push_str(&format!(" -p {}", quoted(&case.package.package)));
        }
        if self.release {
            repeat.push_str(" --release");
        }
        let here = std::env::current_dir().unwrap_or_default();
        let shown_log = log.strip_prefix(&here).unwrap_or(&log);
        term::error(format!("{} failed: {why}", mark::name(&case.shown)))
            .note(format!(
                "its output is in {}",
                mark::path(shown_log.display())
            ))
            .help(format!("{} repeats it", mark::code(repeat)))
            .report();
        shown_output(&output);
        false
    }
}

/// How a run ended.
enum Ended {
    Status(ExitStatus),
    /// It ran for so long and was killed.
    TimedOut(Duration),
}

/// Runs the simulation `executable` in `dir`, emptied first, with its
/// output in `log`, told the package's `root` and the `seed`, and killed
/// past `timeout`; `slow` is called once it has run for longer than
/// [`SLOW`].
fn simulated(
    executable: &Path,
    dir: &Path,
    log: &Path,
    root: &Path,
    seed: u32,
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
        .arg(format!("+astli_root={}", root.display()))
        .arg(format!("+verilator+seed+{seed}"))
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

/// The points of the package `plan` discovered, sorted, from its files
/// as written: every branch of a conditional, and no macro expanded, so
/// that a test a `[top.NAME]` table's defines declare is found though the
/// package's defaults leave it out. What does not parse is the check's to
/// report, under the defines a test is built with. Reports each
/// `astli_test` that marks nothing it can, failing the run if there are
/// any.
fn points(out: &mut Out, plan: &Plan, run: &RunArgs) -> Result<Vec<Point>> {
    let own: Vec<_> = (plan.units.iter())
        .filter(|unit| unit.reported)
        .map(|unit| unit.path.clone())
        .collect();
    let outcome = cmd::each(out, &own, run.jobs, None, |_, file| {
        let opened = session::open(file, &astli_preproc::Build::new())?;
        let parsed = astli_parse::parse(&opened.session, opened.file, MacroTable::new());
        Ok(Summary::new(&opened.session, opened.file, &parsed))
    })?;
    if outcome.failed > 0 {
        return Err(outcome.finish().expect_err("some file failed"));
    }

    let mut points = Vec::new();
    let mut wrong = 0;
    for summary in &outcome.values {
        let (tests, problems) = tests(summary);
        wrong += problems;
        points.extend(tests.into_iter().flat_map(Point::of));
    }
    if wrong > 0 {
        return Err(Error::tally(format!(
            "{} marked no test",
            plural(wrong, "attribute")
        )));
    }
    // A module declared in two branches is one test.
    points.sort();
    points.dedup();
    Ok(points)
}

/// The tests `summary` declares, by name, reporting each `astli_test` that
/// marks no test and counting them: one with a value, which it takes none
/// of, or one on what is no module.
fn tests(summary: &Summary) -> (Vec<&str>, usize) {
    let mut tests = Vec::new();
    let mut wrong = 0;
    for declaration in &summary.declarations {
        let Some(mark) = &declaration.test else {
            continue;
        };
        let place = Place::Name(mark.location.at, "astli_test".len());
        let at = |problem: term::Problem, label: &str| {
            problem
                .at("invalid-test", &mark.location.path, place)
                .label(label)
        };
        let problem = match (declaration.declares, mark.valued) {
            (Declares::Module, false) => {
                tests.push(declaration.name.as_str());
                continue;
            }
            (Declares::Module, true) => at(
                term::error(format!("{} takes no value", mark::code("astli_test"))),
                "given one here",
            )
            .help(format!("write {}", mark::code("(* astli_test *)"))),
            (declares, _) => at(
                term::error(format!(
                    "{} marks a module, and {} is {}",
                    mark::code("astli_test"),
                    mark::name(&declaration.name),
                    article(declares),
                )),
                "marks no module",
            ),
        };
        problem.report();
        wrong += 1;
    }
    (tests, wrong)
}

/// What a declaration of `declares` declares, with its article.
fn article(declares: Declares) -> &'static str {
    match declares {
        Declares::Module => "a module",
        Declares::Interface => "an interface",
        Declares::Program => "a program",
        Declares::Package => "a package",
        Declares::Class => "a class",
        Declares::Checker => "a checker",
        Declares::Primitive => "a primitive",
    }
}
