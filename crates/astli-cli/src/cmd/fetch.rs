//! `astli fetch` subcommand.
//!
//! Resolves the dependencies of the package the current directory is in, as
//! any command reading them does, and checks out each into the shared cache,
//! so that later runs need no network. What the lock holds is kept.

use astli_resolve::{Lock, Store};
use usage_rs::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::Result;
use crate::lockfile::{self, Write};
use crate::render;
use crate::term::status;

/// Download and check out dependencies into the shared cache for offline use
#[derive(Args)]
pub struct Fetch {}

impl RunWith<Ctx<'_>> for Fetch {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let root = lockfile::here()?;
        let run = ctx.run;
        let resolution = lockfile::settle(&root, Lock::clone, Write::Manifest, run.locked)?;
        let store = Store::from_env()?;
        let fetched: Vec<_> = (resolution.packages.iter())
            .filter(|it| it.commit.is_some())
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
