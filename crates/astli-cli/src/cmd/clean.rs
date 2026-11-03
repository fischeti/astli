//! `astli clean` subcommand.
//!
//! Deletes what builds wrote into the package's `.astli/build`, every
//! simulator's and every profile's; the next build starts from nothing.
//! Patches beside it stay, since they are the person's, not a build's.

use astli_package::{LOCAL, mark};
use usage_rs::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::{Error, Result};
use crate::lockfile;
use crate::term::status;

/// Delete the package's builds
#[derive(Args)]
pub struct Clean {}

impl RunWith<Ctx<'_>> for Clean {
    type Output = Result;

    fn run_with(self, _: Ctx<'_>) -> Result {
        let package = lockfile::here()?;
        let builds = package.root.join(LOCAL).join("build");
        match std::fs::remove_dir_all(&builds) {
            Ok(()) => status!("Removed", "{}", mark::path(builds.display())),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => status!(
                "Removed",
                "nothing, since {} does not exist",
                mark::path(builds.display())
            ),
            Err(err) => return Err(Error::io(&builds, err)),
        }
        Ok(())
    }
}
