//! The table [`docs/grammar-coverage.md`](../../../docs/grammar-coverage.md)
//! records at the end of a milestone, in one walk of the corpus.
//!
//!     cargo run --release --example metrics
//!
//! # Why an example and not the tests that assert these things
//!
//! Two of these numbers are asserted elsewhere and neither assertion can print
//! this table. `corpus_verbatim_rate_does_not_rise` reports per repo because
//! that is the same walk as its own assertion -- duplicating *that* would be
//! what [D11](../../../docs/plan.md) argues against -- but it knows nothing
//! about regions, and wall-clock is not something a test may assert at all:
//! a number that fails on a loaded machine is a number that gets deleted.
//!
//! What this adds is the two things a per-test report cannot have. It
//! **deduplicates**: `cva6` vendors `common_cells`, and both `cva6` and `ibex`
//! vendor `lowrisc_ip`, so a pooled number that does not hash file contents
//! counts a thousand files twice. And it **prints the commits** beside the
//! numbers, read out of `corpus/MANIFEST`, because `corpus/` is gitignored and
//! overwritten by the next fetch -- so a figure quoted in a document has to
//! carry its own provenance rather than pointing at whatever is on disk.

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rowan::NodeOrToken;
use svirig_parse::{Raw, SyntaxTree, Tokens};
use svirig_preproc::Session;
use svirig_syntax::{SyntaxKind::*, SyntaxNode};

/// What one repository, or the whole corpus, came to.
#[derive(Default, Clone)]
struct Tally {
    files: usize,
    bytes: usize,
    tokens: usize,
    verbatim: usize,
    regions: usize,
    live: usize,
    parsing: Duration,
}

impl Tally {
    fn add(&mut self, other: &Tally) {
        self.files += other.files;
        self.bytes += other.bytes;
        self.tokens += other.tokens;
        self.verbatim += other.verbatim;
        self.regions += other.regions;
        self.live += other.live;
        self.parsing += other.parsing;
    }

    fn share(part: usize, whole: usize) -> f64 {
        match whole {
            0 => 0.0,
            _ => 100.0 * part as f64 / whole as f64,
        }
    }

    fn row(&self, name: &str, commit: &str) -> String {
        format!(
            "| `{name}` | `{commit}` | {} | {} | {:.2}% | {} | {:.1}% | {:.1} |",
            self.files,
            self.tokens,
            Tally::share(self.verbatim, self.tokens),
            self.regions,
            Tally::share(self.live, self.regions),
            self.bytes as f64 / 1_000_000.0 / self.parsing.as_secs_f64(),
        )
    }
}

/// Everything one file contributes.
fn measure(path: &Path, text: String) -> Tally {
    let mut tally = Tally {
        files: 1,
        bytes: text.len(),
        ..Tally::default()
    };

    let started = Instant::now();
    let tree = SyntaxTree::parse(path, text);
    tally.parsing = started.elapsed();

    // The same count the ratchet asserts on: grammar tokens inside a
    // `VERBATIM`, against grammar tokens in all.
    fn walk(node: &SyntaxNode, inside: bool, tally: &mut Tally) {
        let inside = inside || node.kind() == VERBATIM;
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => walk(&node, inside, tally),
                NodeOrToken::Token(token) if !token.kind().is_trivia() => {
                    tally.tokens += 1;
                    tally.verbatim += usize::from(inside);
                }
                NodeOrToken::Token(_) => {}
            }
        }
    }
    walk(tree.root(), false, &mut tally);

    // Asked of the stream rather than read off the tree, because a region
    // inside a `` `define `` body has a shape and never becomes a node.
    let mut session = Session::new();
    let file = session.add("", tree.source().to_owned());
    let mut raw = Raw::new(session.input(file));
    loop {
        if let Some(shape) = raw.region() {
            tally.regions += 1;
            tally.live += usize::from(shape.live);
        }
        if raw.at_end() {
            break;
        }
        raw.bump();
    }

    tally
}

/// The resolved commit of each repository, as the last fetch wrote it.
fn manifest(corpus: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(corpus.join("MANIFEST")) else {
        return Vec::new();
    };
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let mut fields = line.split('\t');
            Some((fields.next()?.to_string(), fields.next()?.to_string()))
        })
        .collect()
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        // A tool's own copies of sources, such as `.git` or `.bender`, are
        // not part of the pinned corpus.
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
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

fn main() {
    let corpus = Path::new("corpus");
    if !corpus.is_dir() {
        eprintln!("no corpus/ -- run scripts/fetch-corpus.sh");
        std::process::exit(1);
    }

    let commits = manifest(corpus);
    let mut repos: Vec<PathBuf> = std::fs::read_dir(corpus)
        .expect("corpus/")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    repos.sort();

    println!(
        "| Repo | Commit | Files | Tokens | Verbatim | Regions | Live | MB/s |\n\
         | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |"
    );

    let mut pooled = Tally::default();
    let mut seen: HashSet<u64> = HashSet::new();
    let mut duplicates = 0usize;

    for repo in &repos {
        let name = repo.file_name().unwrap().to_string_lossy().to_string();
        let commit = commits
            .iter()
            .find(|(repo, _)| *repo == name)
            .map(|(_, commit)| commit[..10].to_string())
            .unwrap_or_else(|| "?".to_string());

        let mut here = Tally::default();
        let mut paths = Vec::new();
        files(repo, &mut paths);
        paths.sort();

        for path in &paths {
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            let mut hasher = DefaultHasher::new();
            text.hash(&mut hasher);
            let digest = hasher.finish();

            let tally = measure(path, text);
            here.add(&tally);
            // A vendored copy belongs to the repo that vendors it and to the
            // pooled number once.
            match seen.insert(digest) {
                true => pooled.add(&tally),
                false => duplicates += 1,
            }
        }

        println!("{}", here.row(&name, &commit));
    }

    println!("{}", pooled.row("all, deduplicated", "—"));
    println!("\n{duplicates} duplicate files not counted twice.");
    println!(
        "Parsed in {:.2}s, {:.0}k tokens/s.",
        pooled.parsing.as_secs_f64(),
        pooled.tokens as f64 / pooled.parsing.as_secs_f64() / 1000.0
    );
}
