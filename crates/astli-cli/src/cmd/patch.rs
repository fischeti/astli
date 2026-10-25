//! `astli patch` subcommand.
//!
//! Points a dependency at a directory of this machine's, to work on it where
//! the design uses it, without its published manifest changing. A directory
//! that does not exist yet gets a clone of the package's repository at the
//! commit the design builds on. The patch is kept in `.astli/patch.toml`,
//! which ignores itself, so that it is never committed; while one is in
//! place, the lock is read and never written.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use astli_package::LOCAL;
use astli_resolve::{Lock, Origin, Resolved, Store};
use astli_text::clean;
use usage_rs::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::{Error, Result};
use crate::git::git;
use crate::lockfile::{self, Write};
use crate::term::status;

/// Override a dependency with a local checkout, cloning it if necessary
#[derive(Args)]
pub struct Patch {
    /// Name of the dependency to patch
    #[usage(arg, value_name = "NAME")]
    pub name: String,
    /// Local directory path for the checkout (cloned at the locked commit if non-existent) [default: .astli/patch/NAME]
    #[usage(arg, value_name = "DIR")]
    pub dir: Option<PathBuf>,
    /// Remove the active patch and return to the locked dependency version
    #[usage(long)]
    pub remove: bool,
}

impl RunWith<Ctx<'_>> for Patch {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let mut root = lockfile::here()?;
        let mut patches = root.patches.clone();
        let name = &self.name;
        if self.remove {
            if self.dir.is_some() {
                return Err(Error::failed("`--remove` takes no directory"));
            }
            let Some(dir) = patches.0.remove(name) else {
                return Err(Error::failed(format!("`{name}` is not patched")));
            };
            patches.write(&root.root)?;
            status!("Unpatched", "{name}, leaving {} as it is", dir.display());
            return Ok(());
        }

        // Resolved as if it were not patched, for where it comes from.
        root.patches.0.remove(name);
        let resolution = lockfile::settle(&root, Lock::clone, Write::Manifest, ctx.run.locked)?;
        let Some(package) = resolution.packages.iter().find(|it| it.name == *name) else {
            return Err(Error::failed(format!(
                "`{name}` is not a dependency of the package, directly or not"
            )));
        };
        let dir = match &self.dir {
            Some(dir) => std::path::absolute(dir).map_err(|err| Error::io(dir, err))?,
            None => root.root.join(LOCAL).join("patch").join(name),
        };
        let dir = clean(&dir);
        let dir = match dir.exists() {
            true if dir.is_dir() => dir,
            true => {
                return Err(Error::failed(format!(
                    "{} is not a directory",
                    dir.display()
                )));
            }
            false => cloned(package, &dir)?,
        };
        patches.0.insert(name.clone(), dir.clone());
        patches.write(&root.root)?;
        status!("Patched", "{name}, read from {}", dir.display());
        Ok(())
    }
}

/// Clones the repository `package` comes from into `dir`, at the commit
/// chosen for it, and returns the directory the package is in there. The
/// clone is made from the mirror, so it needs no network, and its `origin`
/// is the repository's URL, to push to.
fn cloned(package: &Resolved, dir: &Path) -> Result<PathBuf> {
    let name = &package.name;
    let (url, subdir) = match &package.origin {
        Origin::Git { url } => (url, None),
        Origin::Within { url, subdir } => (url, Some(subdir)),
        Origin::Dir(at) | Origin::Patched(at) => {
            return Err(Error::failed(format!(
                "`{name}` is read from {} already, so there is nothing to clone: work on it \
                 there, or name an existing directory to patch it to",
                at.display()
            )));
        }
    };
    let commit = package
        .commit
        .expect("a repository's package is at a commit");
    let commit = commit.to_string();
    status!(
        "Cloning",
        "{name} at {} into {}",
        &commit[..12],
        dir.display()
    );
    let mirror = Store::from_env()?.mirror(url)?;
    let parent = dir.parent().unwrap_or(dir);
    std::fs::create_dir_all(parent).map_err(|err| Error::io(parent, err))?;
    let clone = [
        OsStr::new("clone"),
        OsStr::new("--quiet"),
        OsStr::new("--no-checkout"),
    ];
    git(
        parent,
        clone
            .into_iter()
            .chain([mirror.dir().as_os_str(), dir.as_os_str()]),
    )?;
    git(dir, ["remote", "set-url", "origin", url])?;
    git(dir, ["checkout", "--quiet", "--detach", &commit])?;
    Ok(match subdir {
        Some(subdir) => dir.join(subdir),
        None => dir.to_path_buf(),
    })
}
