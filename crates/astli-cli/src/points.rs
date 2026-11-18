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

use astli_index::{Declaration, Declares, FailMark, Summary, SweepMark, SweepValues};
use astli_package::mark;
use astli_preproc::MacroTable;
use astli_sema::{Elaboration, TopError, TopValue};

use crate::analysis::Elaborated;
use crate::cmd;
use crate::discovery::Plan;
use crate::error::{Error, Result};
use crate::render::{Out, plural};
use crate::run::Run;
use crate::session;
use crate::term::{self, Place};
use crate::tops::{self, Points, point_of};

/// Refuses `--tests` where it has no tests to add: with files named, which
/// are no package's.
pub(crate) fn asked(tests: bool, named: bool) -> Result {
    if !(tests && named) {
        return Ok(());
    }
    Err(term::error(format!(
        "{} adds the tests a package marks, and files named are no package's",
        mark::code("--tests")
    ))
    .help(format!(
        "run it in the package without naming files, or name a test's point with {}",
        mark::code("--top 'tb_fifo[Depth=16]'")
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

/// A test a package marks.
pub(crate) struct Marked {
    pub name: String,
    /// The text `astli_should_fail` gives, which a run passes only by
    /// failing with in its output.
    pub should_fail: Option<String>,
}

/// The tests of the package `plan` discovered, by name, sorted, from its
/// files as written: every branch of a conditional, and no macro
/// expanded, so that a test a `[top.NAME]` table's defines declare is found
/// though the package's defaults leave it out. What does not parse is the
/// check's to report, under the defines a test is built with. Reports each
/// `astli_test` that marks nothing it can, each `astli_sweep` on what is
/// no test, and each `astli_should_fail` on what is none or without its
/// text, failing the run if there are any.
pub(crate) fn marked(out: &mut Out, plan: &Plan, run: &Run) -> Result<Vec<Marked>> {
    let own: Vec<_> = (plan.units.iter())
        .filter(|unit| unit.reported)
        .map(|unit| unit.path.clone())
        .collect();
    let outcome = cmd::each(out, &own, run, None, |_, file| {
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

    let mut marked = Vec::new();
    let mut wrong = 0;
    for summary in outcome.values.iter().flatten() {
        wrong += tests(run, summary, &mut marked);
    }
    if wrong > 0 {
        return Err(Error::tally(format!(
            "{} marked no test",
            plural(wrong, "attribute")
        )));
    }
    // A module declared in two branches is one test.
    marked.sort_by(|a, b| a.name.cmp(&b.name));
    marked.dedup_by(|a, b| a.name == b.name);
    Ok(marked)
}

/// The names of `marked`.
pub(crate) fn names(marked: Vec<Marked>) -> Vec<String> {
    marked.into_iter().map(|it| it.name).collect()
}

/// What the names of `astli_test`, `astli_sweep` and `astli_should_fail`
/// start with: a file without it marks none of them.
const MARK: &str = "astli_";

/// Adds the tests `summary` declares to `marked`, reporting each
/// `astli_test` that marks no test, each `astli_sweep` and
/// `astli_should_fail` on a declaration that is none, and each
/// `astli_should_fail` without its text, and counting them: an
/// `astli_test` with a value, which it takes none of, or on what is no
/// module.
fn tests(run: &Run, summary: &Summary, marked: &mut Vec<Marked>) -> usize {
    let mut wrong = 0;
    for declaration in &summary.declarations {
        let Some(test) = &declaration.test else {
            if let Some(fail) = &declaration.should_fail {
                at_fail(
                    fail,
                    term::error(format!(
                        "{} says how a test fails, and {} is no test",
                        mark::code("astli_should_fail"),
                        mark::name(&declaration.name)
                    )),
                    "on no test",
                )
                .help(format!(
                    "mark the module a test with {}",
                    mark::code("(* astli_test *)")
                ))
                .report(run);
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
                .report(run);
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
                let fail = declaration.should_fail.as_ref();
                // Empty, it would be in every output, and pass any failure.
                let text = (fail.and_then(|it| it.text.as_ref())).filter(|it| !it.is_empty());
                let Some(fail) = fail.filter(|_| text.is_none()) else {
                    marked.push(Marked {
                        name: declaration.name.clone(),
                        should_fail: text.cloned(),
                    });
                    continue;
                };
                at_fail(
                    fail,
                    term::error(format!(
                        "{} takes the text a failing run writes",
                        mark::code("astli_should_fail")
                    )),
                    "given no text",
                )
                .help(format!(
                    "name what fails, as in {}",
                    mark::code("(* astli_should_fail = \"tb.dut.u_checker\" *)")
                ))
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
        problem.report(run);
        wrong += 1;
    }
    wrong
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

/// `problem` at the `astli_should_fail` `fail`, with `label`.
fn at_fail(fail: &FailMark, problem: term::Problem, label: &str) -> term::Problem {
    let place = Place::Name(fail.location.at, "astli_should_fail".len());
    (problem.at("invalid-test", &fail.location.path, place)).label(label)
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
    /// The run, which tells what is wrong with a sweep.
    pub run: &'a Run,
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
                .report(self.run);
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
            .report(self.run);
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
        at_sweep(sweep, problem, "swept here").report(self.run);
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
        let refused = |err: TopError| at_sweep(sweep, self.refused(err), "here").report(self.run);
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
                    .report(self.run);
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
                .report(self.run);
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
