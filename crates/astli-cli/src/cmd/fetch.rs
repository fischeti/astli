//! `astli fetch` subcommand.
//!
//! Resolves the dependencies of the package the current directory is in, as
//! any command reading them does, and checks out each into the shared cache,
//! so that later runs need no network. What the lock holds is kept.

use astli_resolve::Lock;
use usage_rs::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::Result;
use crate::render;
use crate::settle::{self, Write};
use crate::target;
use crate::term::status;

/// Download and check out dependencies into the shared cache for offline use
#[derive(Args)]
pub struct Fetch {}

impl RunWith<Ctx<'_>> for Fetch {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let run = ctx.run;
        let here = target::here(run)?;
        let resolution = settle::settle(
            run,
            &here,
            &here.overrides(),
            Lock::clone,
            Write::Manifest,
            settle::Tell::Workspace,
            run.args.locked,
        )?;
        let store = run.store()?;
        let fetched: Vec<_> = (resolution.packages.iter())
            .filter(|it| it.origin.commit().is_some())
            .collect();
        for package in &fetched {
            package.checkout(&store)?;
        }
        status!(
            "Fetched",
            "{} into {}",
            render::plural(fetched.len(), "package"),
            store.root.display()
        );
        Ok(())
    }
}
