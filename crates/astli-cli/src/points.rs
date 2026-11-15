//! The tests a package marks, and the points of each: one for each
//! combination of the values of the parameters it sweeps, each a top of its
//! own.
//!
//! `(* astli_sweep = "1, 2, 16" *)` on a `parameter` declared in a test's
//! body lists its values, each a number in decimal or an enum member's
//! name, and a bare `(* astli_sweep *)` takes every value of an enum, a
//! `bit` or a 1-bit `logic`. No expression is evaluated. The values are
//! read once the design is elaborated, each sweep's in the body the values
//! of those before it give, since its type may depend on them. A point is
//! named for its values in declaration order, `tb_fifo[Depth=16,Mode=Bypass]`.
//!
//! On the test's module, `(* astli_plusargs = "+TEST=burst, +TEST=wrap" *)`
//! lists the plusargs each point runs under, a run per entry from one
//! build, and `(* astli_timeout = 600 *)` bounds each of its runs, in
//! seconds.

use std::time::Duration;

use astli_index::{AttributeMark, Declaration, Declares, Given, Summary, SweepMark, SweepValues};
use astli_package::mark;
use astli_preproc::MacroTable;
use astli_sema::{Elaboration, TopError, TopValue};

use crate::analysis::Elaborated;
use crate::cli::RunArgs;
use crate::cmd;
use crate::discovery::Plan;
use crate::error::{Error, Result};
use crate::render::{Out, plural};
use crate::session;
use crate::term::{self, Place};
use crate::tops::{self, Points, point_of};

/// Refuses `--tests` where it has no tests to add: with `--top`, which
/// says what to elaborate, or with files named, which are no package's.
pub(crate) fn asked(tests: bool, top: &[String], named: bool) -> Result {
    let with = match (tests, top.is_empty(), named) {
        (false, _, _) | (true, true, false) => return Ok(()),
        (true, false, _) => "--top",
        (true, true, true) => "files named",
    };
    Err(term::error(format!(
        "{} adds a package's tests to what is elaborated, and {} says what that is",
        mark::code("--tests"),
        mark::code(with)
    ))
    .into())
}

/// How to choose the value of the parameter `name` a sweep of `test` lists,
/// rather than setting it to `value` with `-G`: by naming the point.
pub(crate) fn swept(test: &str, name: &str, value: &str) -> String {
    format!(
        "name the point, as in {}",
        mark::code(format!("--top '{test}[{name}={value}]'"))
    )
}

/// A test a package marks, and what its module's attributes say of its
/// runs.
#[derive(Debug, Clone)]
pub(crate) struct Marked {
    pub name: String,
    /// The entries of its `astli_plusargs`, in order, each one or more
    /// plusargs separated by spaces, as written; none without one.
    pub plusargs: Vec<String>,
    /// Its `astli_timeout`.
    pub timeout: Option<Duration>,
}

/// The names of `tests`, for what elaborates them alone.
pub(crate) fn names(tests: Vec<Marked>) -> Vec<String> {
    tests.into_iter().map(|it| it.name).collect()
}

/// The tests of the package `plan` discovered, sorted by name, from its
/// files as written: every branch of a conditional, and no macro
/// expanded, so that a test a `[top.NAME]` table's defines declare is found
/// though the package's defaults leave it out. What does not parse is the
/// check's to report, under the defines a test is built with. Reports each
/// `astli_test` that marks nothing it can, each `astli_sweep`,
/// `astli_plusargs` and `astli_timeout` on what is no test, and each of the
/// last two given what it cannot take, failing the run if there are any.
pub(crate) fn marked(out: &mut Out, plan: &Plan, run: &RunArgs) -> Result<Vec<Marked>> {
    let own: Vec<_> = (plan.units.iter())
        .filter(|unit| unit.reported)
        .map(|unit| unit.path.clone())
        .collect();
    let outcome = cmd::each(out, &own, run.jobs, None, |_, file| {
        // A file that does not name the attributes marks nothing, and is
        // not parsed again.
        let text = std::fs::read(file).map_err(|err| Error::io(file, err))?;
        if !(text.windows(MARK.len())).any(|it| it == MARK.as_bytes()) {
            return Ok(None);
        }
        let opened = session::open(file, &astli_preproc::Build::new())?;
        let parsed = astli_parse::parse(&opened.session, opened.file, MacroTable::new());
        Ok(Some(Summary::new(&opened.session, opened.file, &parsed)))
    })?;
    if outcome.failed > 0 {
        return Err(outcome.finish().expect_err("some file failed"));
    }

    let mut tests = Vec::new();
    let mut wrong = 0;
    for summary in outcome.values.iter().flatten() {
        wrong += marked_in(summary, &mut tests);
    }
    if wrong > 0 {
        return Err(Error::tally(format!(
            "{} of tests were refused",
            plural(wrong, "attribute")
        )));
    }
    // A module declared in two branches is one test, as the first says.
    tests.sort_by(|a, b| a.name.cmp(&b.name));
    tests.dedup_by(|a, b| a.name == b.name);
    Ok(tests)
}

