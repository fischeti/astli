//! `astli path` subcommand.
//!
//! Prints where each dependency named is, checked out first if need be, for
//! a script that needs what the package model has no word for, such as a
//! register description or a constraints file. A checkout is shared by every
//! design and holds the commit's files unchanged; the path changes with the
//! lock, so a script asks again each run rather than keeping it.

use std::io::Write as _;

use astli_resolve::Lock;
use usage_rs::{Args, RunWith};

use astli_package::mark;

use crate::cli::PackageArg;
use crate::cmd::{self, Ctx};
use crate::error::{Error, Result};
use crate::lockfile::{self, Write};

/// Print filesystem paths to checked-out dependency packages
#[derive(Args)]
pub struct Path {
    /// Names of dependencies as declared in the package manifest
    #[usage(arg, value_name = "NAME", required)]
    pub names: Vec<String>,
    #[usage(flatten)]
    pub package: PackageArg,
}

impl RunWith<Ctx<'_>> for Path {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        cmd::choose(self.package.package.as_deref(), false)?;
        let here = lockfile::here()?;
        let run = ctx.run;
        let resolution = lockfile::settle(
            &here,
            Lock::clone,
            Write::Manifest,
            lockfile::Tell::Design,
            run.locked,
        )?;
        let store = lockfile::store()?;
        let mut dirs = Vec::new();
        for name in &self.names {
            let Some(package) = resolution.packages.iter().find(|it| it.name == *name) else {
                return Err(Error::failed(format!(
                    "{} is not a dependency of the package, directly or not",
                    mark::name(name)
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
