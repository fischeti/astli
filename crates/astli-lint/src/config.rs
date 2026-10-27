//! Which rules run, at what level, and with what options.

use std::fmt;

use crate::options::{OptionError, Value, either};
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

/// A level for every rule, and a value for each of its options. The
/// default level is each group's, and an option's what the rule declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// One per rule, in the order of [`RULES`].
    levels: Vec<Level>,
    /// One list per rule, in the order of [`RULES`], of a value per option
    /// in the order the rule declares them.
    options: Vec<Vec<Value>>,
}

impl Default for Config {
    fn default() -> Config {
        let options = (RULES.iter())
            .map(|rule| {
                (rule.options.iter())
                    .map(|option| {
                        Value::parse(option.kind, option.default)
                            .unwrap_or_else(|err| panic!("`{}`: {err}", rule.name))
                    })
                    .collect()
            })
            .collect();
        Config {
            levels: RULES.iter().map(|rule| rule.group.level()).collect(),
            options,
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

    /// Whether a rule that reads a design is on, so that there is a reason
    /// to build one.
    pub fn needs_design(&self) -> bool {
        (RULES.iter().zip(&self.levels))
            .any(|(rule, level)| rule.needs_design() && *level != Level::Allow)
    }

    /// Whether a rule that reads what tops build is on, so that there is a
    /// reason to elaborate.
    pub fn needs_tops(&self) -> bool {
        (RULES.iter().zip(&self.levels))
            .any(|(rule, level)| rule.needs_tops() && *level != Level::Allow)
    }

    /// Sets the option `key` of the rule called `rule` to `values`, as
    /// configuration spells them, in place of what it was.
    pub fn set_option<S: AsRef<str>>(
        &mut self,
        rule: &str,
        key: &str,
        values: &[S],
    ) -> Result<(), OptionError> {
        let Some(at) = RULES.iter().position(|it| it.name == rule) else {
            return Err(OptionError(format!("no lint rule is called `{rule}`")));
        };
        let declared = RULES[at].options;
        let Some(option) = declared.iter().position(|it| it.key == key) else {
            let keys: Vec<String> = declared.iter().map(|it| format!("`{}`", it.key)).collect();
            return Err(OptionError(match keys.as_slice() {
                [] => format!("`{rule}` has no options"),
                _ => format!(
                    "`{rule}` has no option `{key}`, only {}",
                    either(&keys, "and")
                ),
            }));
        };
        let value = Value::parse(declared[option].kind, values)
            .map_err(|err| OptionError(format!("`{key}`: {err}")))?;
        self.options[at][option] = value;
        Ok(())
    }

    pub(crate) fn levels(&self) -> &[Level] {
        &self.levels
    }

    pub(crate) fn options(&self) -> &[Vec<Value>] {
        &self.options
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
