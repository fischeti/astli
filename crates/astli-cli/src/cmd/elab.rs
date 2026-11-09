//! `astli elab` subcommand.
//!
//! Lowers every file of the design as `check` does, then elaborates it from
//! each top and prints the instance tree: each instance with its definition
//! and the parameters its overrides change, each generate block built, and
//! what sema cannot decide or find.
//!
//! With no file named, the design is the package the current directory is
//! in and its dependencies, as `check` reads it, and the tops are those
//! `--top` names or the manifest's `[top]` tables give.

use std::io::Write;

use usage_rs::{Args, RunWith};

use crate::analysis::{self, Analysis};
use crate::cli::{BuildArgs, FeatureArgs, Sources};
use crate::cmd::Ctx;
use crate::discovery::Plan;
use crate::error::{Error, Result};

/// Inspect the elaborated instance hierarchy for a top-level module
#[derive(Args)]
pub struct Elab {
    #[usage(flatten)]
    pub sources: Sources,
    #[usage(flatten)]
    pub features: FeatureArgs,
    /// Elaborate from the specified root module or [top.NAME] table (can be repeated)
    #[usage(long)]
    pub top: Vec<String>,
    #[usage(flatten)]
    pub build: BuildArgs,
}

impl RunWith<Ctx<'_>> for Elab {
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
        if plan.tops.modules.is_empty() {
            return Err(Error::failed("name a top to elaborate from with --top"));
        }
        let mut analysis = Analysis::new(out, &mut plan, run.jobs, false)?;
        // What the files reported is printed, and the tree is still built
        // from what they hold.
        let parsed = std::mem::take(&mut analysis.parsed);
        let reported = (analysis.origins.iter().zip(parsed))
            .zip(&plan.units)
            .filter(|(_, unit)| unit.reported)
            .map(|((origins, found), _)| (Some(origins), Vec::new(), 0, found))
            .collect();
        analysis::report(out, reported)?;
        for &(_, body) in &analysis.tops {
            out.write_all(analysis.elaboration.dump(body).as_bytes())
                .map_err(Error::Output)?;
        }
        analysis.leak();
        Ok(())
    }
}
