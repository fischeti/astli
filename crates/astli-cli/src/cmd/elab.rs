//! `astli elab` subcommand.
//!
//! Lowers every file of the design as `check` does, then elaborates it from
//! each top named and prints the instance tree: each instance with its
//! definition and the parameters its overrides change, each generate block
//! built, and what sema cannot decide or find.

use std::io::Write;
use std::sync::Arc;

use astli_sema::{Design, Elaboration, Hir};
use usage::{Args, RunWith};

use crate::cli::{BuildArgs, RunArgs, Sources};
use crate::cmd::check::lowered;
use crate::cmd::{self, Ctx};
use crate::error::{Error, Result};
use crate::sources;

/// Inspect the elaborated instance hierarchy for a top-level module
#[derive(Args)]
pub struct Elab {
    #[usage(flatten)]
    pub sources: Sources,
    /// Top-level module, interface, or program to elaborate (can be repeated)
    #[usage(long)]
    pub top: Vec<String>,
    #[usage(flatten)]
    pub build: BuildArgs,
}

impl RunWith<Ctx<'_>> for Elab {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        if self.top.is_empty() {
            return Err(Error::failed("name a top to elaborate from with --top"));
        }
        let resolved = sources::resolve(&self.sources, &self.build)?;
        let build = resolved.build();
        let silent = RunArgs {
            quiet: true,
            jobs: run.jobs,
            diagnostics: run.diagnostics,
            locked: run.locked,
        };
        let outcome = cmd::each(out, &resolved.files, &silent, "", |sink, path| {
            lowered(sink, path, &build, true)
        })?;
        if outcome.failed > 0 {
            return outcome.finish();
        }
        let hirs: Vec<Hir> = outcome.values.into_iter().map(|(hir, ..)| hir).collect();
        let design = Design::new(hirs.into_iter().map(Arc::new).collect());
        let elaboration = Elaboration::with_tops(Arc::new(design), &self.top);
        for top in &self.top {
            let Some(body) = elaboration.top(top) else {
                return Err(Error::failed(format!(
                    "no module, interface or program is called `{top}`"
                )));
            };
            out.write_all(elaboration.dump(body).as_bytes())
                .map_err(Error::Output)?;
        }
        Ok(())
    }
}
