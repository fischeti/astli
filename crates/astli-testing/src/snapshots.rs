//! A directory of cases, each a `.sv` with the files its test writes beside
//! it, compared with what the test produces.
//!
//! Adding a case means writing the `.sv` and running the test with
//! `UPDATE_EXPECT=1`, which writes every snapshot that is missing or differs.
//! The diff is the review.

use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use expect_test::expect_file;

/// One `.sv` under a directory of cases.
pub struct Case {
    /// Its path from that directory, which is what a failure names.
    pub name: String,
    pub path: PathBuf,
}

impl Case {
    pub fn text(&self) -> String {
        std::fs::read_to_string(&self.path).expect("a case is UTF-8")
    }
}

/// The cases under a directory, and what else it holds.
pub struct Cases {
    root: PathBuf,
    pub cases: Vec<Case>,
    /// Everything that is neither a case nor a snapshot of one, so that
    /// deleting a case cannot leave its snapshot behind.
    pub strays: Vec<String>,
}

/// Every `.sv` under `root`, sorted, in a directory of any depth. A file
/// with one of `snapshots` for its extension is allowed if a `.sv` stands
/// beside it.
pub fn cases(root: &Path, snapshots: &[&str]) -> Cases {
    let mut found = Cases {
        root: root.to_path_buf(),
        cases: Vec::new(),
        strays: Vec::new(),
    };
    walk(root, root, snapshots, &mut found);
    found
}

fn walk(root: &Path, dir: &Path, snapshots: &[&str], found: &mut Cases) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("a readable directory")
        .map(|entry| entry.expect("a readable entry").path())
        .collect();
    entries.sort();
    for path in entries {
        match path.extension().and_then(|ext| ext.to_str()) {
            _ if path.is_dir() => walk(root, &path, snapshots, found),
            Some("sv") => found.cases.push(Case {
                name: path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .display()
                    .to_string(),
                path,
            }),
            Some(ext) if snapshots.contains(&ext) && path.with_extension("sv").exists() => {}
            _ => found.strays.push(path.display().to_string()),
        }
    }
}

impl Cases {
    /// Runs `check` on every case. One failure must not hide the rest, so each
    /// is caught and its panic message, the diff, is left to the default hook
    /// to print. Then it fails if anything was `strays`, described by `what`,
    /// or any case failed.
    pub fn run(&self, what: &str, check: impl Fn(&Case)) {
        assert!(
            !self.cases.is_empty(),
            "no cases under {}",
            self.root.display()
        );
        let failed: Vec<&str> = (self.cases.iter())
            .filter(|case| panic::catch_unwind(AssertUnwindSafe(|| check(case))).is_err())
            .map(|case| case.name.as_str())
            .collect();
        assert!(
            self.strays.is_empty(),
            "{what}:\n{}",
            self.strays.join("\n")
        );
        assert!(
            failed.is_empty(),
            "{} of {} cases differ (UPDATE_EXPECT=1 to accept):\n{}",
            failed.len(),
            self.cases.len(),
            failed.join("\n")
        );
    }
}

/// Holds every `.sv` under `root` to the `.<extension>` beside it, which is
/// what `render` makes of the case's name and text.
pub fn check(root: &Path, extension: &str, render: impl Fn(&str, String) -> String) {
    cases(root, &[extension]).run(&format!("not a case or a case's `.{extension}`"), |case| {
        let actual = render(&case.name, case.text());
        expect_file![case.path.with_extension(extension)].assert_eq(&actual);
    });
}
