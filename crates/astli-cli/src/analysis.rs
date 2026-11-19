//! The design `check`, `lint` and `elab` analyse: every unit of a plan
//! lowered, each expanded with its own package's build, less the units an
//! override replaces, elaborated from the plan's tops. The commands differ
//! only in what they report of it.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use astli_index::{Index, Summary};
use astli_parse::parse_expanded;
use astli_preproc::Build;
use astli_sema::{BodyId, Design, Elaboration, FileId, Hir, TopValue, lower};
use astli_text::{Diagnostic, Origins};
use rayon::prelude::*;

use crate::cmd::{self, Outcome};
use crate::discovery::{Plan, kept_only};
use crate::error::{Error, Result};
use crate::points;
use crate::render::{self, Out};
use crate::run::Run;
use crate::session;
use crate::term;
use crate::tops::{Points, Top};

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
    /// What is elaborated from: each of the plan's tops, or each point of
    /// one that stands for a test's points.
    pub tops: Vec<Elaborated>,
    /// How many of the plan's files could not be read, which were reported
    /// and left out of the plan; the run fails for them once it has
    /// reported the rest.
    pub unread: usize,
}

impl Analysis {
    /// Lowers every unit of `plan` on the run's threads and elaborates the
    /// design from its tops, which it takes. Units are summarized, for an
    /// [`Index`], if `summarize`, and wherever an override or a top needs
    /// to know what each declares.
    pub(crate) fn new(out: &mut Out, plan: &mut Plan, run: &Run, summarize: bool) -> Result<Self> {
        let chosen = std::mem::take(&mut plan.tops);
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
        for (hir, origin, summary, found) in outcome.values {
            hirs.push(hir);
            origins.push(origin);
            summaries.extend(summary);
            parsed.push(found);
        }
        let kept = plan.replace(&summaries)?;
        let index = summarize.then(|| Index::new(kept_only(summaries, &kept)));
        let (hirs, origins, parsed) = (
            kept_only(hirs, &kept),
            kept_only(origins, &kept),
            kept_only(parsed, &kept),
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
            if top.points == Points::Alone {
                elaborated.push(Elaborated {
                    body: crate::tops::body(&elaboration, &top.module, &top.params, &[])?,
                    top: top.clone(),
                    values: Vec::new(),
                });
                continue;
            }
            let declared = (index.as_ref())
                .and_then(|it| it.declaration(&top.module))
                .map(|(_, it)| it);
            let test = points::Test {
                top,
                declared,
                flagged: &chosen.flagged,
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
/// against, its summary if asked for, and what the expansion and the parser
/// reported.
pub(crate) type Lowered = (Arc<Hir>, Arc<Origins>, Option<Summary>, Vec<Diagnostic>);

/// Expands, parses and lowers one file, keeping its summary if
/// `summarize`.
fn lowered(path: &Path, build: &Build, summarize: bool) -> Result<Lowered> {
    let mut opened = session::open(path, build)?;
    let expanded = opened.expand();
    let parsed = parse_expanded(&opened.session, &expanded.tokens);
    let mut found = opened.diagnostics().to_vec();
    found.extend(parsed.diagnostics.iter().cloned());
    let hir = lower(&parsed);
    let summary = summarize.then(|| Summary::new(&opened.session, opened.file, &parsed));
    let origins = Arc::new(opened.session.into_origins());
    Ok((Arc::new(hir), origins, summary, found))
}

/// A file to report: what resolves its spans, what was already rendered of
/// it and how many errors that held, and what remains to render.
pub(crate) type Told<'a> = (Option<&'a Origins>, Vec<u8>, usize, Vec<Diagnostic>);

/// Renders what each file was found to hold, in parallel, and writes it in
/// order once `out` is flushed. A file is given with what resolves its
/// spans, unless it has no diagnostics, what was already rendered of it and
/// how many errors that held, and its diagnostics. Hands back how many of
/// them have errors, which [`Outcome::finish`] tallies.
pub(crate) fn report(out: &mut Out, files: Vec<Told>) -> Result<Outcome<()>> {
    write(out, render(files)?)
}

/// What each file was found to hold, rendered in parallel, with how many
/// errors it holds.
pub(crate) fn render(files: Vec<Told>) -> Result<Vec<(Vec<u8>, usize)>> {
    let rendered = |(origins, mut said, errors, found): Told| {
        let errors = match origins {
            Some(origins) => errors + render::diagnostics(&mut said, origins, &found)?,
            None => errors,
        };
        Ok((said, errors))
    };
    // A report that gathers every file's findings in one list gathers them
    // in the order rendered.
    match render::gathered() {
        true => files.into_iter().map(rendered).collect(),
        false => files.into_par_iter().map(rendered).collect(),
    }
}

/// Writes what was rendered of each file, in order, once `out` is flushed,
/// and hands back how many of them have errors.
pub(crate) fn write(out: &mut Out, rendered: Vec<(Vec<u8>, usize)>) -> Result<Outcome<()>> {
    out.flush()?;
    let mut outcome = Outcome::<()>::new(rendered.len());
    for (said, errors) in rendered {
        term::write(&said).map_err(Error::Output)?;
        outcome.values.push(());
        outcome.wrong += usize::from(errors > 0);
    }
    Ok(outcome)
}
