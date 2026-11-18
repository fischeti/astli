//! Git LFS pointers: the few lines a commit holds in place of a file stored
//! with Git LFS, which stand in for it in a checkout that did not fetch it.

/// The SHA-256 of the object the Git LFS pointer `text` names, in hex, if
/// it is one.
pub fn lfs_object(text: &[u8]) -> Option<&str> {
    let text = std::str::from_utf8(text).ok()?;
    if !astli_text::is_lfs_pointer(text) {
        return None;
    }
    let oid = text
        .lines()
        .find_map(|line| line.strip_prefix("oid sha256:"))?;
    (oid.len() == 64 && oid.bytes().all(|it| it.is_ascii_hexdigit())).then_some(oid)
}
