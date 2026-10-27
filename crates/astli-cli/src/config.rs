//! `astli.toml`: the lint levels a project sets, for all of it and per path,
//! the options of its rules, and the layout its formatting takes and the
//! files it leaves alone.
//!
//! One file serves a run: the one `--config` names, or else the manifest of
//! the package the current directory is in. Paths in it are relative to it,
//! so it can live at a repository's root and be run from anywhere below.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use astli_fmt::Options;
use astli_lint::{Config, Group, Level};
use astli_package::{Package, mark};
use globset::{GlobBuilder, GlobMatcher, GlobSet, GlobSetBuilder};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use usage_rs::Args;

use crate::error::{Error, Result};
use crate::term;

/// Severity overrides for lint rules and groups.
#[derive(Args, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Levels {
    /// Silence a lint rule or entire rule group (can be repeated)
    #[usage(short = 'A', long, value_name = "RULE")]
    pub allow: Vec<String>,
    /// Set a lint rule or entire rule group to emit warnings (can be repeated)
    #[usage(short = 'W', long, value_name = "RULE")]
    pub warn: Vec<String>,
    /// Set a lint rule or entire rule group to emit fatal errors (can be repeated)
    #[usage(short = 'D', long, value_name = "RULE")]
    pub deny: Vec<String>,
}

impl Levels {
    /// Sets each name in `config`. Groups go first, so a rule named on its
    /// own wins over its group whichever list names it; among lists, the
    /// stricter wins.
    pub fn apply(&self, config: &mut Config) -> std::result::Result<(), astli_lint::UnknownRule> {
        let lists = [
            (&self.allow, Level::Allow),
            (&self.warn, Level::Warn),
            (&self.deny, Level::Deny),
        ];
        let is_group = |name: &&String| Group::named(name).is_some();
        // Groups first, so that a rule named on its own wins over its group.
        for (names, level) in lists {
            for name in names.iter().filter(|name| is_group(name)) {
                config.set(name, level)?;
            }
        }
        for (names, level) in lists {
            for name in names.iter().filter(|name| !is_group(name)) {
                config.set(name, level)?;
            }
        }
        Ok(())
    }
}

/// What a project sets of [`Options`], the rest keeping their defaults,
/// and the files it leaves as they are.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
struct Fmt {
    width: Option<usize>,
    indent: Option<usize>,
    max_pad: Option<usize>,
    exclude: Vec<String>,
}

/// A project's formatting configuration.
pub struct FmtConfig {
    /// The layout, over the defaults.
    pub options: Options,
    /// The directory `exclude` is relative to.
    root: PathBuf,
    exclude: GlobSet,
}

impl FmtConfig {
    /// Reads `explicit`, or the manifest of the package the current
    /// directory is in. With neither, the defaults, and no file excluded.
    pub fn load(explicit: Option<&Path>) -> Result<FmtConfig> {
        let read = read(explicit)?;
        let Fmt {
            width,
            indent,
            max_pad,
            exclude,
        } = read.table("fmt", |package| &package.fmt)?;
        let default = Options::default();
        let options = Options {
            width: width.unwrap_or(default.width),
            indent: indent.unwrap_or(default.indent),
            max_pad: max_pad.unwrap_or(default.max_pad),
        };

        let wrong = |what: String| {
            let key = mark::code("exclude");
            Error::failed(format!("{}: [fmt]: {key}: {what}", read.name))
        };
        let mut set = GlobSetBuilder::new();
        for pattern in &exclude {
            let glob = GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .map_err(|err| wrong(format!("{}: {err}", mark::code(pattern))))?;
            set.add(glob);
        }
        let exclude = set.build().map_err(|err| wrong(err.to_string()))?;

        Ok(FmtConfig {
            options,
            root: read.root(),
            exclude,
        })
    }

    /// Whether `exclude` names `file` or a directory it is in, by its path
    /// relative to the configuration, as a package's `exclude` does.
    pub fn excludes(&self, file: &Path) -> bool {
        let Ok(absolute) = std::path::absolute(file) else {
            return false;
        };
        let Ok(relative) = absolute.strip_prefix(&self.root) else {
            return false;
        };
        (relative.ancestors())
            .take_while(|it| !it.as_os_str().is_empty())
            .any(|it| self.exclude.is_match(it))
    }
}

/// A configuration as read, with what explains it.
struct Read {
    /// The package whose manifest it is, if any.
    package: Option<Package>,
    /// What a message about it calls it.
    name: String,
}

