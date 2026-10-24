//! `astli update` subcommand.
//!
//! Resolves the dependencies of the package the current directory is in
//! again and writes `astli.lock`. Every package keeps the commit the lock
//! holds unless it is named, or a requirement no longer admits that commit;
//! with none named, the lock is set aside and every package takes the
//! highest version admitted.

use std::collections::BTreeSet;

use astli_package::Package;
use astli_resolve::{LOCKFILE, Lock, Store, resolve};
use usage::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::{Error, Result};

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
        let path = root.root.join(LOCKFILE);
        let old = Lock::read(&path)?;

        let mut kept = Lock::default();
        if let Some(old) = &old
            && !self.names.is_empty()
        {
            if let Some(name) = self.names.iter().find(|it| old.get(it).is_none()) {
                return Err(Error::failed(format!(
                    "`{name}` is not in {}",
                    path.display()
                )));
            }
            kept = old.clone();
            kept.packages.retain(|it| !self.names.contains(&it.name));
        }
        let pinned: BTreeSet<String> = root.pinned.iter().cloned().collect();
        let resolution = resolve(&Store::from_env()?, &root, &pinned, &kept)?;
        for warning in &resolution.warnings {
            eprintln!("astli: {warning}");
        }

        let new = Lock::of(&resolution);
        if !ctx.run.quiet {
            for change in old.unwrap_or_default().changes(&new) {
                eprintln!("{change}");
            }
        }
        let text = new.to_string();
        if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            std::fs::write(&path, text).map_err(|err| Error::io(&path, err))?;
        }
        Ok(())
    }
}
