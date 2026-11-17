//! The rules, a module per construct they read.

pub(crate) mod begin;
pub(crate) mod case;
pub(crate) mod declarations;
pub(crate) mod drivers;
pub(crate) mod expressions;
pub(crate) mod files;
pub(crate) mod generate;
pub(crate) mod instances;
pub(crate) mod items;
pub(crate) mod names;
pub(crate) mod preproc;
pub(crate) mod procedural;
pub(crate) mod resets;
pub(crate) mod tokens;
pub(crate) mod types;
pub(crate) mod unused;
pub(crate) mod widths;

/// What `rule` alone finds in `source`, with `options` set.
#[cfg(test)]
pub(crate) fn found(rule: &str, options: &[(&str, &[&str])], source: &str) -> Vec<String> {
    use crate::{Config, Level, lint};

    let mut config = Config::default();
    config.set("correctness", Level::Allow).unwrap();
    config.set("suspicious", Level::Allow).unwrap();
    config.set("lowrisc", Level::Allow).unwrap();
    config.set(rule, Level::Warn).unwrap();
    for (key, values) in options {
        config.set_option(rule, key, values).unwrap();
    }
    let tree = astli_parse::SyntaxTree::parse("top.sv", source.to_string());
    (lint(&tree, &config).into_iter())
        .map(|it| it.message)
        .collect()
}
