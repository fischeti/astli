//! The design `check`, `lint` and `elab` analyse: every unit of a plan
//! lowered, each expanded with its own package's build, less the units an
//! override replaces, elaborated from the tops chosen. The commands differ
//! only in what they report of it. `check`, `lint`, `build` and `test`
//! analyse a package once for each set of defines its tops give, through
//! [`Designs::each`].

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use astli_index::{Index, Summary};
use astli_parse::parse_expanded;
use astli_preproc::Build;
use astli_sema::{BodyId, Design, Elaboration, FileId, Hir, TopValue, lower};
use astli_text::{Diagnostic, Origins};

use crate::cmd::{self, Outcome};
use crate::discovery::{Plan, kept_only};
use crate::error::{Error, Result};
use crate::marks::{Marks, marks};
use crate::points;
use crate::render::{self, Out, Report};
use crate::run::Run;
use crate::session;
use crate::term;
use crate::tops::{Chosen, Flags, Points, Top};

/// A plan's design, lowered and elaborated. Each list is in the order of the
/// plan's units, once [`Plan::replace`] has dropped those an override
/// replaces.
pub(crate) struct Analysis {
    pub elaboration: Elaboration,
    /// Each unit's file in the design.
    pub files: Vec<FileId>,
    /// What each unit's spans resolve against.
    pub origins: Vec<Arc<Origins>>,
    /// What each unit's expansion and parser reported, in the order found.
    pub parsed: Vec<Vec<Diagnostic>>,
    /// What each unit declares and uses, where the run asked for it.
    pub index: Option<Index>,
    /// What is elaborated from: each top chosen, or each point of one that
    /// stands for a test's points.
    pub tops: Vec<Elaborated>,
    /// How many of the plan's files could not be read, which were reported
    /// and left out of the plan; the run fails for them once it has
    /// reported the rest.
    pub unread: usize,
}

impl Analysis {
    /// Lowers every unit of `plan` on the run's threads and elaborates the
    /// design from `chosen`, whose defines [`Plan::define`] gave `plan`,
    /// with the parameters `flags` sets. Units are summarized, for an
    /// [`Index`], if `summarize`, and wherever an override or a top needs
    /// to know what each declares.
    pub(crate) fn new(
        out: &mut Out,
        plan: &mut Plan,
        chosen: &Chosen,
        flags: &Flags,
        run: &Run,
        summarize: bool,
    ) -> Result<Self> {
        let summarize = summarize || plan.overrides() || !chosen.tops.is_empty();
        let outcome = cmd::each(out, &plan.units, run, None, |_, path| {
            let unit = plan.unit(path);
            match plan.made(unit) {
                // The designs of a workspace hold mostly the same files,
                // built alike, so each is lowered once for all of them that
                // build it with what `made` says.
                Some(made) => run.lowered(path.to_path_buf(), made, || {
                    lowered(path, plan.build(unit), true)
                }),
                None => lowered(path, plan.build(unit), summarize),
            }
        })?;
        plan.unread(&outcome.failed_of(&plan.units));
        let unread = outcome.failed;

        let count = outcome.values.len();
        let mut hirs = Vec::with_capacity(count);
        let mut origins = Vec::with_capacity(count);
        let mut summaries = Vec::with_capacity(count);
        let mut parsed = Vec::with_capacity(count);
        let mut marked = Vec::with_capacity(count);
        for (hir, origin, summary, found) in outcome.values {
            hirs.push(hir);
            origins.push(origin);
            match summary {
                Some((summary, marks)) => {
                    summaries.push(summary);
                    marked.push(marks);
                }
                None => marked.push(Vec::new()),
            }
            parsed.push(found);
        }
        let kept = plan.replace(&summaries)?;
        let index = summarize.then(|| Index::new(kept_only(summaries, &kept)));
        let (hirs, origins, parsed, marked) = (
            kept_only(hirs, &kept),
            kept_only(origins, &kept),
            kept_only(parsed, &kept),
            kept_only(marked, &kept),
        );

        let design = Arc::new(Design::new(hirs));
        let modules: Vec<&str> = (chosen.tops.iter())
            .map(|top| top.module.as_str())
            .collect();
        let elaboration = match modules.is_empty() {
            true => Elaboration::new(design),
            false => Elaboration::with_tops(design, &modules),
        };
        chosen.unambiguous(|name| elaboration.design().definition(name).is_some())?;
        let files = elaboration.design().files().map(|(file, _)| file).collect();
        let mut elaborated = Vec::with_capacity(chosen.tops.len());
        let mut wrong = 0;
        for top in &chosen.tops {
            let mut top = top.clone();
            if top.points == Points::Alone {
                flags.set(&elaboration, &mut top, &[]);
                elaborated.push(Elaborated {
                    body: crate::tops::body(&elaboration, &top.module, &top.params, &[])?,
                    top,
                    values: Vec::new(),
                });
                continue;
            }
            let declared = (index.as_ref()).and_then(|it| it.declaration(&top.module));
            // Read off the tree this design expands, as its sweeps are
            // elaborated in it.
            let marks = declared.and_then(|(file, declaration)| {
                (marked[file].iter()).find(|it| it.location == declaration.location)
            });
            let swept: Vec<&str> = (marks.iter())
                .flat_map(|it| &it.sweeps)
                .flat_map(|sweep| &sweep.parameters)
                .map(String::as_str)
                .collect();
            flags.set(&elaboration, &mut top, &swept);
            let test = points::Test {
                top: &top,
                declared: declared.is_some(),
                marks,
                run,
            };
            elaborated.extend(test.points(&elaboration, &mut wrong)?);
        }
        if wrong > 0 {
            return Err(Error::tally(format!(
                "{} swept no parameter",
                render::plural(wrong, "attribute")
            )));
        }
        Ok(Analysis {
            elaboration,
            files,
            origins,
            parsed,
            index,
            tops: elaborated,
            unread,
        })
    }

