//! `astli path` subcommand.
//!
//! Prints where each dependency named is, checked out first if need be, for
//! a script that needs what the package model has no word for, such as a
//! register description or a constraints file. A checkout is shared by every
//! design and holds the commit's files unchanged; the path changes with the
//! lock, so a script asks again each run rather than keeping it.

use std::io::Write as _;

use astli_resolve::{Lock, Store};
use usage::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::{Error, Result};
use crate::lockfile::{self, Write};

/// Print the directory of each dependency named, one a line, checked out first if need be
#[derive(Args)]
pub struct Path {
    /// The dependencies, by the names the manifests give them
    #[usage(arg, value_name = "NAME", required)]
    pub names: Vec<String>,
}

impl RunWith<Ctx<'_>> for Path {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let root = lockfile::here()?;
        let run = ctx.run;
        let resolution =
            lockfile::settle(&root, Lock::clone, Write::Manifest, run.locked, run.quiet)?;
        let store = Store::from_env()?;
        let mut dirs = Vec::new();
        for name in &self.names {
            let Some(package) = resolution.packages.iter().find(|it| it.name == *name) else {
                return Err(Error::failed(format!(
                    "`{name}` is not a dependency of the package, directly or not"
                )));
            };
            dirs.push(package.checkout(&store)?.dir);
        }
        for dir in dirs {
            writeln!(ctx.out, "{}", dir.display())?;
        }
        Ok(())
    }
}
