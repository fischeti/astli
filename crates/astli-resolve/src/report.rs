//! What a store says as it fetches and checks out, for a driver to show.

use std::fmt;

use gix::ObjectId;
use semver::Version;

/// Told of each step that reaches the network or writes a checkout, so that
/// a driver can show what a run is waiting on. Steps come one at a time,
/// each begun and then ended, whether it succeeded or not.
pub trait Report: Send + Sync + fmt::Debug {
    fn begin(&self, step: &Step<'_>);
    fn end(&self, step: &Step<'_>);
}

/// One step of getting a dependency's files.
#[derive(Debug, Clone, Copy)]
pub enum Step<'a> {
    /// Fetching the repository at the URL into its mirror, made first if
    /// there is none, for the dependency named. A mirror is bare, so its
    /// clone is a fetch too.
    Fetching { name: &'a str, url: &'a str },
    /// Writing out the files of a package's commit.
    CheckingOut {
        name: &'a str,
        version: Option<&'a Version>,
        commit: ObjectId,
    },
}

/// Reports nothing: the store's own until a driver gives it another.
#[derive(Debug)]
pub struct Silent;

impl Report for Silent {
    fn begin(&self, _: &Step<'_>) {}
    fn end(&self, _: &Step<'_>) {}
}

/// Runs `doing` as `step`, begun and ended on `report`.
pub(crate) fn step<T>(report: &dyn Report, step: Step<'_>, doing: impl FnOnce() -> T) -> T {
    report.begin(&step);
    let done = doing();
    report.end(&step);
    done
}
