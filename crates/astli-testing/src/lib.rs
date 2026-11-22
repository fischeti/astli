//! What the workspace's tests and examples share, so that each holds it once.
//!
//! It depends on no `astli-*` crate. A crate's own tests link it as a
//! dev-dependency, and one that depended back would be built twice, with the
//! types of the two builds never agreeing.

pub mod corpus;
pub mod fixture;
pub mod snapshots;

pub use fixture::Fixture;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Files held in memory, `(path, text)` each, for a reader to find.
pub fn files(files: &[(&str, &str)]) -> HashMap<PathBuf, String> {
    (files.iter())
        .map(|(path, text)| (PathBuf::from(path), text.to_string()))
        .collect()
}

/// The variables naming a repository and what is inside it, which `git`
/// clears itself before it works in another repository: what `git rev-parse
/// --local-env-vars` prints. `git` sets `GIT_DIR` for a hook or a `rebase
/// --exec` it runs, and a test run from one must not reach that repository.
pub const REPOSITORY_ENV: [&str; 15] = [
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

/// `git` to run in `dir` and no repository the test was started in, reading
/// `config` as the user's configuration and none of the system's, and
/// committing as a fixed author.
pub fn git(dir: &Path, config: &Path) -> Command {
    let mut command = Command::new("git");
    for name in REPOSITORY_ENV {
        command.env_remove(name);
    }
    command
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "astli")
        .env("GIT_AUTHOR_EMAIL", "astli@example.com")
        .env("GIT_COMMITTER_NAME", "astli")
        .env("GIT_COMMITTER_EMAIL", "astli@example.com");
    command
}
