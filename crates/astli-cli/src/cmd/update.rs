//! `astli update` subcommand.
//!
//! Resolves the dependencies of the package the current directory is in
//! again and writes `astli.lock`. Every package keeps the commit the lock
//! holds unless it is named, or a requirement no longer admits that commit;
//! with none named, the lock is set aside and every package takes the
//! highest version admitted.

use astli_package::Package;
use astli_resolve::{LOCKFILE, Lock};
use usage::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::{Error, Result};
use crate::lockfile::{self, Write};

/// Resolve the dependencies again, fetching them, and write astli.lock
#[derive(Args)]
pub struct Update {
    /// Update only these packages; the others keep the commit the lock holds
    #[usage(arg, value_name = "NAME")]
    pub names: Vec<String>,
}

impl RunWith<Ctx<'_>> for Update {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let here = std::env::current_dir().map_err(|err| Error::io(".", err))?;
        let root = Package::find(&here)?.ok_or_else(|| {
            Error::failed("no package here: run inside a git repository or beside an astli.toml")
        })?;
        if let Some(lock) = Lock::read(&root.root.join(LOCKFILE))?
            && let Some(name) = self.names.iter().find(|it| lock.get(it).is_none())
        {
            return Err(Error::failed(format!("`{name}` is not in {LOCKFILE}")));
        }
        let kept = |lock: &Lock| {
            let mut kept = Lock::default();
            if !self.names.is_empty() {
                kept = lock.clone();
                kept.packages.retain(|it| !self.names.contains(&it.name));
            }
            kept
        };
        let run = ctx.run;
        lockfile::settle(&root, kept, Write::Always, run.locked, run.quiet)?;
        Ok(())
    }
}
