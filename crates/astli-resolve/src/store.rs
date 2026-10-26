//! The cache of mirrors and checkouts every design on the machine shares.

use std::ffi::OsString;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use astli_text::clean;

use crate::{Error, Mirror, Report, Silent};

/// A directory of bare mirrors, one per repository, and of checkouts, one
/// per commit:
///
/// ```text
/// git/db/axi-<hash of the URL>/                    a bare mirror
/// git/checkouts/axi-<hash of the URL>/<commit>/    a commit's files, no .git
/// ```
///
/// Everything in it can be fetched again, so deleting it loses nothing.
#[derive(Debug, Clone)]
pub struct Store {
    pub root: PathBuf,
    /// Told of each clone, fetch and checkout.
    pub(crate) report: Arc<dyn Report>,
}

impl Store {
    /// The store at `root`, made on first use, reporting nothing.
    pub fn at(root: impl Into<PathBuf>) -> Store {
        Store {
            root: root.into(),
            report: Arc::new(Silent),
        }
    }

    /// The store, telling `report` of each clone, fetch and checkout.
    pub fn reporting(self, report: Arc<dyn Report>) -> Store {
        Store { report, ..self }
    }

    /// The store the environment names: `ASTLI_CACHE_DIR`, or else `astli`
    /// in the user's cache directory, `XDG_CACHE_HOME` or `~/.cache`, and
    /// `%LOCALAPPDATA%` on Windows.
    ///
    /// Made absolute and cleaned of `.` and `..`, as a package's root is, so
    /// that a checkout's files are named as a walk of it names them.
    pub fn from_env() -> Result<Store, Error> {
        let dir = cache_dir(|name| std::env::var_os(name)).ok_or(Error::NoStore)?;
        let dir = std::path::absolute(&dir).map_err(|err| Error::Io(dir, err))?;
        Ok(Store::at(clean(&dir)))
    }

    /// The mirror of the repository at `url`, cloned if the store has none.
    /// It stays locked until dropped, so that two runs never fetch into one
    /// mirror at once; opening it again before then waits for ever.
    ///
    /// `name` is the dependency it is opened for, which its fetches are
    /// reported as: what a reader wrote in a manifest, where a URL is only
    /// where it comes from.
    pub fn mirror(&self, name: &str, url: &str) -> Result<Mirror, Error> {
        Mirror::open(self, name, url)
    }

    /// Deletes the store, and says whether there was one. It waits for each
    /// mirror's lock first, so that no run is fetching into a mirror or
    /// checking one out while it goes; a run that reads a checkout holds
    /// no lock, and has it taken away.
    pub fn clean(&self) -> Result<bool, Error> {
        if !self.root.exists() {
            return Ok(false);
        }
        let db = self.root.join("git").join("db");
        let locks: Vec<PathBuf> = (std::fs::read_dir(&db).into_iter().flatten().flatten())
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|it| it == "lock"))
            .collect();
        let mut held = Vec::new();
        for path in &locks {
            let lock = File::create(path).map_err(|err| Error::Io(path.clone(), err))?;
            lock.lock().map_err(|err| Error::Io(path.clone(), err))?;
            held.push(lock);
        }
        // Everything but the locks first, which Windows cannot delete while
        // they are held; then they go too.
        deleted(&self.root, &locks)?;
        drop(held);
        deleted(&self.root, &[])?;
        Ok(true)
    }
}

/// Deletes `path`, a file or a directory, but what `kept` names and the
/// directories holding it. A checkout's files are read-only, which Windows
/// will not delete, so they are made writable first there.
fn deleted(path: &Path, kept: &[PathBuf]) -> Result<(), Error> {
    if kept.iter().any(|it| it == path) {
        return Ok(());
    }
    let failed = |err| Error::Io(path.to_path_buf(), err);
    let metadata = std::fs::symlink_metadata(path).map_err(failed)?;
    if !metadata.is_dir() {
        if cfg!(windows) && metadata.permissions().readonly() {
            let mut permissions = metadata.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            std::fs::set_permissions(path, permissions).map_err(failed)?;
        }
        return std::fs::remove_file(path).map_err(failed);
    }
    for entry in std::fs::read_dir(path).map_err(failed)? {
        deleted(&entry.map_err(failed)?.path(), kept)?;
    }
    if kept.iter().any(|it| it.starts_with(path)) {
        return Ok(());
    }
    std::fs::remove_dir(path).map_err(failed)
}