    /// The definitions the tops elaborate.
    pub(crate) fn top_names(&self) -> Vec<String> {
        (self.tops.iter()).map(|it| it.top.module.clone()).collect()
    }

    /// The body of each top.
    pub(crate) fn bodies(&self) -> Vec<BodyId> {
        self.tops.iter().map(|top| top.body).collect()
    }

    /// Leaves the design to the operating system: dropping every HIR and
    /// every origin takes the main thread tens of milliseconds on a large
    /// design, just before the process exits and frees it all at once.
    pub(crate) fn leak(self) {
        std::mem::forget(self);
    }
}

/// The designs a run analyses of one package, each the tops of one set of
/// defines, and what `-G` sets in them.
pub(crate) struct Designs {
    pub chosen: Vec<Chosen>,
    pub flags: Flags,
}

impl Designs {
    /// What `check` and `lint` analyse of the package `plan` holds: the
    /// tops `asked` names, and if `tests` every point of each test it
    /// marks, as [`Chosen::designs`] groups them. A run that names its tops
    /// and no test is one design, which refuses at once a parameter none
    /// of them declares.
    pub(crate) fn of(
        out: &mut Out,
        plan: &Plan,
        asked: &[String],
        tests: bool,
        param: &[String],
        run: &Run,
    ) -> Result<Designs> {
        let tests = match tests {
            true => points::names(points::marked(out, plan, run)?),
            false => Vec::new(),
        };
        let chosen = Chosen::designs(plan, asked, &tests)?;
        let strict = !asked.is_empty() && tests.is_empty();
        let flags = Flags::new(param, &chosen, strict)?;
        Ok(Designs { chosen, flags })
    }