/// What the names of `astli_test` and `astli_sweep` start with: a file
/// without it marks neither.
const MARK: &str = "astli_";

/// Adds the tests `summary` declares to `tests`, reporting each
/// `astli_test` that marks no test, each attribute of a test on a
/// declaration that is none, and each `astli_plusargs` or `astli_timeout`
/// given what it cannot take, and counting them: an `astli_test` with a
/// value, which it takes none of, or on what is no module.
fn marked_in(summary: &Summary, tests: &mut Vec<Marked>) -> usize {
    let mut wrong = 0;
    for declaration in &summary.declarations {
        let Some(test) = &declaration.test else {
            let on_module = [
                (&declaration.plusargs, PLUSARGS),
                (&declaration.timeout, TIMEOUT),
            ];
            for (mark, name) in on_module {
                let Some(mark) = mark else {
                    continue;
                };
                at_mark(
                    mark,
                    name,
                    term::error(format!(
                        "{} is a test's, and {} is no test",
                        mark::code(name),
                        mark::name(&declaration.name)
                    )),
                    "on no test",
                )
                .help(format!(
                    "mark the module a test with {}",
                    mark::code(format!("(* astli_test, {name} = ... *)"))
                ))
                .report();
                wrong += 1;
            }
            for sweep in &declaration.sweeps {
                at_sweep(
                    sweep,
                    term::error(format!(
                        "{} sweeps a test's parameter, and {} is no test",
                        mark::code("astli_sweep"),
                        mark::name(&declaration.name)
                    )),
                    "on no test",
                )
                .help(format!(
                    "mark the module a test with {}",
                    mark::code("(* astli_test *)")
                ))
                .report();
                wrong += 1;
            }
            continue;
        };
        let place = Place::Name(test.location.at, "astli_test".len());
        let at = |problem: term::Problem, label: &str| {
            problem
                .at("invalid-test", &test.location.path, place)
                .label(label)
        };
        let problem = match (declaration.declares, test.valued) {
            (Declares::Module, false) => {
                // `None` for one refused, as each says.
                let plusargs = match &declaration.plusargs {
                    Some(mark) => plusargs(mark),
                    None => Some(Vec::new()),
                };
                let timeout = match &declaration.timeout {
                    Some(mark) => timeout(mark).map(Some),
                    None => Some(None),
                };
                match (plusargs, timeout) {
                    (Some(plusargs), Some(timeout)) => tests.push(Marked {
                        name: declaration.name.clone(),
                        plusargs,
                        timeout,
                    }),
                    (plusargs, timeout) => {
                        wrong += usize::from(plusargs.is_none()) + usize::from(timeout.is_none());
                    }
                }
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
    wrong
}

/// The attribute that lists the plusargs a test runs under.
const PLUSARGS: &str = "astli_plusargs";

/// The attribute that bounds each run of a test.
const TIMEOUT: &str = "astli_timeout";

/// The entries `mark`, an `astli_plusargs`, lists, or `None`, having said
/// why, if it lists none it can: each must hold plusargs, each starting
/// with `+`, and be listed once.
fn plusargs(mark: &AttributeMark) -> Option<Vec<String>> {
    let refused = |problem: term::Problem| {
        at_mark(mark, PLUSARGS, problem, "here")
            .help(format!(
                "list them separated by commas, as in {}",
                mark::code("(* astli_plusargs = \"+TEST=burst, +TEST=wrap\" *)")
            ))
            .report();
        None
    };
    let Given::String(text) = &mark.given else {
        return refused(term::error(format!(
            "{} takes its entries as a string",
            mark::code(PLUSARGS)
        )));
    };
    let mut entries: Vec<String> = Vec::new();
    for entry in text.split(',') {
        // Spelled one way, so that the name a filter matches is the same
        // however the attribute spaces it.
        let words: Vec<&str> = entry.split_whitespace().collect();
        let entry = words.join(" ");
        if let Some(word) = words.iter().find(|it| !it.starts_with('+')) {
            return refused(term::error(format!(
                "{} lists {}, which is no plusarg",
                mark::code(PLUSARGS),
                mark::code(word)
            )));
        }
        if words.is_empty() {
            return refused(term::error(format!(
                "{} lists an empty entry",
                mark::code(PLUSARGS)
            )));
        }
        if entries.contains(&entry) {
            return refused(term::error(format!(
                "{} lists {} twice",
                mark::code(PLUSARGS),
                mark::code(&entry)
            )));
        }
        entries.push(entry);
    }
    Some(entries)
}

/// How long `mark`, an `astli_timeout`, lets a run take, or `None`, having
/// said why, if it is no whole number of seconds above none.
fn timeout(mark: &AttributeMark) -> Option<Duration> {
    let seconds = match &mark.given {
        Given::Number(text) => text.replace('_', "").parse::<u64>().ok(),
        _ => None,
    };
    if let Some(seconds) = seconds.filter(|it| *it > 0) {
        return Some(Duration::from_secs(seconds));
    }
    at_mark(
        mark,
        TIMEOUT,
        term::error(format!(
            "{} takes a whole number of seconds, more than none",
            mark::code(TIMEOUT)
        )),
        "here",
    )
    .help(format!(
        "write it as in {}",
        mark::code("(* astli_timeout = 600 *)")
    ))
    .report();
    None
}

/// `problem` at `mark`, the attribute `name`, with `label`.
fn at_mark(mark: &AttributeMark, name: &str, problem: term::Problem, label: &str) -> term::Problem {
    let place = Place::Name(mark.location.at, name.len());
    (problem.at("invalid-test", &mark.location.path, place)).label(label)
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

/// `problem` at the sweep `sweep`, with `label`.
fn at_sweep(sweep: &SweepMark, problem: term::Problem, label: &str) -> term::Problem {
    let place = Place::Name(sweep.location.at, "astli_sweep".len());
    (problem.at("invalid-sweep", &sweep.location.path, place)).label(label)
}

/// A top that stands for the points of a test.
pub(crate) struct Test<'a> {
    /// What the top is called: the test's name, or a point's.
    pub name: &'a str,
    /// The test's module.
    pub module: &'a str,
    /// Its declaration, which holds its sweeps.
    pub declared: Option<&'a Declaration>,
    /// What its table and `-G` set its parameters to, by name.
    pub params: &'a [(String, String)],
    /// The parameters `-G` set, by name.
    pub flagged: &'a [String],
}

impl Test<'_> {
    /// The points the top stands for, as `points` says, each elaborated.
    /// What is wrong with a sweep is reported at it, and counted in
    /// `wrong`, and leaves the test no point.
    pub(crate) fn points(
        &self,
        elaboration: &Elaboration,
        points: Points,
        wrong: &mut usize,
    ) -> Result<Vec<Elaborated>> {
        let Some(declared) = self.declared else {
            return Err(tops::refused(TopError::NoTop, self.module).into());
        };
        if declared.test.is_none() {
            return Err(term::error(format!(
                "{} names a point of {}, which is no test",
                mark::name(self.name),
                mark::name(self.module)
            ))
            .help(format!(
                "a test is a module marked {}",
                mark::code("(* astli_test *)")
            ))
            .into());
        }
        let wrong_before = *wrong;
        let mut sweeps = Vec::new();
        for sweep in &declared.sweeps {
            match self.swept(sweep) {
                Some(name) => sweeps.push((sweep, name)),
                None => *wrong += 1,
            }
        }

        // Each sweep's values in the body of each combination of those
        // before it.
        let mut combinations: Vec<Vec<(String, TopValue)>> = vec![Vec::new()];
        for (sweep, name) in sweeps {
            let mut next = Vec::new();
            for before in &combinations {
                let body = tops::body(elaboration, self.module, self.params, before)?;
                match self.values(elaboration, body, sweep, name) {
                    Some(values) => next.extend(values.into_iter().map(|value| {
                        let mut values = before.clone();
                        values.push((name.to_string(), value));
                        values
                    })),
                    None => {
                        *wrong += 1;
                        next = combinations.clone();
                        break;
                    }
                }
            }
            combinations = next;
        }
        if *wrong > wrong_before {
            return Ok(Vec::new());
        }

        let test = point_of(self.name).unwrap_or(self.name);
        let mut elaborated = Vec::new();
        for values in combinations {
            let name = match values.is_empty() {
                true => test.to_string(),
                false => {
                    let values: Vec<String> = (values.iter())
                        .map(|(name, value)| format!("{name}={}", value.short()))
                        .collect();
                    format!("{test}[{}]", values.join(","))
                }
            };
            if points == Points::Named && name != self.name {
                continue;
            }
            elaborated.push(Elaborated {
                name,
                module: self.module.to_string(),
                body: tops::body(elaboration, self.module, self.params, &values)?,
                params: self.params.to_vec(),
                values,
            });
        }
        if points == Points::Named && elaborated.is_empty() {
            return Err(term::error(format!(
                "{} names no point of {}",
                mark::name(self.name),
                mark::name(test)
            ))
            .help(format!(
                "{} lists every point",
                mark::code(format!("astli test --list {test}"))
            ))
            .into());
        }
        Ok(elaborated)
    }

    /// The name of the parameter `sweep` sweeps, or `None`, having said
    /// why, if it sweeps none.
    fn swept<'a>(&self, sweep: &'a SweepMark) -> Option<&'a str> {
        let name = match sweep.parameters.as_slice() {
            [name] => name.as_str(),
            names => {
                at_sweep(
                    sweep,
                    term::error(format!(
                        "{} sweeps one parameter, and its declaration declares {}",
                        mark::code("astli_sweep"),
                        names.len()
                    )),
                    "on several",
                )
                .help("declare each parameter a sweep lists the values of on its own")
                .report();
                return None;
            }
        };
        if sweep.values == SweepValues::NotAString {
            at_sweep(
                sweep,
                term::error(format!(
                    "{} takes its values as a string",
                    mark::code("astli_sweep")
                )),
                "given what is no string",
            )
            .help(format!(
                "write them as in {}",
                mark::code("(* astli_sweep = \"1, 2, 16\" *)")
            ))
            .report();
            return None;
        }
        if !self.params.iter().any(|(it, _)| it == name) {
            return Some(name);
        }
        let test = point_of(self.name).unwrap_or(self.name);
        let problem = match self.flagged.iter().any(|it| it == name) {
            true => term::error(format!(
                "{} sets {}, whose values a sweep lists",
                mark::code(format!("-G {name}")),
                mark::name(name)
            ))
            .help(format!(
                "name a point, as in {}",
                mark::code(format!("--top '{test}[{name}=VALUE]'"))
            )),
            false => term::error(format!(
                "{} sets {}, whose values a sweep lists",
                mark::code(format!("[top.{test}] params")),
                mark::name(name)
            ))
            .help("leave it to the sweep, which sets it at each point"),
        };
        at_sweep(sweep, problem, "swept here").report();
        None
    }

    /// The values `sweep` gives the parameter `name`, read in `body`, or
    /// `None`, having said why, if it gives none it can.
    fn values(
        &self,
        elaboration: &Elaboration,
        body: astli_sema::BodyId,
        sweep: &SweepMark,
        name: &str,
    ) -> Option<Vec<TopValue>> {
        let refused = |err: TopError| at_sweep(sweep, self.refused(err), "here").report();
        let values = match &sweep.values {
            SweepValues::Every => elaboration.top_values(body, name).map_err(refused).ok()?,
            SweepValues::Listed(text) => {
                let texts: Vec<&str> = text.split(',').map(str::trim).collect();
                if texts.iter().any(|it| it.is_empty()) {
                    at_sweep(
                        sweep,
                        term::error(format!(
                            "{} lists an empty value",
                            mark::code("astli_sweep")
                        )),
                        "here",
                    )
                    .help(format!(
                        "list its values separated by commas, as in {}",
                        mark::code("(* astli_sweep = \"1, 2, 16\" *)")
                    ))
                    .report();
                    return None;
                }
                (texts.into_iter())
                    .map(|text| elaboration.top_value(body, name, text))
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(refused)
                    .ok()?
            }
            SweepValues::NotAString => unreachable!("refused before"),
        };
        for (at, value) in values.iter().enumerate() {
            if values[..at].contains(value) {
                at_sweep(
                    sweep,
                    term::error(format!(
                        "{} lists {} twice",
                        mark::code("astli_sweep"),
                        mark::code(value.short())
                    )),
                    "here",
                )
                .report();
                return None;
            }
        }
        Some(values)
    }

    /// Why a sweep cannot set the test's parameter, as `err` says.
    fn refused(&self, err: TopError) -> term::Problem {
        let of = |name: &str| format!("{} of {}", mark::name(name), mark::name(self.module));
        match err {
            TopError::Local(name) => term::error(format!(
                "{} sweeps a {}, and {} is a {}",
                mark::code("astli_sweep"),
                mark::code("parameter"),
                mark::name(&name),
                mark::code("localparam")
            ))
            .help(format!("declare it a {}", mark::code("parameter"))),
            TopError::Listed(name) => term::error(format!(
                "{} is local, since {} has a parameter port list",
                of(&name),
                mark::name(self.module)
            ))
            .help(format!(
                "remove the {} list: a test that sweeps declares its parameters in its body",
                mark::code("#( ... )")
            )),
            TopError::NoValues(name) => term::error(format!(
                "{} has a type with more values than a sweep takes without a list",
                of(&name)
            ))
            .help(format!(
                "list the values, as in {}",
                mark::code("(* astli_sweep = \"1, 2, 16\" *)")
            )),
            TopError::NotAValue(name, text) => term::error(format!(
                "{} cannot be {}: a sweep lists numbers in decimal and enum members' names",
                of(&name),
                mark::code(&text)
            )),
            err => tops::refused(err, self.module),
        }
    }
}
