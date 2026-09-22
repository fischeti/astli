//! The corpus tests: the M4 gate. Each skips, and says so, when `corpus/` has
//! not been fetched.

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
