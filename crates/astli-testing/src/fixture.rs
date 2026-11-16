//! A temporary directory a test writes its files into.

use std::path::{Path, PathBuf};

/// A directory of its own under the system's temporary directory, removed when
/// dropped.
pub struct Fixture(PathBuf);

impl Fixture {
    /// An empty directory. The process id keeps two test binaries running at
    /// once apart, and `name` the tests of one.
    pub fn new(name: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("astli-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        Fixture(dir)
    }

    /// Writes `text` at `name` under the fixture, with the directories above it.
    pub fn file(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("a directory for the file");
        }
        std::fs::write(&path, text).expect("a written fixture");
        path
    }

    /// Makes the directory `name` under the fixture.
    pub fn dir(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(&path).expect("a directory");
        path
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
