//! What the formatter still writes as it was read, over the corpus: the share
//! of tokens a rule laid out, and the kinds of node that hold the rest, most
//! first. The kinds are where the next rule pays most.
//!
//!     cargo run --release -p svirig-fmt --example unformatted
//!
//! Files are deduplicated by content, since the repositories vendor each
//! other.

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use rustc_hash::FxHashMap;
use svirig_fmt::unformatted;
use svirig_parse::SyntaxTree;
use svirig_syntax::{SyntaxKind, SyntaxNode};

/// Rows of the table by kind.
const KINDS: usize = 20;

fn main() {
    let corpus = Path::new("corpus");
    if !corpus.is_dir() {
        eprintln!("no corpus/ -- run scripts/fetch-corpus.sh");
        std::process::exit(1);
    }
    let mut paths = Vec::new();
    files(corpus, &mut paths);
    paths.sort();

    let mut seen = HashSet::new();
    let mut total = 0usize;
    let mut by_kind: FxHashMap<SyntaxKind, (usize, usize)> = FxHashMap::default();
    for path in &paths {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        if !seen.insert(hasher.finish()) {
            continue;
        }

        let tree = SyntaxTree::parse(path, text);
        total += tokens(tree.root());
        for node in unformatted(&tree) {
            let (nodes, tokens_in) = by_kind.entry(node.kind()).or_default();
            *nodes += 1;
            *tokens_in += tokens(&node);
        }
    }

    let left: usize = by_kind.values().map(|&(_, tokens)| tokens).sum();
    let share = |part: usize| 100.0 * part as f64 / total.max(1) as f64;
    println!(
        "{} files, {total} tokens, {:.1}% laid out by a rule\n",
        seen.len(),
        share(total - left)
    );

    let mut kinds: Vec<_> = by_kind.into_iter().collect();
    kinds.sort_by_key(|&(kind, (_, tokens))| (std::cmp::Reverse(tokens), kind));
    println!("| Kind | Nodes | Tokens | Share |\n| --- | ---: | ---: | ---: |");
    for (kind, (nodes, tokens)) in kinds.into_iter().take(KINDS) {
        println!(
            "| `{kind:?}` | {nodes} | {tokens} | {:.1}% |",
            share(tokens)
        );
    }
}

/// Tokens other than whitespace and comments.
fn tokens(node: &SyntaxNode) -> usize {
    (node.descendants_with_tokens())
        .filter_map(|it| it.into_token())
        .filter(|token| !token.kind().is_trivia())
        .count()
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("sv" | "svh")
        ) {
            out.push(path);
        }
    }
}
