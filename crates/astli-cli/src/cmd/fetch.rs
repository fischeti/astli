//! `astli fetch` subcommand.
//!
//! Resolves the dependencies of the package the current directory is in, as
//! any command reading them does, and checks out each into the shared cache,
//! so that later runs need no network. What the lock holds is kept.

use astli_resolve::{Lock, Store};
use usage::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::Result;
use crate::lockfile::{self, Write};

/// Fetch the dependencies into the shared cache, so that later runs need no network
#[derive(Args)]
pub struct Fetch {}

impl RunWith<Ctx<'_>> for Fetch {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let root = lockfile::here()?;
        let run = ctx.run;
        let resolution =
            lockfile::settle(&root, Lock::clone, Write::Manifest, run.locked, run.quiet)?;
        if !run.quiet {
            let fetched = (resolution.packages.iter())
                .filter(|it| it.commit.is_some())
                .count();
            eprintln!(
                "Fetched {fetched} package(s) into {}",
                Store::from_env()?.root.display()
            );
        }
        Ok(())
    }
}
