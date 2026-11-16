//! Every `tests/data/**/*.sv`, formatted and compared with the `.out` beside
//! it, then formatted again to check that nothing changes.
//!
//! A case is a file, and the reason it exists is a comment inside it. Adding
//! one means writing the `.sv` and running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run -p astli-fmt snapshots
//! ```
//!
//! which writes every `.out` that is missing or differs. The diff is the
//! review.

use std::path::Path;

use astli_fmt::{Options, format};
use astli_parse::SyntaxTree;
use astli_testing::snapshots;

#[test]
fn snapshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    snapshots::check(&root, "out", |name, text| {
        let once = formatted(name, text);
        assert_eq!(
            formatted(name, once.clone()),
            once,
            "{name}: not idempotent"
        );
        once
    });
}

fn formatted(name: &str, text: String) -> String {
    let tree = SyntaxTree::parse(name, text);
    format(&tree, &Options::default())
        .unwrap_or_else(|refusal| panic!("{name}:{}: {refusal}", tree.line_col(refusal.offset)))
}
