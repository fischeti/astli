//! Running `git` on a clone a person works in.
//!
//! The shared store is read and written through `gix` alone. A clone that
//! `astli patch` makes is the person's to commit in and push from, so it is
//! made and read by their own `git`, with their configuration.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use astli_package::mark;

use crate::error::{Error, Result};

/// The variables naming a repository or what is inside it, of those `git
/// rev-parse --local-env-vars` prints: `git` clears them itself before it
/// works in another repository. It sets `GIT_DIR` for a hook or a `rebase
/// --exec` it runs, and inherited, that would point the `git remote set-url`
/// and `git checkout` `astli patch` runs in its clone at the repository astli
/// was started in. Its other three, `GIT_CONFIG`,
/// `GIT_CONFIG_PARAMETERS` and `GIT_CONFIG_COUNT`, stay: they carry
/// configuration a user or a CI job sets on purpose, such as a `url.*.
/// insteadOf` or an `http.extraHeader` that fetching a private dependency
/// needs.
const REPOSITORY_ENV: [&str; 12] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_GRAFT_FILE",
    "GIT_SHALLOW_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
];

/// What `git` prints for `args`, run in `dir`, trimmed.
pub fn git<S: AsRef<OsStr>>(dir: &Path, args: impl IntoIterator<Item = S>) -> Result<String> {
    let args: Vec<S> = args.into_iter().collect();
    let shown = (args.iter())
        .map(|it| it.as_ref().to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    let mut command = Command::new("git");
    for name in REPOSITORY_ENV {
        command.env_remove(name);
    }
    let output = (command.args(&args).current_dir(dir).output()).map_err(|err| {
        Error::failed(format!(
            "cannot run {}: {err}",
            mark::code(format!("git {shown}"))
        ))
    })?;
    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr);
        return Err(Error::failed(format!(
            "{} failed in {}: {}",
            mark::code(format!("git {shown}")),
            mark::path(dir.display()),
            said.trim_end()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
