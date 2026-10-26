//! What the root says of a top it is built from: the module, and what that
//! module needs defined, so that no run has to repeat it.

use serde::Deserialize;

/// A `[top.NAME]` table, selected by `--top NAME`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Top {
    /// The module, interface or program elaborated from; the table's own
    /// name when not written.
    pub top: Option<String>,
    /// Each as `-D` takes it, `NAME` or `NAME=VALUE`.
    pub defines: Vec<String>,
}

impl Top {
    /// What the table named `name` elaborates from.
    pub fn module<'a>(&'a self, name: &'a str) -> &'a str {
        self.top.as_deref().unwrap_or(name)
    }
}
