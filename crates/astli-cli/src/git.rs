//! Running `git` on a clone a person works in.
//!
//! The shared store is read and written through `gix` alone. A clone that
//! `astli patch` makes is the person's to commit in and push from, so it is
//! made and read by their own `git`, with their configuration.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};

/// What `git` prints for `args`, run in `dir`, trimmed.
pub fn git<S: AsRef<OsStr>>(dir: &Path, args: impl IntoIterator<Item = S>) -> Result<String> {
    let args: Vec<S> = args.into_iter().collect();
    let shown = (args.iter())
        .map(|it| it.as_ref().to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    let output = Command::new("git")
        .args(&args)
        .current_dir(dir)
        .output()
        .map_err(|err| Error::failed(format!("cannot run `git {shown}`: {err}")))?;
    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr);
        return Err(Error::failed(format!(
            "`git {shown}` failed in {}: {}",
            dir.display(),
            said.trim_end()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
