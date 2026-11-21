//! `astli remove` subcommand.
//!
//! Drops dependencies from the root's `astli.toml` and resolves again, so
//! that the lock no longer holds what nothing needs. A feature bringing a
//! removed dependency in and `[resolve] pinned` no longer mention it
//! either, since both would then name what the root does not require. A
//! description, an override or a patch of it stays: another dependency may
//! still bring it into the design.

use astli_package::mark;
use usage_rs::{Args, RunWith};

use crate::cli::PackageArg;
use crate::cmd::{self, Ctx};
use crate::error::Result;
use crate::manifest::{self, Editable, tables};
use crate::settle::Tell;
use crate::term::{self, status};

/// Remove dependencies from astli.toml and astli.lock
#[derive(Args)]
pub struct Remove {
    /// Names of the dependencies to remove
    #[usage(arg, value_name = "NAME", required)]
    pub names: Vec<String>,
    /// Remove from [dev-dependencies] instead of [dependencies]
    #[usage(long)]
    pub dev: bool,
    #[usage(flatten)]
    pub package: PackageArg,
}

impl RunWith<Ctx<'_>> for Remove {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let run = ctx.run;
        let target = cmd::choose(self.package.package.as_deref(), false)?;
        let here = target.here(run)?;
        let root = &here.package.package;
        let mut manifest = Editable::open(root)?;
        let (table, other) = tables(self.dev);
        if !self.dev
            && let Some(problem) = manifest::from_bender(&here, root)
        {
            return Err(problem.into());
        }
        for name in &self.names {
            if !manifest.unrequire(table, name) {
                let problem = term::error(format!(
                    "{} is not in {}",
                    mark::name(name),
                    mark::code(format!("[{table}]"))
                ));
                let flag = match self.dev {
                    true => "",
                    false => " --dev",
                };
                return Err(match manifest.holds(other, name) {
                    true => problem.help(format!(
                        "it is in {}: {} removes it",
                        mark::code(format!("[{other}]")),
                        mark::code(format!("astli remove{flag} {name}"))
                    )),
                    false => problem,
                }
                .into());
            }
        }
        manifest::commit(run, &here, &[manifest], None, Tell::Design, run.args.locked)?;
        for name in &self.names {
            status!("Removed", "{name} from [{table}]");
        }
        Ok(())
    }
}
