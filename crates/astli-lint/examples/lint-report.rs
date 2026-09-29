//! What each lint rule finds over the corpus, with every rule on: how often,
//! in how many files, in which repositories, and how many of the hits carry a
//! verible waiver for the rule of the same name. A hit in OpenTitan's design
//! code without one is suspect, since OpenTitan's CI runs verible's lint.
//!
//!     cargo run --release -p astli-lint --example lint-report
//!     cargo run --release -p astli-lint --example lint-report -- <rule>
//!
//! Given a rule, it lists that rule's hits instead, one `path:line:col` each,
//! marked `waived` where verible's waiver stands.
//!
//! Files are deduplicated by content, since the repositories vendor each
//! other.

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use astli_lint::{Config, Group, Level, RULES, lint};
use astli_parse::SyntaxTree;
use rustc_hash::FxHashMap;

/// Repositories named per rule, most hit first.
const REPOS: usize = 3;

/// One rule's hits.
#[derive(Default)]
struct Tally {
    hits: usize,
    waived: usize,
    files: HashSet<PathBuf>,
    by_repo: FxHashMap<String, usize>,
}

fn main() {
    let corpus = Path::new("corpus");
    if !corpus.is_dir() {
        eprintln!("no corpus/ -- run scripts/fetch-corpus.sh");
        std::process::exit(1);
    }
    let only = std::env::args().nth(1);
    if let Some(rule) = &only
        && !RULES.iter().any(|it| it.name == rule)
    {
        eprintln!("no rule is called `{rule}`");
        std::process::exit(1);
    }

    let mut config = Config::default();
    for group in Group::ALL {
        config.set(group.name(), Level::Warn).unwrap();
    }

    let mut paths = Vec::new();
    files(corpus, &mut paths);
    paths.sort();

    let mut seen = HashSet::new();
    let mut tallies: FxHashMap<&str, Tally> = FxHashMap::default();
    for path in &paths {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        if !seen.insert(hasher.finish()) {
            continue;
        }

        let lines: Vec<&str> = text.lines().collect();
        let tree = SyntaxTree::parse(path, text.clone());
        for found in lint(&tree, &config) {
            let code = found.code.as_str();
            let at = tree.line_col(found.at.start);
            let waived = waived(&lines, code, at.line as usize);
            if only.as_deref() == Some(code) {
                let mark = if waived { "  waived" } else { "" };
                println!("{}:{at}: {}{mark}", path.display(), found.message);
            }

            let tally = tallies.entry(code).or_default();
            tally.hits += 1;
            tally.waived += usize::from(waived);
            tally.files.insert(path.clone());
            *tally.by_repo.entry(repo(corpus, path)).or_default() += 1;
        }
    }
    if only.is_some() {
        return;
    }

    let hits: usize = tallies.values().map(|tally| tally.hits).sum();
    println!("{} files, {hits} hits\n", seen.len());
    println!("| Rule | Group | Hits | Files | Waived for verible | Most in |");
    println!("| --- | --- | ---: | ---: | ---: | --- |");
    for rule in RULES {
        let tally = tallies.remove(rule.name).unwrap_or_default();
        let mut repos: Vec<_> = tally.by_repo.into_iter().collect();
        repos.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let most: Vec<String> = (repos.iter().take(REPOS))
            .map(|(repo, hits)| format!("{repo} {hits}"))
            .collect();
        println!(
            "| `{}` | {} | {} | {} | {} | {} |",
            rule.name,
            rule.group.name(),
            tally.hits,
            tally.files.len(),
            tally.waived,
            most.join(", ")
        );
    }
    // What is left is not a rule's, such as a malformed waiver.
    let mut others: Vec<_> = tallies.into_iter().collect();
    others.sort_by_key(|&(code, _)| code);
    for (code, tally) in others {
        println!(
            "| `{code}` | | {} | {} | | |",
            tally.hits,
            tally.files.len()
        );
    }
}

/// Whether verible's waiver for `rule` covers 1-based `line`: a
/// `verilog_lint: waive` on it or alone on the line before, or a
/// `waive-start` not yet stopped.
fn waived(lines: &[&str], rule: &str, line: usize) -> bool {
    let waives = |text: &str, how: &str| {
        text.find(&format!("verilog_lint: {how} {rule}"))
            .is_some_and(|at| text[at..].split_whitespace().nth(2) == Some(rule))
    };
    let here = lines.get(line - 1).copied().unwrap_or("");
    let before = line.checked_sub(2).and_then(|at| lines.get(at)).copied();
    if waives(here, "waive")
        || before.is_some_and(|it| it.trim_start().starts_with("//") && waives(it, "waive"))
    {
        return true;
    }
    let mut open = false;
    for text in &lines[..line] {
        if waives(text, "waive-start") {
            open = true;
        } else if waives(text, "waive-stop") {
            open = false;
        }
    }
    open
}

/// The repository under `corpus` that `path` is in.
fn repo(corpus: &Path, path: &Path) -> String {
    let inside = path.strip_prefix(corpus).unwrap_or(path);
    let first = inside.components().next();
    first.map_or_else(String::new, |it| {
        it.as_os_str().to_string_lossy().into_owned()
    })
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
