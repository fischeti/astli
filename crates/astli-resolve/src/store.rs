//! The cache of mirrors and checkouts every design on the machine shares.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::{Error, Mirror};

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
}

impl Store {
    /// The store at `root`, made on first use.
    pub fn at(root: impl Into<PathBuf>) -> Store {
        Store { root: root.into() }
    }

    /// The store the environment names: `ASTLI_CACHE_DIR`, or else `astli`
    /// in the user's cache directory, `XDG_CACHE_HOME` or `~/.cache`, and
    /// `%LOCALAPPDATA%` on Windows.
    pub fn from_env() -> Result<Store, Error> {
        cache_dir(|name| std::env::var_os(name))
            .map(Store::at)
            .ok_or(Error::NoStore)
    }

    /// The mirror of the repository at `url`, cloned if the store has none.
    /// It stays locked until dropped, so that two runs never fetch into one
    /// mirror at once; opening it again before then waits for ever.
    pub fn mirror(&self, url: &str) -> Result<Mirror, Error> {
        Mirror::open(self, url)
    }
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

/// `url` as compared: without a trailing `/` or `.git`, scheme and host in
/// lower case. Manifests write one repository both ways, and a forge
/// ignores the difference.
pub fn canonical(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    format!("{}://{}/{path}", scheme.to_lowercase(), host.to_lowercase())
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
        let written = "https://GitHub.com/pulp-platform/common_cells.git/";

        assert_eq!(canonical(written), plain);
        let key = |url| key(url).unwrap();
        assert_eq!(key(written), key(plain));
        assert!(key(plain).starts_with("common_cells-"), "{}", key(plain));
        assert_eq!(key(plain).len(), "common_cells-".len() + 16);
        assert_ne!(key(plain), key("https://github.com/fork/common_cells"));
        assert_eq!(canonical("git@github.com:a/b.git"), "git@github.com:a/b");
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
