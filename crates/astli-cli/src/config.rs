//! `astli.toml`: the lint levels a project sets, for all of it and per path.
//!
//! One file serves a run: the one `--config` names, or else the first found
//! walking up from the current directory. Paths in it are relative to it, so
//! it can live at a repository's root and be run from anywhere below.

use std::path::{Path, PathBuf};

use astli_lint::{Config, Group, Level};
use globset::{GlobBuilder, GlobMatcher};
use serde::Deserialize;

use crate::error::{Error, Result};

/// The name looked for.
pub const NAME: &str = "astli.toml";

/// Levels by name, as `-A`, `-W` and `-D` give them.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Levels {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub warn: Vec<String>,
    #[serde(default)]
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
        for groups in [true, false] {
            for (names, level) in lists {
                let named = (names.iter()).filter(|name| Group::named(name).is_some() == groups);
                for name in named {
                    config.set(name, level)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    lint: Lint,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Lint {
    #[serde(default)]
    allow: Vec<String>,
    #[serde(default)]
    warn: Vec<String>,
    #[serde(default)]
    deny: Vec<String>,
    /// Kept as a table so that the patterns apply in the order written.
    #[serde(default)]
    paths: toml::Table,
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
    /// Reads `explicit`, or the `astli.toml` found from the current
    /// directory up, then applies `flags` over its levels. With no file, the
    /// flags apply over the defaults.
    pub fn load(explicit: Option<&Path>, flags: &Levels) -> Result<LintConfig> {
        let found = match explicit {
            Some(path) => Some(path.to_path_buf()),
            None => find()?,
        };
        let (file, root) = match &found {
            Some(path) => {
                let text = std::fs::read_to_string(path).map_err(|err| Error::io(path, err))?;
                let file: File = toml::from_str(&text)
                    .map_err(|err| Error::failed(format!("{}: {err}", path.display())))?;
                let root = std::path::absolute(path).map_err(|err| Error::io(path, err))?;
                (
                    file,
                    root.parent().map(Path::to_path_buf).unwrap_or_default(),
                )
            }
            None => (File::default(), PathBuf::new()),
        };
        let name = found
            .as_deref()
            .unwrap_or(Path::new(NAME))
            .display()
            .to_string();
        let wrong = |what: String| Error::failed(format!("{name}: {what}"));

        let Lint {
            allow,
            warn,
            deny,
            paths,
        } = file.lint;
        let mut all = Config::default();
        (Levels { allow, warn, deny }.apply(&mut all)).map_err(|err| wrong(err.to_string()))?;
        (flags.apply(&mut all)).map_err(|err| Error::failed(format!("{err}; see --list")))?;

        let mut matchers = Vec::new();
        for (pattern, levels) in paths {
            let glob = (GlobBuilder::new(&pattern).literal_separator(true).build())
                .map_err(|err| wrong(format!("`{pattern}`: {err}")))?;
            let levels: Levels =
                (levels.try_into()).map_err(|err| wrong(format!("`{pattern}`: {err}")))?;
            // Checked now, rather than on the first file it matches.
            (levels.apply(&mut Config::default()))
                .map_err(|err| wrong(format!("`{pattern}`: {err}")))?;
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
                // Every name was checked when the file was read.
                let _ = levels.apply(&mut config);
            }
        }
        config
    }
}

/// The nearest `astli.toml` in the current directory or above it.
fn find() -> Result<Option<PathBuf>> {
    let here = std::env::current_dir().map_err(|err| Error::io(".", err))?;
    Ok(here
        .ancestors()
        .map(|dir| dir.join(NAME))
        .find(|path| path.is_file()))
}