    /// Analyses the design `plan` holds once for each of the designs, each
    /// from the plan [`Plan::define`] makes for its tops and summarized if
    /// `summarize`, and hands each to `each` with that plan and the flags,
    /// which refuses with [`Flags::check`] what `-G` cannot set. Hands
    /// back what `each` made of each, in order.
    ///
    /// Several designs tell each problem and finding once, since each reads
    /// the same files. Each analysis is dropped once `each` is done with
    /// it, but the last where the run `ends` with these designs, which is
    /// left to the process's exit, unless another package's run follows
    /// and would add its design to this one's.
    pub(crate) fn each<T>(
        &mut self,
        out: &mut Out,
        plan: Plan,
        run: &Run,
        summarize: bool,
        ends: bool,
        mut each: impl FnMut(&mut Out, Plan, &mut Analysis, &mut Flags) -> Result<T>,
    ) -> Result<Vec<T>> {
        if self.chosen.len() > 1 {
            run.tell_once();
        }
        let mut made = Vec::with_capacity(self.chosen.len());
        let mut whole = Some(plan);
        for (at, tops) in self.chosen.iter().enumerate() {
            let last = at + 1 == self.chosen.len();
            // The last design takes the plan itself, each before it a copy.
            let mut plan = match last {
                true => whole.take(),
                false => whole.clone(),
            }
            .expect("the plan is kept for the last design");
            plan.define(tops);
            let mut analysis = Analysis::new(out, &mut plan, tops, &self.flags, run, summarize)?;
            made.push(each(out, plan, &mut analysis, &mut self.flags)?);
            match last && ends && !run.several_packages() {
                true => analysis.leak(),
                false => drop(analysis),
            }
        }
        Ok(made)
    }
}

/// A top as elaborated: one the run chose, or a point of one that stands
/// for a test's points, named for it as `--top` would name it.
#[derive(Clone)]
pub(crate) struct Elaborated {
    pub top: Top,
    pub body: BodyId,
    /// What its point sets each swept parameter to, in declaration order.
    pub values: Vec<(String, TopValue)>,
}

impl Elaborated {
    /// Every parameter it sets, by name, each value as a simulator's `-G`
    /// takes it: an enum's member as its integer.
    pub(crate) fn settings(&self) -> Vec<(String, String)> {
        let values = (self.values.iter()).map(|(name, value)| (name.clone(), value.integer()));
        self.top.params.iter().cloned().chain(values).collect()
    }
}

/// A file expanded, parsed and lowered: its HIR, what its spans resolve
/// against, its summary and the marks on its declarations if asked for, and
/// what the expansion and the parser reported.
pub(crate) type Lowered = (
    Arc<Hir>,
    Arc<Origins>,
    Option<(Summary, Vec<Marks>)>,
    Vec<Diagnostic>,
);

/// Expands, parses and lowers one file, keeping its summary and its marks
/// if `summarize`.
fn lowered(path: &Path, build: &Build, summarize: bool) -> Result<Lowered> {
    let mut opened = session::open(path, build)?;
    let expanded = opened.expand();
    let parsed = parse_expanded(&opened.session, &expanded.tokens);
    let mut found = opened.diagnostics().to_vec();
    found.extend(parsed.diagnostics.iter().cloned());
    let hir = lower(&parsed);
    let summary = summarize.then(|| {
        let summary = Summary::new(&opened.session, opened.file, &parsed);
        (summary, marks(opened.session.origins(), &parsed))
    });
    let origins = Arc::new(opened.session.into_origins());
    Ok((Arc::new(hir), origins, summary, found))
}

/// Writes what was rendered of each file, in order, once `out` is flushed,
/// and hands back how many of them have errors, which
/// [`Outcome::finish`] tallies.
pub(crate) fn write(out: &mut Out, rendered: Vec<Report>) -> Result<Outcome<()>> {
    out.flush()?;
    let mut outcome = Outcome::<()>::new(rendered.len());
    for said in rendered {
        term::write(&said.said).map_err(Error::Output)?;
        outcome.values.push(());
        outcome.wrong += usize::from(said.errors > 0);
    }
    Ok(outcome)
}
