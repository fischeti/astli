//! Git LFS pointers: the few lines a commit holds in place of a file stored
//! with Git LFS, which stand in for it in a checkout that did not fetch it.

use std::io;
use std::path::Path;

/// What a Git LFS pointer begins with.
const POINTER: &str = "version https://git-lfs.github.com/spec/v1";

/// Reads the source file at `path`: its text, or why it cannot be read. A
/// Git LFS pointer is an error rather than text, since it would otherwise
/// be read as a file that does not parse, with no word of why, saying what
/// fetches the file: `git lfs pull` in a git working copy, and outside one,
/// which a dependency's checkout is, asking for its LFS files.
pub fn read_source(path: &Path) -> io::Result<String> {
    let text = std::fs::read_to_string(path)?;
    if !text.starts_with(POINTER) {
        return Ok(text);
    }
    let absolute = std::path::absolute(path)?;
    let working = absolute.ancestors().any(|dir| dir.join(".git").exists());
    let fetch = match working {
        true => "`git lfs pull` fetches it",
        false => "to fetch a dependency's, set `lfs = true` on its requirement",
    };
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("a Git LFS pointer, not the file it stands for, which was never fetched: {fetch}"),
    ))
}

/// The SHA-256 of the object the Git LFS pointer `text` names, in hex, if
/// it is one.
pub fn lfs_object(text: &[u8]) -> Option<&str> {
    let text = std::str::from_utf8(text).ok()?;
    if !text.starts_with(POINTER) {
        return None;
    }
    let oid = text
        .lines()
        .find_map(|line| line.strip_prefix("oid sha256:"))?;
    (oid.len() == 64 && oid.bytes().all(|it| it.is_ascii_hexdigit())).then_some(oid)
}
