//! Which tops a run elaborates from, what they define and what their
//! parameters are set to: those `--top` names, each a `[top.NAME]` of the
//! root or else a module, or a point of a test, or without any, the root's
//! default, or for `check` and `lint` every table and every point of each
//! test, one run per set of defines.

use std::collections::BTreeMap;

use astli_package::{RootTables, mark};
use astli_sema::{BodyId, Elaboration, TopError, TopSetting, TopValue};

use crate::analysis::{Analysis, Elaborated};
use crate::discovery::Plan;
use crate::error::Result;
use crate::term;

/// One top a run elaborates from: a `[top.NAME]` table, or a module no
/// table names, or a test, or a point of one.
#[derive(Clone)]
pub struct Top {
    /// What it is called: the table's name, or the module's.
    pub name: String,
    /// What is elaborated from.
    pub module: String,
    /// What its parameters are set to, by name, each value as `-G` takes
    /// it.
    pub params: Vec<(String, String)>,
    /// What its table defines, each macro's body by name; `None` for one
    /// defined as `-D NAME` defines it.
    pub defines: BTreeMap<String, Option<String>>,
    /// Which of the points its test's sweeps give it stands for.
    pub points: Points,
}

/// Which of the points a test's sweeps give a top stands for, once the
/// design is elaborated and the sweeps' values are known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Points {
    /// None: the module alone, its parameters at their defaults.
    Alone,
    /// Every one, a top each: a test, as `test` builds it.
    Every,
    /// The one the top's name names, `tb_fifo[Depth=16,Mode=Bypass]`.
    Named,
}

impl Top {
    /// The top `name` names among a package's `tables`: the table of that
    /// name, or else the module, which then needs nothing defined. A point's name
    /// names its test's table or module, and the point.
    pub fn of(tables: &RootTables, name: &str) -> Top {
        let mut top = Top::module(name);
        let base = point_of(name).unwrap_or(name);
        if let Some(table) = tables.tops.get(base) {
            top.module = table.module(base).to_string();
            top.params = table.params.clone().into_iter().collect();
            top.defines = table.defines.clone();
        }
        top
    }

    /// The top `name` names where no table does: the module, or the point
    /// of the test it names.
    fn module(name: &str) -> Top {
        let (module, points) = match point_of(name) {
            Some(test) => (test, Points::Named),
            None => (name, Points::Alone),
        };
        Top {
            name: name.to_string(),
            module: module.to_string(),
            params: Vec::new(),
            defines: BTreeMap::new(),
            points,
        }
    }

    /// The test `name` names, every point of it a top: its table, or else
    /// the module.
    pub fn test(tables: &RootTables, name: &str) -> Top {
        Top {
            points: Points::Every,
            ..Top::of(tables, name)
        }
    }
}

/// The test whose point `name` names, if it names one: what is before its
/// `[`, where it ends in `]`.
pub fn point_of(name: &str) -> Option<&str> {
    let (test, rest) = name.split_once('[')?;
    (rest.ends_with(']') && !test.is_empty()).then_some(test)
}

/// The tops of a run.
#[derive(Default, Clone)]
pub struct Chosen {
    /// What is elaborated from, in the order named.
    pub tops: Vec<Top>,
    /// What the tables chosen define, each once, as `-D` takes it.
    pub defines: Vec<String>,
}

impl Chosen {
    /// The tops `asked` names of the files `plan` reads, or without any the
    /// package's `default-top`, else its only table, else none if it has
    /// no table. Files named are no package's, so each top they have is a
    /// module.
    pub fn of(plan: &Plan, asked: &[String]) -> Result<Chosen> {
        let Some((root, _)) = plan.packages() else {
            let tops = asked.iter().map(|name| Top::module(name)).collect();
            return Ok(Chosen::joined(tops).expect("a module defines nothing"));
        };
        let tables = &root.tables;
        let defaulted: Vec<String>;
        let asked = match (asked, &tables.default_top) {
            ([], Some(top)) => std::slice::from_ref(top),
            ([], None) => {
                // Of several tables, a run would be guessing.
                defaulted = match tables.tops.len() {
                    0 | 1 => tables.tops.keys().cloned().collect(),
                    _ => return Err(several(tables)),
                };
                &defaulted
            }
            (asked, _) => asked,
        };
        Chosen::joined(asked.iter().map(|name| Top::of(tables, name)).collect())
    }

