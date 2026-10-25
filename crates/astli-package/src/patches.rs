//! The dependencies a root reads from a directory of this machine's instead
//! of where its requirements say, written by `astli patch`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use astli_text::clean;
use serde::Deserialize;

use crate::Error;

/// The directory beside the root's manifest that holds what is this
/// machine's alone. It ignores itself, so that nothing in it is committed.
pub const LOCAL: &str = ".astli";

/// The file in [`LOCAL`] that holds the patches.
pub const PATCHES: &str = "patch.toml";

/// Each patched dependency's directory, by name, absolute.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Patches(pub BTreeMap<String, PathBuf>);

impl Patches {
    /// Where the patches of the package rooted at `root` are kept.
    pub fn path(root: &Path) -> PathBuf {
        root.join(LOCAL).join(PATCHES)
    }

    /// The patches of the package rooted at `root`, none without a file. A
    /// relative path is relative to `root`, and each must be a directory.
    pub fn read(root: &Path) -> Result<Patches, Error> {
        let path = Patches::path(root);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Patches::default());
            }
            Err(err) => return Err(Error::Io(path, err)),
        };
        let wrong = |what: String| Error::Manifest(path.clone(), what);
        let file: File =
            toml::from_str(&text).map_err(|err| wrong(err.to_string().trim_end().to_string()))?;
        let mut patches = BTreeMap::new();
        for (name, entry) in file.patch {
            let dir = clean(&root.join(&entry.path));
            if !dir.is_dir() {
                return Err(wrong(format!(
                    "`patch.{name}.path`: `{}` is not a directory",
                    entry.path.display()
                )));
            }
            patches.insert(name, dir);
        }
        Ok(Patches(patches))
    }

    /// Writes the patches for the package rooted at `root`, and the
    /// `.gitignore` that keeps [`LOCAL`] out of its repository.
    pub fn write(&self, root: &Path) -> Result<(), Error> {
        let local = root.join(LOCAL);
        std::fs::create_dir_all(&local).map_err(|err| Error::Io(local.clone(), err))?;
        let ignore = local.join(".gitignore");
        if !ignore.is_file() {
            std::fs::write(&ignore, "*\n").map_err(|err| Error::Io(ignore, err))?;
        }
        let mut text = String::from(
            "# What this machine reads each dependency named from, written by `astli patch`.\n\
             [patch]\n",
        );
        for (name, dir) in &self.0 {
            let dir = toml::Value::String(dir.display().to_string());
            text.push_str(&format!("{name} = {{ path = {dir} }}\n"));
        }
        let path = Patches::path(root);
        std::fs::write(&path, text).map_err(|err| Error::Io(path, err))
    }
}

/// The file as written.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct File {
    patch: BTreeMap<String, Entry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: PathBuf,
}