/// Where the store is by default, `var` reading the environment.
fn cache_dir(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let set = |name| {
        var(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    if let Some(dir) = set("ASTLI_CACHE_DIR") {
        return Some(dir);
    }
    if cfg!(windows) {
        return set("LOCALAPPDATA").map(|dir| dir.join("astli").join("cache"));
    }
    set("XDG_CACHE_HOME")
        .map(|dir| dir.join("astli"))
        .or_else(|| set("HOME").map(|home| home.join(".cache").join("astli")))
}

/// `url` as compared: its host and path in lower case, without a trailing
/// `/` or `.git`, whatever the scheme or user. Manifests write one
/// repository over HTTPS and over SSH, with `.git` and without, and in
/// either case, and a forge serves the same repository to each. A local
/// path is kept as it is.
pub fn canonical(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);
    let (authority, path) = match url.split_once("://") {
        Some((_, rest)) => rest.split_once('/').unwrap_or((rest, "")),
        // `user@host:path`, as scp writes it. A single letter before the
        // colon is a Windows drive.
        None => match url.split_once(':') {
            Some((authority, path)) if authority.len() > 1 && !authority.contains('/') => {
                (authority, path)
            }
            _ => return url.to_string(),
        },
    };
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host);
    format!("{host}/{}", path.trim_start_matches('/')).to_lowercase()
}

/// The directory name the store gives the repository at `url`: its last
/// path component, for whoever looks, and the canonical URL's SHA-1, so
/// that a fork is a mirror of its own. A hash that never changes, so that a
/// store outlives the binary that wrote it.
pub(crate) fn key(url: &str) -> Result<String, Error> {
    let url = canonical(url);
    let last = url
        .rsplit(['/', ':'])
        .find(|it| !it.is_empty())
        .unwrap_or("repo");
    let last: String = (last.chars())
        .map(|c| match c.is_ascii_alphanumeric() || "-_.".contains(c) {
            true => c,
            false => '_',
        })
        .collect();
    let mut hasher = gix::hash::hasher(gix::hash::Kind::Sha1);
    hasher.update(url.as_bytes());
    let hash = (hasher.try_finalize()).map_err(|err| Error::Git(format!("{url}: {err}")))?;
    Ok(format!("{last}-{}", hash.to_hex_with_len(16)))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use super::{cache_dir, canonical, key};

    #[test]
    fn one_repository_written_two_ways_is_one_mirror() {
        let plain = "https://github.com/pulp-platform/common_cells";
        let written = "https://GitHub.com/pulp-platform/Common_Cells.git/";

        assert_eq!(canonical(written), "github.com/pulp-platform/common_cells");
        for ssh in [
            "git@github.com:pulp-platform/common_cells.git",
            "ssh://git@github.com/pulp-platform/common_cells",
            "ssh://git@github.com:22/pulp-platform/common_cells.git",
        ] {
            assert_eq!(canonical(ssh), canonical(plain), "{ssh}");
        }
        assert_eq!(canonical("/srv/git/cells.git"), "/srv/git/cells");
        assert_eq!(canonical(r"C:\git\cells"), r"C:\git\cells");
        let key = |url| key(url).unwrap();
        assert_eq!(key(written), key(plain));
        assert!(key(plain).starts_with("common_cells-"), "{}", key(plain));
        assert_eq!(key(plain).len(), "common_cells-".len() + 16);
        assert_ne!(key(plain), key("https://github.com/fork/common_cells"));
    }

    #[cfg(not(windows))]
    #[test]
    fn the_store_is_where_the_environment_says() {
        let with = |vars: &[(&str, &str)]| {
            let vars: Vec<(String, OsString)> = (vars.iter())
                .map(|(name, value)| (name.to_string(), OsString::from(value)))
                .collect();
            cache_dir(|name| {
                (vars.iter())
                    .find(|(set, _)| set == name)
                    .map(|(_, value)| value.clone())
            })
        };

        let all = [
            ("ASTLI_CACHE_DIR", "/astli"),
            ("XDG_CACHE_HOME", "/xdg"),
            ("HOME", "/home"),
        ];
        assert_eq!(with(&all), Some(PathBuf::from("/astli")));
        assert_eq!(with(&all[1..]), Some(PathBuf::from("/xdg/astli")));
        assert_eq!(with(&all[2..]), Some(PathBuf::from("/home/.cache/astli")));
        assert_eq!(with(&[("XDG_CACHE_HOME", "")]), None);
    }
}