    /// The tops of each design a command checks, each to be built from a
    /// plan [`Plan::for_tops`] makes: those `asked` names, if any, and
    /// every point of each of `tests` besides, or without any every table
    /// of the root's and every point of each test, one design for each set
    /// of defines they give; with neither, one design of no top, which
    /// builds every file at its defaults.
    pub fn designs(plan: &Plan, asked: &[String], tests: &[String]) -> Result<Vec<Chosen>> {
        let Some((root, _)) = plan.packages() else {
            return Ok(vec![Chosen::of(plan, asked)?]);
        };
        let tables = &root.tables;
        if asked.is_empty() {
            return Ok(match tables.tops.is_empty() && tests.is_empty() {
                true => vec![Chosen::default()],
                false => Chosen::every(tables, tests),
            });
        }
        // Tops named are one design, which refuses a macro they define two
        // ways, though the tests come beside them.
        let named = Chosen::of(plan, asked)?;
        Ok(match tests.is_empty() {
            true => vec![named],
            false => Chosen::with_tests(tables, &named.tops, tests),
        })
    }

    /// Every top of `tables` and every point of each of `tests`, as
    /// [`grouped`] groups them; none if it has neither. A top of a test's
    /// name is that test's.
    fn every(tables: &RootTables, tests: &[String]) -> Vec<Chosen> {
        let tops = (tables.tops.keys())
            .filter(|name| !tests.contains(name))
            .map(|name| Top::of(tables, name));
        let tests = tests.iter().map(|name| Top::test(tables, name));
        grouped(tops.chain(tests).collect())
    }

    /// The tops `named` and every point of each of `tests` of `tables`, as
    /// [`grouped`] groups them. A test named, or a point of one, is left to
    /// its test's points.
    fn with_tests(tables: &RootTables, named: &[Top], tests: &[String]) -> Vec<Chosen> {
        let named = (named.iter())
            .filter(|top| {
                !tests
                    .iter()
                    .any(|test| test == point_of(&top.name).unwrap_or(&top.name))
            })
            .cloned();
        let tests = tests.iter().map(|name| Top::test(tables, name));
        grouped(named.chain(tests).collect())
    }

    /// `tops`, built together: each defines its macros for every file, so
    /// one macro defined two ways is an error.
    pub fn joined(tops: Vec<Top>) -> Result<Chosen> {
        let mut chosen = Chosen::default();
        let mut defined: Vec<(&str, &str, &str)> = Vec::new();
        for top in &tops {
            for (macro_name, body) in &top.defines {
                // `NAME` alone defines it as `1`, as `-D NAME` does.
                let text = body.as_deref().unwrap_or("1");
                match defined.iter().find(|(it, _, _)| it == macro_name) {
                    Some((_, _, before)) if *before == text => {}
                    Some((_, by, before)) => {
                        return Err(term::error(format!(
                            "{} is defined as {} by {} and as {} by {}",
                            mark::name(macro_name),
                            mark::code(before),
                            mark::code(format!("top.{by}")),
                            mark::code(text),
                            mark::code(format!("top.{}", top.name))
                        ))
                        .help("a define is the same for every file, so the two cannot be built together")
                        .into());
                    }
                    None => {
                        defined.push((macro_name, &top.name, text));
                        chosen.defines.push(match body {
                            Some(body) => format!("{macro_name}={body}"),
                            None => macro_name.clone(),
                        });
                    }
                }
            }
        }
        chosen.tops = tops;
        Ok(chosen)
    }

    /// Refuses a table chosen whose name `declared` says is also a module's
    /// other than the one it builds, since which of the two `--top` means is
    /// a guess.
    pub fn unambiguous(&self, declared: impl Fn(&str) -> bool) -> Result {
        let Some(Top { name, module, .. }) =
            (self.tops.iter()).find(|top| top.module != top.name && declared(&top.name))
        else {
            return Ok(());
        };
        Err(term::error(format!(
            "{} is a module, and the name of {}, which builds {}",
            mark::name(name),
            mark::code(format!("top.{name}")),
            mark::name(module)
        ))
        .help(format!(
            "rename {}, so that {} means one of them",
            mark::code(format!("top.{name}")),
            mark::code(format!("--top {name}"))
        ))
        .into())
    }
}

/// Each parameter `flags` set as `-G` takes them, by name, with the value it
/// is set to.
fn parsed(flags: &[String]) -> Result<Vec<(String, String)>> {
    (flags.iter())
        .map(|flag| match flag.split_once('=') {
            Some((name, value)) => Ok((name.to_string(), value.to_string())),
            None => Err(term::error(format!(
                "{} sets no value",
                mark::code(format!("-G {flag}"))
            ))
            .help(format!(
                "write it as {}",
                mark::code(format!("-G {flag}=VALUE"))
            ))
            .into()),
        })
        .collect()
}

