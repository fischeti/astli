//! `astli.toml`: the lint levels a project sets, for all of it and per path,
//! and the layout its formatting takes.
//!
//! One file serves a run: the one `--config` names, or else the first found
//! walking up from the current directory. Paths in it are relative to it, so
//! it can live at a repository's root and be run from anywhere below.

use std::path::{Path, PathBuf};

use astli_fmt::Options;
use astli_lint::{Config, Group, Level};
use globset::{GlobBuilder, GlobMatcher};
use serde::Deserialize;
use usage::Args;

use crate::error::{Error, Result};

/// The name looked for.
pub const NAME: &str = "astli.toml";

/// Levels by name, from `-A`, `-W` and `-D` or a pattern in `astli.toml`.
#[derive(Args, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Levels {
    /// Turn off a rule, or every rule of a group (can be repeated)
    #[usage(short = 'A', long, value_name = "RULE")]
    pub allow: Vec<String>,
    /// Report a rule, or every rule of a group, as a warning (can be repeated)
    #[usage(short = 'W', long, value_name = "RULE")]
    pub warn: Vec<String>,
    /// Report a rule, or every rule of a group, as an error (can be repeated)
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

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct File {
    lint: Lint,
    fmt: Fmt,
}

/// What a project sets of [`Options`]; the rest keep their defaults.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
struct Fmt {
    width: Option<usize>,
    indent: Option<usize>,
    max_pad: Option<usize>,
}

/// The formatter's options from `explicit`, or the `astli.toml` found from
/// the current directory up, over the defaults.
pub fn fmt_options(explicit: Option<&Path>) -> Result<Options> {
    let Fmt {
        width,
        indent,
        max_pad,
    } = read(explicit)?.file.fmt;
    let default = Options::default();
    Ok(Options {
        width: width.unwrap_or(default.width),
        indent: indent.unwrap_or(default.indent),
        max_pad: max_pad.unwrap_or(default.max_pad),
    })
}

/// A configuration as read, with what explains it.
struct Read {
    file: File,
    /// The directory paths in it are relative to.
    root: PathBuf,
    /// What a message about it calls it.
    name: String,
}

/// Reads `explicit`, or the `astli.toml` found from the current directory
/// up. With neither, the file is empty.
fn read(explicit: Option<&Path>) -> Result<Read> {
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
    Ok(Read { file, root, name })
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
        let Read { file, root, name } = read(explicit)?;
        let wrong = |what: String| Error::failed(format!("{name}: {what}"));

        let Lint {
            allow,
            warn,
            deny,
            paths,
        } = file.lint;
        let mut all = Config::default();
        // The project's levels, over the defaults.
        Levels { allow, warn, deny }
            .apply(&mut all)
            .map_err(|err| wrong(err.to_string()))?;
        // The flags, over the project's, so that a run can override it.
        flags
            .apply(&mut all)
            .map_err(|err| Error::failed(format!("{err}; see --list")))?;

        let mut matchers = Vec::new();
        for (pattern, levels) in paths {
            let glob = GlobBuilder::new(&pattern)
                .literal_separator(true)
                .build()
                .map_err(|err| wrong(format!("`{pattern}`: {err}")))?;
            let levels: Levels = levels
                .try_into()
                .map_err(|err| wrong(format!("`{pattern}`: {err}")))?;
            // Checked now, rather than on the first file it matches.
            levels
                .apply(&mut Config::default())
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
                // Over the flags, since a pattern is more specific than the
                // whole run, and over earlier patterns. Every name was
                // checked when the file was read.
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
