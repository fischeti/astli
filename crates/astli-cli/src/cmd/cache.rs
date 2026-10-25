//! `astli cache` subcommand.
//!
//! The cache of mirrors and checkouts every design on the machine shares:
//! where it is, and deleting it. Everything in it is fetched again when a
//! run next needs it.

use std::io::Write as _;

use astli_resolve::Store;
use usage_rs::{Args, RunWith, Subcommands};

use crate::cmd::Ctx;
use crate::error::Result;

/// Inspect or clean the shared dependency cache
#[derive(Args)]
#[usage(run_with)]
pub struct Cache {
    #[usage(subcommand)]
    pub command: CacheCommand,
}

#[derive(Subcommands)]
#[usage(run_with)]
pub enum CacheCommand {
    Clean(Clean),
    Dir(Dir),
}

/// Clear cached repositories (packages will be re-downloaded when needed)
#[derive(Args)]
pub struct Clean {}

/// Print the absolute path to the local dependency cache directory
#[derive(Args)]
pub struct Dir {}

impl RunWith<Ctx<'_>> for Clean {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let store = Store::from_env()?;
        let removed = store.clean()?;
        if !ctx.run.quiet {
            match removed {
                true => eprintln!("Removed {}", store.root.display()),
                false => eprintln!("No cache at {}", store.root.display()),
            }
        }
        Ok(())
    }
}

impl RunWith<Ctx<'_>> for Dir {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        writeln!(ctx.out, "{}", Store::from_env()?.root.display())?;
        Ok(())
    }
}