/// The parameters `-G` sets, each in every top of a run that declares it,
/// over what its table sets, and whether a top has declared each.
pub struct Flags {
    flags: Vec<(String, String)>,
    declared: Vec<bool>,
    /// Whether the run chose its tops, rather than building every table
    /// or every test that declares a parameter: it is then one design, so
    /// a parameter none of its tops declares is refused once it is
    /// elaborated.
    strict: bool,
    /// How to choose the value of the parameter a sweep of a test lists,
    /// rather than setting it with `-G`: of the test, the parameter and the
    /// value.
    instead: fn(&str, &str, &str) -> String,
}

impl Flags {
    /// The parameters `flags` set, as `-G` takes them, for a run of
    /// `designs`, `strict` if it chose their tops. One where no design
    /// builds a top has nothing to set.
    pub fn new(flags: &[String], designs: &[Chosen], strict: bool) -> Result<Flags> {
        if let Some(flag) = flags.first()
            && designs.iter().all(|it| it.tops.is_empty())
        {
            return Err(term::error(format!(
                "{} sets a parameter of a top, and the run builds none",
                mark::code(format!("-G {flag}"))
            ))
            .help(format!("name one with {}", mark::code("--top")))
            .into());
        }
        let flags = parsed(flags)?;
        Ok(Flags {
            declared: vec![false; flags.len()],
            flags,
            strict,
            instead: named_point,
        })
    }

    /// The parameters `flags` set, as `-G` takes them, for a run of the
    /// tests a filter selects.
    pub fn tests(flags: &[String]) -> Result<Flags> {
        let flags = parsed(flags)?;
        Ok(Flags {
            declared: vec![false; flags.len()],
            flags,
            strict: false,
            instead: filtered,
        })
    }

    /// Sets each parameter on `top` where its module declares it, but
    /// those a sweep of its test lists, `swept`, which the sweep sets and
    /// [`Flags::check`] refuses. Nothing is refused here: a run of tests
    /// refuses a parameter only for a test the filter selects, which the
    /// sweeps decide.
    pub(crate) fn set(&self, elaboration: &Elaboration, top: &mut Top, swept: &[&str]) {
        for (name, value) in &self.flags {
            let setting = [(name, TopSetting::Literal(value.clone()))];
            if swept.contains(&name.as_str())
                || elaboration.top_with(&top.module, &setting).is_err()
            {
                continue;
            }
            top.params.retain(|(it, _)| it != name);
            top.params.push((name.clone(), value.clone()));
        }
    }

    /// Refuses a parameter a top of `analysis` cannot take, or whose values
    /// a sweep lists, and counts as declared each that one takes. A strict
    /// run is one design, which refuses at once one no top declares.
    pub(crate) fn check(&mut self, analysis: &Analysis) -> Result {
        let elaboration = &analysis.elaboration;
        for ((name, value), declared) in self.flags.iter().zip(&mut self.declared) {
            let mut here = false;
            for Elaborated { top, values, .. } in &analysis.tops {
                if values.iter().any(|(it, _)| it == name) {
                    let test = point_of(&top.name).unwrap_or(&top.name);
                    return Err(term::error(format!(
                        "{} sets {}, whose values a sweep of {} lists",
                        mark::code(format!("-G {name}={value}")),
                        mark::name(name),
                        mark::name(test),
                    ))
                    .help((self.instead)(test, name, value))
                    .into());
                }
                let setting = [(name, TopSetting::Literal(value.clone()))];
                match elaboration.top_with(&top.module, &setting) {
                    Err(TopError::NoParameter(_)) => {}
                    Err(err) => return Err(refused(err, &top.module).into()),
                    Ok(_) => here = true,
                }
            }
            *declared |= here;
            if !self.strict || here {
                continue;
            }
            return Err(match analysis.tops.as_slice() {
                // One top's own error says which top it is.
                [one] => refused(TopError::NoParameter(name.clone()), &one.top.module).into(),
                _ => undeclared(name, value, "no top"),
            });
        }
        Ok(())
    }

    /// Counts as declared what `other`, of the same flags, found declared.
    pub fn add(&mut self, other: &Flags) {
        for (declared, other) in self.declared.iter_mut().zip(&other.declared) {
            *declared |= other;
        }
    }

    /// Refuses a parameter no top declared, `tops` saying which tops.
    pub fn undeclared(&self, tops: &str) -> Result {
        match (self.flags.iter().zip(&self.declared)).find(|(_, declared)| !**declared) {
            Some(((name, value), _)) => Err(undeclared(name, value, tops)),
            None => Ok(()),
        }
    }
}

/// Why `-G name=value` sets nothing: `tops` do not declare it.
fn undeclared(name: &str, value: &str, tops: &str) -> crate::error::Error {
    term::error(format!(
        "{} sets a parameter {tops} declares",
        mark::code(format!("-G {name}={value}"))
    ))
    .into()
}

