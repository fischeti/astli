//! `astli check` subcommand.
//!
//! Lowers every file of the design, each expanded as it is compiled and in
//! parallel, then checks each against what all of them declare: its names,
//! the packages and definitions it uses, and each instance's connections
//! and overrides. What a file's expansion and parse report is printed as it
//! is lowered, and counts as an error too.
//!
//! With no file named, the design is the package the current directory is
//! in and its dependencies, and only the package's own files are reported.
//!
//! A variable driven twice is a warning, compared at its definition's
//! defaults. With `--top`, it elaborates the design from each top first: an
//! instance in a generate block is checked as what the tops build makes it,
//! and a variable driven twice in what is built is an error.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use astli_index::{Index, Summary};
use astli_parse::parse_expanded;
use astli_preproc::Build;
use astli_sema::{Built, Design, Elaboration, Hir, check, lower};
use astli_text::Origins;
use rayon::prelude::*;
use usage::{Args, RunWith};

use crate::cli::{BuildArgs, FeatureArgs, RunArgs, Sources};
use crate::cmd::{self, Ctx, files};
use crate::discovery::{Plan, kept_only};
use crate::error::{Error, Result};
use crate::render;
use crate::session;

/// Check a design for undeclared names and instances their definitions do not take; an error fails the run
#[derive(Args)]
pub struct Check {
    #[usage(flatten)]
    pub sources: Sources,
    #[usage(flatten)]
    pub features: FeatureArgs,
    /// Elaborate from this module, interface or program, and check what it builds (can be repeated)
    #[usage(long)]
    pub top: Vec<String>,
    #[usage(flatten)]
    pub build: BuildArgs,
}

impl RunWith<Ctx<'_>> for Check {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        let mut plan = Plan::new(&self.sources, &self.features, &self.build)?;

        // Nothing goes to standard output, so a heading per file would stand
        // alone.
        let silent = RunArgs {
            quiet: true,
            jobs: run.jobs,
            diagnostics: run.diagnostics,
        };
        let outcome = cmd::each(out, &plan.units, &silent, "", |sink, path| {
            let unit = plan.unit(path);
            lowered(sink, path, plan.build(unit), unit.reported)
        })?;
        if outcome.failed > 0 {
            return outcome.finish();
        }
        let mut wrong = outcome.wrong;
        let total = plan.units.iter().filter(|unit| unit.reported).count();
        let mut hirs = Vec::with_capacity(outcome.values.len());
        let mut origins = Vec::with_capacity(outcome.values.len());
        let mut summaries = Vec::with_capacity(outcome.values.len());
        for (hir, origin, summary) in outcome.values {
            hirs.push(hir);
            origins.push(origin);
            summaries.push(summary);
        }
        let kept = plan.replace(&summaries)?;
        let (hirs, origins, summaries) = (
            kept_only(hirs, &kept),
            kept_only(origins, &kept),
            kept_only(summaries, &kept),
        );
        let reported: Vec<usize> = (plan.units.iter().enumerate())
            .filter(|(_, unit)| unit.reported)
            .map(|(at, _)| at)
            .collect();
        let twice = files::redeclared(&Index::new(summaries), &reported, &self.top);

        let design = Design::new(hirs.into_iter().map(Arc::new).collect());
        let elaboration = Elaboration::new(Arc::new(design));
        let files: Vec<_> = elaboration.design().files().map(|(file, _)| file).collect();
        let (built, drivers) = match self.top.is_empty() {
            true => (None, Vec::new()),
            false => {
                let mut tops = Vec::new();
                for top in &self.top {
                    let Some(body) = elaboration.top(top) else {
                        return Err(Error::failed(format!(
                            "no module, interface or program is called `{top}`"
                        )));
                    };
                    tops.push(body);
                }
                let built = Built::new(&elaboration, &tops);
                let drivers = built.multiple_drivers(&elaboration);
                (Some(built), drivers)
            }
        };
        let mut found: Vec<_> = files
            .par_iter()
            .map(|&file| check(&elaboration, file, built.as_ref()))
            .collect();
        for (file, diagnostic) in drivers {
            found[file.index()].push(diagnostic);
        }
        for found in &mut found {
            found.sort_by_key(|diagnostic| (diagnostic.at.src_id, diagnostic.at.start));
        }

        out.flush()?;
        let mut stderr = std::io::stderr().lock();
        let reported = found.iter().zip(&origins).zip(&plan.units);
        for ((found, origins), _) in reported.filter(|(_, unit)| unit.reported) {
            let mut said = Vec::new();
            if render::diagnostics(&mut said, origins, found)? > 0 {
                wrong += 1;
            }
            stderr.write_all(&said).map_err(Error::Output)?;
        }
        twice?;
        match wrong {
            0 => Ok(()),
            _ if total == 1 => Err(Error::Silent),
            wrong => Err(Error::failed(format!(
                "{wrong} of {total} file(s) have errors"
            ))),
        }
    }
}

/// Expands, parses and lowers one file, printing what the expansion and the
/// parser reported if it is `reported`, and keeping what its spans resolve
/// against.
pub(crate) fn lowered(
    sink: &mut cmd::Sink,
    path: &Path,
    build: &Build,
    reported: bool,
) -> Result<(Hir, Origins, Summary)> {
    let mut opened = session::open(path, build)?;
    let expanded = opened.expand();
    let parsed = parse_expanded(&opened.session, &expanded.tokens);
    let mut found = opened.diagnostics().to_vec();
    found.extend(parsed.diagnostics.iter().cloned());
    if reported {
        sink.errors += render::diagnostics(sink.diagnostics, opened.session.origins(), &found)?;
    }
    let hir = lower(&parsed);
    let summary = Summary::new(&opened.session, opened.file, &parsed);
    Ok((hir, opened.session.into_origins(), summary))
}
