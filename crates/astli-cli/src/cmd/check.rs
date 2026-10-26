//! `astli check` subcommand.
//!
//! Lowers every file of the design, each expanded as it is compiled and in
//! parallel, then checks each against what all of them declare: its names,
//! the packages and definitions it uses, and each instance's connections
//! and overrides. What a file's expansion and parse report is printed before
//! what its check finds, and counts as an error too.
//!
//! With no file named, the design is the package the current directory is
//! in and its dependencies, and only the package's own files are reported.
//!
//! A variable driven twice is a warning, compared at its definition's
//! defaults. With `--top`, it elaborates the design from each top first: an
//! instance in a generate block is checked as what the tops build makes it,
//! and a variable driven twice in what is built is an error. Only the files
//! the tops reach are reported, and a name declared more than once is told
//! for every one of them, a dependency's too, since which declaration the
//! build means is a guess whoever's files hold them.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use astli_index::{Index, Summary};
use astli_parse::parse_expanded;
use astli_preproc::Build;
use astli_sema::{Built, Design, Elaboration, Hir, check, lower};
use astli_text::{Diagnostic, Origins};
use rayon::prelude::*;
use usage_rs::{Args, RunWith};

use astli_package::mark;

use crate::cli::{BuildArgs, FeatureArgs, RunArgs, Sources};
use crate::cmd::{self, Ctx, files};
use crate::discovery::{Plan, kept_only};
use crate::error::{Error, Result};
use crate::render;
use crate::session;
use crate::term;

/// Verify design semantics, port connections, parameter overrides, and undeclared identifiers
#[derive(Args)]
pub struct Check {
    #[usage(flatten)]
    pub sources: Sources,
    #[usage(flatten)]
    pub features: FeatureArgs,
    /// Elaborate hierarchy starting from the specified root module, interface, or program, or a `[top.NAME]` of astli.toml (can be repeated)
    #[usage(long)]
    pub top: Vec<String>,
    #[usage(flatten)]
    pub build: BuildArgs,
}

impl RunWith<Ctx<'_>> for Check {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        let mut plan = Plan::new(
            &self.sources,
            &self.features,
            &self.build,
            Some(&self.top),
            run,
        )?;
        let tops = std::mem::take(&mut plan.tops.modules);

        // Nothing goes to standard output, so a heading per file would stand
        // alone.
        let silent = RunArgs {
            quiet: true,
            jobs: run.jobs,
            diagnostics: run.diagnostics,
            color: run.color,
            locked: run.locked,
        };
        let outcome = cmd::each(out, &plan.units, &silent, "", |_, path| {
            lowered(path, plan.build(plan.unit(path)))
        })?;
        if outcome.failed > 0 {
            return outcome.finish();
        }
        let mut hirs = Vec::with_capacity(outcome.values.len());
        let mut origins = Vec::with_capacity(outcome.values.len());
        let mut summaries = Vec::with_capacity(outcome.values.len());
        let mut parsed = Vec::with_capacity(outcome.values.len());
        for (hir, origin, summary, found) in outcome.values {
            hirs.push(hir);
            origins.push(origin);
            summaries.push(summary);
            parsed.push(found);
        }
        let kept = plan.replace(&summaries)?;
        let (hirs, origins, summaries, mut parsed) = (
            kept_only(hirs, &kept),
            kept_only(origins, &kept),
            kept_only(summaries, &kept),
            kept_only(parsed, &kept),
        );
        let index = Index::new(summaries);

        let design = Design::new(hirs.into_iter().map(Arc::new).collect());
        let elaboration = match tops.is_empty() {
            true => Elaboration::new(Arc::new(design)),
            false => Elaboration::with_tops(Arc::new(design), &tops),
        };
        (plan.tops).unambiguous(|name| elaboration.design().definition(name).is_some())?;
        let files: Vec<_> = elaboration.design().files().map(|(file, _)| file).collect();
        let (built, drivers) = match tops.is_empty() {
            true => (None, Vec::new()),
            false => {
                let mut bodies = Vec::new();
                for top in &tops {
                    let Some(body) = elaboration.top(top) else {
                        return Err(Error::failed(format!(
                            "no module, interface or program is called {}",
                            mark::name(top)
                        )));
                    };
                    bodies.push(body);
                }
                let built = Built::new(&elaboration, &bodies);
                let drivers = built.multiple_drivers(&elaboration);
                (Some(built), drivers)
            }
        };
        let reported: Vec<usize> = (plan.units.iter().enumerate())
            .filter(|(_, unit)| unit.reported)
            .map(|(at, _)| at)
            .collect();
        // What is declared twice is told as `files` tells it, so that the
        // filelist it writes for the tops and the check of the same tops
        // agree. The rest is told for the package's own files the tops
        // reach: a file no top builds is another top's to check.
        let (told, shown) = match tops.is_empty() {
            true => (reported.clone(), reported),
            false => {
                let reached =
                    (index.reachable(&tops)).map_err(|err| Error::failed(err.to_string()))?;
                let shown = (reached.iter().copied())
                    .filter(|&file| plan.units[file].reported)
                    .collect();
                (reached, shown)
            }
        };
        let twice = files::redeclared(&index, &told, &tops);
        let mut found: Vec<Vec<Diagnostic>> = vec![Vec::new(); files.len()];
        let checked: Vec<_> = (shown.par_iter())
            .map(|&file| check(&elaboration, files[file], built.as_ref()))
            .collect();
        for (&file, checked) in shown.iter().zip(checked) {
            found[file] = checked;
        }
        for (file, diagnostic) in drivers {
            found[file.index()].push(diagnostic);
        }

        out.flush()?;
        let mut wrong = 0;
        for &file in &shown {
            let mut checked = std::mem::take(&mut found[file]);
            checked.sort_by_key(|diagnostic| (diagnostic.at.src_id, diagnostic.at.start));
            // What the expansion and the parser said comes first, as they
            // found it.
            let mut all = std::mem::take(&mut parsed[file]);
            all.extend(checked);
            let mut said = Vec::new();
            if render::diagnostics(&mut said, &origins[file], &all)? > 0 {
                wrong += 1;
            }
            term::write(&said).map_err(Error::Output)?;
        }
        twice?;
        let total = shown.len();
        match wrong {
            0 => Ok(()),
            _ if total == 1 => Err(Error::Silent),
            wrong => Err(Error::tally(format!(
                "{wrong} of {} have errors",
                render::plural(total, "file")
            ))),
        }
    }
}

/// Expands, parses and lowers one file, keeping what its spans resolve
/// against and what the expansion and the parser reported.
pub(crate) fn lowered(
    path: &Path,
    build: &Build,
) -> Result<(Hir, Origins, Summary, Vec<Diagnostic>)> {
    let mut opened = session::open(path, build)?;
    let expanded = opened.expand();
    let parsed = parse_expanded(&opened.session, &expanded.tokens);
    let mut found = opened.diagnostics().to_vec();
    found.extend(parsed.diagnostics.iter().cloned());
    let hir = lower(&parsed);
    let summary = Summary::new(&opened.session, opened.file, &parsed);
    Ok((hir, opened.session.into_origins(), summary, found))
}