/// How to choose the value of the parameter `name` a sweep of `test` lists,
/// rather than setting it to `value` with `-G`: by naming the point.
fn named_point(test: &str, name: &str, value: &str) -> String {
    format!(
        "name the point, as in {}",
        mark::code(format!("--top '{test}[{name}={value}]'"))
    )
}

/// How to choose the value of the parameter `name` a sweep lists, rather
/// than setting it to `value` with `-G`, in a run of tests: with a filter.
fn filtered(_: &str, name: &str, value: &str) -> String {
    format!(
        "select the points of one value with a filter, as in {}",
        mark::code(format!("astli test '{name}={value}'"))
    )
}

/// `tops` as one run for each set of defines they give, in the order of
/// the first top of each: those that define the same are built together,
/// as a filelist of them would be, and those that define one macro two
/// ways are built apart.
pub fn grouped(tops: Vec<Top>) -> Vec<Chosen> {
    let mut groups: Vec<Vec<Top>> = Vec::new();
    for top in tops {
        match groups
            .iter_mut()
            .find(|group| group[0].defines == top.defines)
        {
            Some(group) => group.push(top),
            None => groups.push(vec![top]),
        }
    }
    (groups.into_iter())
        .map(|group| Chosen::joined(group).expect("one set of defines joins"))
        .collect()
}

/// Why a run of a package whose `tables` hold several tops and no
/// default cannot tell which to build.
fn several(tables: &RootTables) -> crate::error::Error {
    let names: Vec<String> = tables.tops.keys().map(mark::name).collect();
    term::error(format!(
        "no top named, and the manifest has {}: {}",
        tables.tops.len(),
        names.join(", ")
    ))
    .help(format!(
        "name one with {}, or set {} in {}",
        mark::code("--top"),
        mark::code("default-top"),
        mark::code("[package]")
    ))
    .into()
}

/// The body of `module` as a top, with `params` set, by name, to the value
/// each spells, and `values` to theirs.
pub fn body(
    elaboration: &Elaboration,
    module: &str,
    params: &[(String, String)],
    values: &[(String, TopValue)],
) -> Result<BodyId> {
    let settings: Vec<_> = (params.iter())
        .map(|(name, text)| (name.as_str(), TopSetting::Literal(text.clone())))
        .chain(
            (values.iter()).map(|(name, value)| (name.as_str(), TopSetting::Value(value.clone()))),
        )
        .collect();
    elaboration
        .top_with(module, &settings)
        .map_err(|err| refused(err, module).into())
}

/// Why `module` cannot be a top, or one of its parameters not be set, as
/// `err` says.
pub fn refused(err: TopError, module: &str) -> term::Problem {
    let of = |name: &str| format!("{} of {}", mark::name(name), mark::name(module));
    match err {
        TopError::NoTop => term::error(format!(
            "no module, interface or program is called {}",
            mark::name(module)
        )),
        TopError::NoParameter(name) => term::error(format!(
            "{} has no parameter called {}",
            mark::name(module),
            mark::name(&name)
        )),
        TopError::Local(name) => term::error(format!(
            "{} is a {}, which only its declaration sets",
            of(&name),
            mark::code("localparam")
        )),
        TopError::Listed(name) => term::error(format!(
            "{} is local, since {} has a parameter port list",
            of(&name),
            mark::name(module)
        ))
        .help(format!(
            "declare it in the {} list, or remove the list",
            mark::code("#( ... )")
        )),
        TopError::Type(name) => term::error(format!(
            "{} is a type parameter, which a value cannot set",
            of(&name)
        )),
        TopError::Unknown(name) => term::error(format!(
            "the type of {} is not known well enough to set it",
            of(&name)
        )),
        TopError::NoSpelling(name) => term::error(format!(
            "{} has a type whose values have no short spelling",
            of(&name)
        )),
        TopError::NoValues(name) => term::error(format!(
            "{} has a type with more values than a list holds",
            of(&name)
        )),
        TopError::OutOfRange(name, text) => {
            term::error(format!("{} cannot hold {}", of(&name), mark::code(&text)))
        }
        TopError::NoMember(name, text) => term::error(format!(
            "{} is set to {}, which is no member of its enum",
            of(&name),
            mark::code(&text)
        )),
        TopError::NoMemberValued(name, text) => term::error(format!(
            "{} is set to {}, which is no member's value",
            of(&name),
            mark::code(&text)
        )),
        TopError::NotAValue(name, text) => {
            let err = term::error(format!(
                "{} is set to {}, which is no number or string",
                of(&name),
                mark::code(&text)
            ));
            // A shell takes the quotes of `-G Name="text"` for its own.
            match text.contains('"') {
                true => err,
                false => err.help(format!(
                    "a string is quoted, as in {}",
                    mark::code(format!("-G '{name}=\"{text}\"'"))
                )),
            }
        }
    }
}