/// Reads `explicit`, or the manifest of the package the current directory
/// is in. With neither, every table is empty.
fn read(explicit: Option<&Path>) -> Result<Read> {
    let package = match explicit {
        Some(path) => Some(Package::read(path)?),
        None => {
            let here = std::env::current_dir().map_err(|err| Error::io(".", err))?;
            Package::find(&here)?
        }
    };
    let name = (package.as_ref())
        .and_then(|package| package.manifest.as_deref())
        .unwrap_or(Path::new(astli_package::MANIFEST))
        .display()
        .to_string();
    Ok(Read { package, name })
}

impl Read {
    /// The manifest's table `key`, which `table` picks, as `T`.
    fn table<T: DeserializeOwned + Default>(
        &self,
        key: &str,
        table: fn(&Package) -> &toml::Table,
    ) -> Result<T> {
        let Some(package) = &self.package else {
            return Ok(T::default());
        };
        toml::Value::Table(table(package).clone())
            .try_into()
            // A TOML error ends in a newline, which a message does not.
            .map_err(|err| {
                let err = err.to_string();
                Error::failed(format!("{}: [{key}]: {}", self.name, err.trim_end()))
            })
    }

    /// The directory paths in the manifest are relative to.
    fn root(&self) -> PathBuf {
        (self.package.as_ref())
            .map(|package| package.root.clone())
            .unwrap_or_default()
    }
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct Lint {
    // Spelled out rather than a flattened `Levels`: flattening loses where
    // an unknown key is and which keys were expected.
    allow: Vec<String>,
    warn: Vec<String>,
    deny: Vec<String>,
    /// Kept as a table so that the patterns apply in the order written.
    paths: toml::Table,
    /// Each rule's options, by its name, and each option's values by its
    /// key.
    rules: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

/// Sets each option `rules` holds in `config`, over what it was.
fn set_options(
    rules: &BTreeMap<String, BTreeMap<String, Vec<String>>>,
    config: &mut Config,
) -> std::result::Result<(), String> {
    for (rule, options) in rules {
        for (key, values) in options {
            config
                .set_option(rule, key, values)
                .map_err(|err| format!("{}: {err}", mark::code(format!("[lint.rules.{rule}]"))))?;
        }
    }
    Ok(())
}

/// A project's lint configuration, ready to answer for each file.
pub struct LintConfig {
    /// The levels for every file: the file's, then the command line's.
    all: Config,
    /// The directory paths are relative to.
    root: PathBuf,
    /// Each pattern's levels, which apply after `all`, in order.
    paths: Vec<(GlobMatcher, Levels)>,
}

impl LintConfig {
    /// Reads `explicit`, or the manifest of the package the current
    /// directory is in, then applies `flags` over its levels. With no file,
    /// the flags apply over the defaults.
    pub fn load(explicit: Option<&Path>, flags: &Levels) -> Result<LintConfig> {
        let read = read(explicit)?;
        let root = read.root();
        let name = &read.name;
        let wrong = |what: String| Error::failed(format!("{name}: {what}"));

        let Lint {
            allow,
            warn,
            deny,
            paths,
            rules,
        } = read.table("lint", |package| &package.lint)?;
        let mut all = Config::default();
        // The project's levels and options, over the defaults.
        Levels { allow, warn, deny }
            .apply(&mut all)
            .map_err(|err| wrong(err.to_string()))?;
        set_options(&rules, &mut all).map_err(wrong)?;
        // The flags, over the project's, so that a run can override it.
        flags.apply(&mut all).map_err(|err| {
            term::error(err.to_string()).help(format!("{} names every rule", mark::code("--list")))
        })?;

        let mut matchers = Vec::new();
        for (pattern, levels) in paths {
            let glob = GlobBuilder::new(&pattern)
                .literal_separator(true)
                .build()
                .map_err(|err| wrong(format!("{}: {err}", mark::code(&pattern))))?;
            let levels: Levels = levels
                .try_into()
                .map_err(|err| wrong(format!("{}: {err}", mark::code(&pattern))))?;
            // Checked now, rather than on the first file it matches.
            levels
                .apply(&mut Config::default())
                .map_err(|err| wrong(format!("{}: {err}", mark::code(&pattern))))?;
            matchers.push((glob.compile_matcher(), levels));
        }

        Ok(LintConfig {
            all,
            root,
            paths: matchers,
        })
    }

    /// The levels for `file`: every pattern that matches its path relative
    /// to the configuration, in order, over those for all files.
    pub fn for_file(&self, file: &Path) -> Config {
        let mut config = self.all.clone();
        let Ok(absolute) = std::path::absolute(file) else {
            return config;
        };
        let Ok(relative) = absolute.strip_prefix(&self.root) else {
            return config;
        };
        for (glob, levels) in &self.paths {
            if glob.is_match(relative) {
                // Over the flags, since a pattern is more specific than the
                // whole run, and over earlier patterns. Every name was
                // checked when the file was read.
                let _ = levels.apply(&mut config);
            }
        }
        config
    }
}
