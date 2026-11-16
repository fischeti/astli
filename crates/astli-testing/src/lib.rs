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
use std::path::PathBuf;

/// Files held in memory, `(path, text)` each, for a reader to find.
pub fn files(files: &[(&str, &str)]) -> HashMap<PathBuf, String> {
    (files.iter())
        .map(|(path, text)| (PathBuf::from(path), text.to_string()))
        .collect()
}
