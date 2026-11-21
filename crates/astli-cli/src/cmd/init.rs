//! `astli init` subcommand.
//!
//! Initializes a new `astli.toml` manifest in the current directory.
//!
//! Sets the package name based on an existing `Bender.yml` or the directory name.
//! Its dependencies are left to `astli bender import`, so that they are
//! copied only on purpose.

use astli_package::{BENDER_MANIFEST, MANIFEST, mark};
use usage_rs::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::{Error, Result};
use crate::manifest::{self, Editable};
use crate::term::{self, status};

/// Initialize a new astli.toml manifest in the current directory
#[derive(Args)]
pub struct Init {}

impl RunWith<Ctx<'_>> for Init {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let run = ctx.run;
        let here = std::env::current_dir().map_err(|err| Error::io(".", err))?;
        let path = here.join(MANIFEST);
        if path.exists() {
            return Err(Error::failed(format!("{} already exists", path.display())));
        }
        let bender = manifest::bender_text(&here)?;
        let (name, manifest) = Editable::named(&here, bender.as_deref())?;
        manifest.create()?;
        status!("Created", "{MANIFEST} for {}", mark::name(&name));
        if bender.is_some() {
            term::note(format!(
                "dependencies are still read from {BENDER_MANIFEST}"
            ))
            .help(format!(
                "{} copies them in",
                mark::code("astli bender import")
            ))
            .report(run);
        }
        Ok(())
    }
}
