//! The design `check`, `lint` and `elab` analyse: every unit of a plan
//! lowered, each expanded with its own package's build, less the units an
//! override replaces, elaborated from the plan's tops. The commands differ
//! only in what they report of it.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use astli_index::{Index, Summary};
use astli_package::mark;
use astli_parse::parse_expanded;
use astli_preproc::Build;
use astli_sema::{BodyId, Design, Elaboration, FileId, Hir, lower};
use astli_text::{Diagnostic, Origins};
use rayon::prelude::*;

use crate::cmd::{self, Outcome};
use crate::discovery::{Plan, kept_only};
use crate::error::{Error, Result};
use crate::render::{self, Out};
use crate::session;
use crate::term;

/// A plan's design, lowered and elaborated. Each list is in the order of the
/// plan's units, once [`Plan::replace`] has dropped those an override
/// replaces.
pub(crate) struct Analysis {
    pub elaboration: Elaboration,
    /// Each unit's file in the design.
    pub files: Vec<FileId>,
    /// What each unit's spans resolve against.
    pub origins: Vec<Origins>,
    /// What each unit's expansion and parser reported, in the order found.
    pub parsed: Vec<Vec<Diagnostic>>,
    /// What each unit declares and uses, where the run asked for it.
    pub index: Option<Index>,
    /// The definitions the plan's tops name, and the body of each.
    pub tops: Vec<(String, BodyId)>,
}

impl Analysis {
    /// Lowers every unit of `plan` on `jobs` threads and elaborates the
    /// design from its tops, which it takes. Units are summarized, for an
    /// [`Index`], if `summarize`, and wherever an override or a top needs
    /// to know what each declares.
    pub(crate) fn new(
        out: &mut Out,
        plan: &mut Plan,
        jobs: usize,
        summarize: bool,
    ) -> Result<Self> {
        let tops = std::mem::take(&mut plan.tops.modules);
        let summarize = summarize || plan.overrides() || !tops.is_empty();
        let outcome = cmd::each(out, &plan.units, jobs, None, |_, path| {
            lowered(path, plan.build(plan.unit(path)), summarize)
        })?;
        // A file that could not be read leaves nothing to check it against.
        if outcome.failed > 0 {
            outcome.finish()?;
        }

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

        let design = Arc::new(Design::new(hirs.into_iter().map(Arc::new).collect()));
        let elaboration = match tops.is_empty() {
            true => Elaboration::new(design),
            false => Elaboration::with_tops(design, &tops),
        };
        (plan.tops).unambiguous(|name| elaboration.design().definition(name).is_some())?;
        let files = elaboration.design().files().map(|(file, _)| file).collect();
        let mut bodies = Vec::with_capacity(tops.len());
        for top in tops {
            let Some(body) = elaboration.top(&top) else {
                return Err(Error::failed(format!(
                    "no module, interface or program is called {}",
                    mark::name(&top)
                )));
            };
            bodies.push((top, body));
        }
        Ok(Analysis {
            elaboration,
            files,
            origins,
            parsed,
            index,
            tops: bodies,
        })
    }

    /// The names of the tops.
    pub(crate) fn top_names(&self) -> Vec<String> {
        self.tops.iter().map(|(name, _)| name.clone()).collect()
    }

    /// The body of each top.
    pub(crate) fn bodies(&self) -> Vec<BodyId> {
        self.tops.iter().map(|&(_, body)| body).collect()
    }

    /// Leaves the design to the operating system: dropping every HIR and
    /// every origin takes the main thread tens of milliseconds on a large
    /// design, just before the process exits and frees it all at once.
    pub(crate) fn leak(self) {
        std::mem::forget(self);
    }
}

/// Expands, parses and lowers one file, keeping what its spans resolve
/// against, its summary if `summarize`, and what the expansion and the
/// parser reported.
pub(crate) fn lowered(
    path: &Path,
    build: &Build,
    summarize: bool,
) -> Result<(Hir, Origins, Option<Summary>, Vec<Diagnostic>)> {
    let mut opened = session::open(path, build)?;
    let expanded = opened.expand();
    let parsed = parse_expanded(&opened.session, &expanded.tokens);
    let mut found = opened.diagnostics().to_vec();
    found.extend(parsed.diagnostics.iter().cloned());
    let hir = lower(&parsed);
    let summary = summarize.then(|| Summary::new(&opened.session, opened.file, &parsed));
    Ok((hir, opened.session.into_origins(), summary, found))
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
    let rendered = |(origins, mut said, errors, found): Told| {
        let errors = match origins {
            Some(origins) => errors + render::diagnostics(&mut said, origins, &found)?,
            None => errors,
        };
        Ok((said, errors))
    };
    // A report that gathers every file's findings in one list gathers them
    // in the order rendered.
    let rendered: Vec<(Vec<u8>, usize)> = match render::gathered() {
        true => files.into_iter().map(rendered).collect::<Result<_>>()?,
        false => files.into_par_iter().map(rendered).collect::<Result<_>>()?,
    };
    out.flush()?;
    let mut outcome = Outcome::<()>::new(rendered.len());
    for (said, errors) in rendered {
        term::write(&said).map_err(Error::Output)?;
        outcome.values.push(());
        outcome.wrong += usize::from(errors > 0);
    }
    Ok(outcome)
}
