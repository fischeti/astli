//! Which rules run, and at what level.

use std::fmt;

use crate::{Group, RULES};

/// How a rule's findings are reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// Not run.
    Allow,
    /// Reported as a warning.
    Warn,
    /// Reported as an error.
    Deny,
}

/// A level for every rule. The default is each group's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// One per rule, in the order of [`RULES`].
    levels: Vec<Level>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            levels: RULES.iter().map(|rule| rule.group.level()).collect(),
        }
    }
}

impl Config {
    /// Sets the rule called `name` to `level`, or every rule of the group
    /// called `name`. A later call overrides an earlier one, so a group set
    /// first and one of its rules after leaves that rule as the second call
    /// says.
    pub fn set(&mut self, name: &str, level: Level) -> Result<(), UnknownRule> {
        let group = Group::named(name);
        let mut found = group.is_some();
        for (rule, slot) in RULES.iter().zip(&mut self.levels) {
            if Some(rule.group) == group || rule.name == name {
                *slot = level;
                found = true;
            }
        }
        match found {
            true => Ok(()),
            false => Err(UnknownRule(name.to_string())),
        }
    }

    /// The level of the rule called `name`, if there is one.
    pub fn level(&self, name: &str) -> Option<Level> {
        let at = RULES.iter().position(|rule| rule.name == name)?;
        Some(self.levels[at])
    }

    pub(crate) fn levels(&self) -> &[Level] {
        &self.levels
    }
}

/// A name that is neither a rule's nor a group's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownRule(pub String);

impl fmt::Display for UnknownRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "no lint rule or group is called `{}`", self.0)
    }
}

impl std::error::Error for UnknownRule {}
