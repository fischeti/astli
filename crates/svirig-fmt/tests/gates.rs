//! The corpus tests: the M4 gate. Each skips, and says so, when `corpus/` has
//! not been fetched.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use svirig_fmt::format;
use svirig_parse::SyntaxTree;

mod corpus;

/// Every corpus file is formatted without a refusal, and formatting the result
/// again changes nothing.
#[test]
fn corpus_formats_transparently_and_idempotently() {
    let Some(files) = corpus::files() else {
        return;
    };

    let mut formatted = 0usize;
    let mut refused = Vec::new();
    let mut unstable = Vec::new();

    for path in &files {
        let Ok(tree) = SyntaxTree::read(path) else {
            continue; // not UTF-8; not ours to format
        };
        let at = |offset| format!("{}:{}", path.display(), tree.line_col(offset));

        let once = match format(&tree) {
            Ok(text) => text,
            Err(refusal) => {
                refused.push(format!("{}: {refusal}", at(refusal.offset)));
                continue;
            }
        };
        formatted += 1;

        match format(&SyntaxTree::parse(path, once.clone())) {
            Ok(twice) if twice == once => {}
            Ok(_) => unstable.push(path.display().to_string()),
            Err(refusal) => unstable.push(format!("{} (refused: {refusal})", path.display())),
        }
    }

    eprintln!("{formatted} files formatted");
    let first = |of: &[String]| of.iter().take(10).cloned().collect::<Vec<_>>().join("\n");
    assert!(
        refused.is_empty(),
        "{} files refused, first few:\n{}",
        refused.len(),
        first(&refused)
    );
    assert!(
        unstable.is_empty(),
        "{} files changed when formatted again, first few:\n{}",
        unstable.len(),
        first(&unstable)
    );
}

/// `slang` parses every corpus file to the same syntax tree before and after
/// formatting.
///
/// An independent parser, so a whitespace change that alters what the grammar
/// reads is caught even where the preprocessed tokens stay the same. Trivia is
/// left out of the comparison, since moving it is the formatter's job, and a
/// file `slang` rejects must be rejected alike: the tree it recovers is
/// compared too.
#[test]
fn corpus_slang_parses_alike_before_and_after() {
    let Some(files) = corpus::files() else {
        return;
    };
    if Command::new("slang").arg("--version").output().is_err() {
        eprintln!("skipping: slang is not on PATH");
        return;
    }

    let scratch = std::env::temp_dir().join(format!("svirig-fmt-slang-{}", std::process::id()));
    let next = AtomicUsize::new(0);
    let compared = AtomicUsize::new(0);
    let rejected = AtomicUsize::new(0);
    let disagreed = Mutex::new(Vec::new());

    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|scope| {
        for worker in 0..workers {
            let dir = scratch.join(worker.to_string());
            let (files, next, compared, rejected, disagreed) =
                (&files, &next, &compared, &rejected, &disagreed);
            scope.spawn(move || {
                std::fs::create_dir_all(&dir).unwrap();
                while let Some(path) = files.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let Ok(tree) = SyntaxTree::read(path) else {
                        continue;
                    };
                    let Ok(formatted) = format(&tree) else {
                        continue; // the test above reports refusals
                    };
                    if formatted == tree.source() {
                        compared.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    // The copy keeps the original's name, and both are named
                    // relative to where slang runs, so `__FILE__` expands
                    // alike.
                    let (near, name) = (path.parent().unwrap(), path.file_name().unwrap());
                    std::fs::write(dir.join(name), &formatted).unwrap();
                    let before = slang(near, name, near);
                    let after = slang(&dir, name, near);
                    std::fs::remove_file(dir.join(name)).unwrap();
                    compared.fetch_add(1, Ordering::Relaxed);
                    if !before.0 {
                        rejected.fetch_add(1, Ordering::Relaxed);
                    }
                    if before != after {
                        disagreed.lock().unwrap().push(path.display().to_string());
                    }
                }
            });
        }
    });
    let _ = std::fs::remove_dir_all(&scratch);

    let disagreed = disagreed.into_inner().unwrap();
    eprintln!(
        "{} files compared, {} of them rejected by slang alike",
        compared.into_inner(),
        rejected.into_inner()
    );
    assert!(
        disagreed.is_empty(),
        "{} files parse differently once formatted, first few:\n{}",
        disagreed.len(),
        disagreed
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Whether `slang`, run in `dir`, accepts the file `name` there, and the syntax
/// tree it builds, trivia left out. A quoted include resolves from `near`
/// first, where the original sits, so a formatted copy elsewhere finds the
/// same headers.
///
/// Integer literals are masked, since `__LINE__` expands to one and formatting
/// moves lines. That hides nothing: a literal written in the file cannot
/// change without failing the transparency check.
fn slang(dir: &Path, name: &OsStr, near: &Path) -> (bool, String) {
    let output = Command::new("slang")
        .current_dir(dir)
        .args([
            "--parse-only",
            "-q",
            "--cst-json",
            "-",
            "--cst-json-mode",
            "no-trivia",
        ])
        .arg("--incdir-first")
        .arg("-I")
        .arg(near)
        .arg(name)
        .output()
        .unwrap();
    let tree = String::from_utf8_lossy(&output.stdout);
    let mut masked = String::with_capacity(tree.len());
    let mut literal = false;
    for line in tree.lines() {
        masked.push_str(if literal { "" } else { line });
        masked.push('\n');
        literal = line.trim() == r#""kind": "IntegerLiteral","#;
    }
    (output.status.success(), masked)
}
