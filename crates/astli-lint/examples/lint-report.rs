//! What each lint rule finds over the corpus, with every rule on: how often,
//! in how many files, in which repositories, and how many of the hits carry a
//! waiver of another tool's in the same repository. For a rule that reads a
//! tree, verible's waiver for the rule of the same name, in a comment or in a
//! waiver file; for one that reads a design, Verilator's for the warning that
//! finds the same thing, in a comment or a `.vlt` file. A hit in OpenTitan's
//! design code without one is suspect, since OpenTitan's CI runs both.
//!
//! The rules that read a design read each repository as one design, every
//! file in it expanded with the directories its headers are in, but for a
//! file another includes, which is part of that one; those that read what
//! is built, that design built from every module nothing instantiates.
//!
//! PULP's EDA benchmarks, in `eda-benchmarks/` if fetched, count too: each
//! variant a design of the files its filelist names, with its directories
//! and definitions, built from the top it names. Each design of them is
//! named in the table as `eda-benchmarks/<design>`.
//!
//!     cargo run --release -p astli-lint --example lint-report
//!     cargo run --release -p astli-lint --example lint-report -- <rule>
//!
//! Given a rule, it lists that rule's hits instead, one `path:line:col` each,
//! marked `waived` where verible's waiver stands.
//!
//! Files are deduplicated by content, since the repositories vendor each
//! other.

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use astli_lint::{Config, Group, Level, Linter, RULES, Waivers, lint};
use astli_parse::{SyntaxTree, parse_expanded};
use astli_preproc::{Build, Session};
use astli_sema::{Built, Design, Elaboration, SymbolKind, SymbolRef, lower};
use astli_syntax::SyntaxKind::MODULE_KW;
use regex::Regex;
use rustc_hash::FxHashMap;

/// Repositories named per rule, most hit first.
const REPOS: usize = 3;

/// Where `scripts/fetch-corpus.sh` puts PULP's EDA benchmarks.
const BENCHMARKS: &str = "eda-benchmarks";

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
    // After the corpus, so that a file it holds a copy of is the corpus's.
    let variants = variants();
    let mut named: BTreeSet<&PathBuf> = BTreeSet::new();
    for variant in &variants {
        named.extend(&variant.files);
    }
    paths.extend(named.into_iter().cloned());
    let roots = [corpus, Path::new(BENCHMARKS)];
    let waiver_files: FxHashMap<_, _> = roots.iter().flat_map(|it| waiver_files(it)).collect();

    let mut seen = HashSet::new();
    let mut kept = HashSet::new();
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
        kept.insert(path.clone());

        let lines: Vec<&str> = text.lines().collect();
        let tree = SyntaxTree::parse(path, text.clone());
        for found in lint(&tree, &config) {
            let code = found.code.as_str();
            let at = tree.line_col(found.at.start);
            let waived = waived(&lines, code, at.line as usize)
                || (waiver_files.get(&repo(path))).is_some_and(|waivers| {
                    (waivers.iter()).any(|(rule, location)| {
                        rule == code && location.is_match(&path.to_string_lossy())
                    })
                });
            if only.as_deref() == Some(code) {
                let mark = if waived { "  waived" } else { "" };
                println!("{}:{at}: {}{mark}", path.display(), found.message);
            }

            let tally = tallies.entry(code).or_default();
            tally.hits += 1;
            tally.waived += usize::from(waived);
            tally.files.insert(path.clone());
            *tally.by_repo.entry(repo(path)).or_default() += 1;
        }
    }

    let vlt: FxHashMap<_, _> = roots.iter().flat_map(|it| vlt_waivers(it)).collect();
    let mut found = HashSet::new();
    for hit in design_hits(corpus, &paths, &variants) {
        // A copy of a file counts once, as for the rules that read a tree,
        // and so does a hit in each variant that builds its file.
        if !kept.contains(&hit.path)
            || !found.insert((hit.path.clone(), hit.line, hit.col, hit.code))
        {
            continue;
        }
        let code = hit.code;
        let lines = std::fs::read_to_string(&hit.path).unwrap_or_default();
        let lines: Vec<&str> = lines.lines().collect();
        let warnings = verilator(code);
        let messages = verilator_messages(code, &hit.name);
        let path = hit.path.to_string_lossy();
        let waived = !warnings.is_empty()
            && (verilator_waived(&lines, warnings, hit.line)
                || (vlt.get(&repo(&hit.path))).is_some_and(|waivers| {
                    waivers.iter().any(|(rule, file, matching)| {
                        warnings.contains(&rule.as_str())
                            && file.is_match(&path)
                            && matching
                                .as_ref()
                                .is_none_or(|it| messages.iter().any(|m| it.is_match(m)))
                    })
                }));
        if only.as_deref() == Some(code) {
            let mark = if waived { "  waived" } else { "" };
            let (line, col) = (hit.line, hit.col);
            println!("{}:{line}:{col}: {}{mark}", hit.path.display(), hit.message);
        }
        let tally = tallies.entry(code).or_default();
        tally.hits += 1;
        tally.waived += usize::from(waived);
        tally.files.insert(hit.path.clone());
        *tally.by_repo.entry(repo(&hit.path)).or_default() += 1;
    }
    if only.is_some() {
        return;
    }

    let hits: usize = tallies.values().map(|tally| tally.hits).sum();
    println!("{} files, {hits} hits\n", seen.len());
    println!("| Rule | Group | Hits | Files | Waived | Most in |");
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

/// One finding of a rule that reads a design.
struct Hit {
    path: PathBuf,
    line: u32,
    col: u32,
    code: &'static str,
    message: String,
    /// The name it is about, the first one its message quotes.
    name: String,
}

/// What the rules that read a design find, each corpus repository read as
/// one, and each benchmark variant.
fn design_hits(corpus: &Path, paths: &[PathBuf], variants: &[Variant]) -> Vec<Hit> {
    let mut config = Config::default();
    for group in Group::ALL {
        config.set(group.name(), Level::Allow).unwrap();
    }
    for rule in RULES.iter().filter(|rule| rule.needs_design()) {
        config.set(rule.name, Level::Warn).unwrap();
    }

    let mut by_repo: BTreeMap<String, Vec<&PathBuf>> = BTreeMap::new();
    for path in paths.iter().filter(|it| it.starts_with(corpus)) {
        by_repo.entry(repo(path)).or_default().push(path);
    }
    let mut hits = Vec::new();
    for paths in by_repo.values() {
        let headers: BTreeSet<&Path> = (paths.iter())
            .filter(|path| path.extension().is_some_and(|e| e == "svh"))
            .filter_map(|path| path.parent())
            .collect();
        let build = (headers.iter()).fold(Build::new(), |build, dir| build.include_dir(dir));
        let texts: Vec<(&PathBuf, String)> = (paths.iter())
            .filter_map(|&path| Some((path, std::fs::read_to_string(path).ok()?)))
            .collect();
        // A `.sv` another file includes is part of that one's unit.
        let included: HashSet<&str> = (texts.iter())
            .flat_map(|(_, text)| text.lines())
            .filter_map(|line| line.trim_start().strip_prefix("`include"))
            .filter_map(|rest| rest.split('"').nth(1))
            .filter_map(|name| name.rsplit('/').next())
            .collect();

        let units: Vec<(&Path, &str)> = (texts.iter())
            .filter(|(path, _)| {
                let name = path.file_name().map(|it| it.to_string_lossy());
                path.extension().is_some_and(|e| e == "sv")
                    && !name.is_some_and(|name| included.contains(&*name))
            })
            .map(|(path, text)| (path.as_path(), text.as_str()))
            .collect();
        hits.extend(design(&units, &build, None, &config));
    }
    for variant in variants {
        let texts: Vec<(&PathBuf, String)> = (variant.files.iter())
            .filter_map(|path| Some((path, std::fs::read_to_string(path).ok()?)))
            .collect();
        let units: Vec<(&Path, &str)> = (texts.iter())
            .map(|(path, text)| (path.as_path(), text.as_str()))
            .collect();
        hits.extend(design(&units, &variant.build, Some(&variant.top), &config));
    }
    hits
}

/// What the rules `config` turns on find in the design `units` make, built
/// from `top`, or from every module nothing instantiates.
fn design(units: &[(&Path, &str)], build: &Build, top: Option<&str>, config: &Config) -> Vec<Hit> {
    let mut hirs = Vec::new();
    let mut lowered = Vec::new();
    for (path, text) in units {
        let tree = SyntaxTree::parse(path, text.to_string());
        let mut session = Session::new().building(build.clone());
        let file = session.add(path, text.to_string());
        let expanded = session.expand(file);
        hirs.push(lower(&parse_expanded(&session, &expanded.tokens)));
        lowered.push((Waivers::of(&tree), session.into_origins()));
    }

    let design = Design::new(hirs.into_iter().map(Arc::new).collect());
    let elaboration = Elaboration::new(Arc::new(design));
    let design = elaboration.design();
    let linter = Linter::new(&elaboration);
    let instantiated: HashSet<&str> = (design.files())
        .flat_map(|(_, hir)| hir.symbols())
        .filter_map(|(_, symbol)| match &symbol.kind {
            SymbolKind::Instance(instance) => Some(&*instance.definition.text),
            _ => None,
        })
        .collect();
    let tops: Vec<_> = (design.files())
        .flat_map(|(file, hir)| hir.symbols().map(move |(id, symbol)| (file, id, symbol)))
        .filter(|(_, _, symbol)| match (top, &symbol.kind) {
            (Some(top), SymbolKind::Definition { .. }) => &*symbol.name.text == top,
            (None, SymbolKind::Definition { keyword, .. }) => {
                *keyword == MODULE_KW && !instantiated.contains(&*symbol.name.text)
            }
            _ => false,
        })
        .filter_map(|(file, symbol, _)| elaboration.body(SymbolRef { file, symbol }))
        .collect();
    let mut widths = vec![Vec::new(); lowered.len()];
    for width in astli_lint::widths(&elaboration, &Built::new(&elaboration, &tops)) {
        widths[width.file.index()].push(width);
    }
    let mut hits = Vec::new();
    for ((file, _), (waivers, origins)) in design.files().zip(&lowered) {
        let built = linter.lint_built(origins, waivers, config, &widths[file.index()]);
        for found in linter
            .lint(file, origins, waivers, config)
            .into_iter()
            .chain(built)
        {
            let at = origins.spelled(origins.reported_at(found.at));
            let Some(path) = origins.path(at.src_id) else {
                continue;
            };
            let position = origins.line_col(at.src_id, at.start);
            let name = found.message.split('`').nth(1).unwrap_or("").to_string();
            hits.push(Hit {
                path: path.to_path_buf(),
                line: position.line,
                col: position.col,
                code: found.code.as_str(),
                message: found.message,
                name,
            });
        }
    }
    hits
}

/// A benchmark variant: the top it names, and what its filelist does.
struct Variant {
    top: String,
    build: Build,
    files: Vec<PathBuf>,
}

/// Each benchmark variant, none if they were never fetched.
fn variants() -> Vec<Variant> {
    let root = Path::new(BENCHMARKS);
    let Ok(targets) = std::fs::read_to_string(root.join("targets.mk")) else {
        return Vec::new();
    };
    let mut variants = Vec::new();
    // `<design>.<variant>.TOP := <top>`, one line each.
    for line in targets.lines() {
        let Some((target, top)) = line.split_once(".TOP :=") else {
            continue;
        };
        let Some((design, variant)) = target.trim().split_once('.') else {
            continue;
        };
        let dir = root.join(design).join("variants").join(variant);
        let Ok(filelist) = std::fs::read_to_string(dir.join("filelist.f")) else {
            continue;
        };
        let mut build = Build::new();
        let mut files = Vec::new();
        for line in filelist.lines().map(str::trim).filter(|it| !it.is_empty()) {
            if let Some(include) = line.strip_prefix("+incdir+") {
                build = build.include_dir(normal(&dir.join(include)));
            } else if let Some(define) = line.strip_prefix("+define+") {
                let (name, body) = define.split_once('=').unwrap_or((define, "1"));
                build = build.define(name, body);
            } else {
                files.push(normal(&dir.join(line)));
            }
        }
        let top = top.trim().to_string();
        variants.push(Variant { top, build, files });
    }
    variants
}

/// `path` with each `..` taken back, so that the variants reaching one file
/// name it alike.
fn normal(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            part => out.push(part),
        }
    }
    out
}

/// The Verilator warnings that find what the rule `code` finds; none for a
/// rule it has no warning for.
fn verilator(code: &str) -> &'static [&'static str] {
    match code {
        "unused-signal" => &["UNUSED", "UNUSEDSIGNAL"],
        "unused-parameter" => &["UNUSED", "UNUSEDPARAM"],
        "undriven-signal" => &["UNDRIVEN"],
        "multiple-drivers" => &["MULTIDRIVEN"],
        "width-expand" => &["WIDTH", "WIDTHEXPAND"],
        "width-trunc" => &["WIDTH", "WIDTHTRUNC"],
        _ => &[],
    }
}

/// What Verilator says of `name` where the rule `code` finds it, which a
/// `.vlt` waiver's `-match` is matched against. Of a signal some of whose
/// bits are used, it says something else, which waives no finding here.
fn verilator_messages(code: &str, name: &str) -> Vec<String> {
    let says: &[&str] = match code {
        "unused-signal" => &["Signal is not used", "Signal is not driven, nor used"],
        "unused-parameter" => &["Parameter is not used"],
        "undriven-signal" => &["Signal is not driven", "Signal is not driven, nor used"],
        "multiple-drivers" => &["Signal has multiple driving blocks with different clocking"],
        _ => &[],
    };
    says.iter().map(|say| format!("{say}: '{name}'")).collect()
}

/// Whether a `verilator lint_off` for one of `warnings` is in force at
/// 1-based `line`, not yet turned on again.
fn verilator_waived(lines: &[&str], warnings: &[&str], line: u32) -> bool {
    let mut off = false;
    for text in lines.iter().take(line as usize) {
        for (how, state) in [("lint_off", true), ("lint_on", false)] {
            let names = text.split(&format!("verilator {how}")).nth(1);
            let named = names.and_then(|it| it.split_whitespace().next());
            if named.is_some_and(|it| warnings.contains(&it.trim_end_matches("*/"))) {
                off = state;
            }
        }
    }
    off
}

/// The `lint_off` lines of each repository's `.vlt` files: the warning, the
/// files it is off in, and the messages, if it says.
fn vlt_waivers(corpus: &Path) -> FxHashMap<String, Vec<(String, Regex, Option<Regex>)>> {
    let mut all = Vec::new();
    listed(corpus, &["vlt"], &mut all);
    let mut found: FxHashMap<String, Vec<_>> = FxHashMap::default();
    for path in all {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in text
            .lines()
            .filter(|line| line.trim_start().starts_with("lint_off"))
        {
            let flag = |name: &str| {
                let at = line.find(&format!("-{name} "))? + name.len() + 2;
                let value = line[at..].trim_start();
                let value = match value.strip_prefix('"') {
                    Some(quoted) => &quoted[..quoted.find('"')?],
                    None => value.split_whitespace().next()?,
                };
                Some(value.to_string())
            };
            let (Some(rule), Some(file)) = (flag("rule"), flag("file")) else {
                continue;
            };
            let (Some(file), matching) = (glob(&file), flag("match").map(|it| glob(&it))) else {
                continue;
            };
            let matching = match matching {
                Some(None) => continue,
                Some(it) => it,
                None => None,
            };
            found
                .entry(repo(&path))
                .or_default()
                .push((rule, file, matching));
        }
    }
    found
}

/// A Verilator glob, in which only `*` and `?` are special, as a regex
/// that matches all of a text.
fn glob(pattern: &str) -> Option<Regex> {
    let mut regex = String::from("^");
    for c in pattern.chars() {
        match c {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            c => regex.push_str(&regex::escape(&c.to_string())),
        }
    }
    regex.push('$');
    Regex::new(&regex).ok()
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

/// The waivers in verible's waiver files, by repository: a rule, and the
/// paths it is waived in. Only `--rule` with `--location` is read, which is
/// all but one of the corpus's.
fn waiver_files(corpus: &Path) -> FxHashMap<String, Vec<(String, Regex)>> {
    let mut all = Vec::new();
    let mut found: FxHashMap<String, Vec<(String, Regex)>> = FxHashMap::default();
    listed(corpus, &["vbl", "vbw"], &mut all);
    for path in all {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines().filter(|line| line.starts_with("waive ")) {
            // A value is quoted, or runs to the next space.
            let flag = |name: &str| {
                let at = line.find(&format!("--{name}="))? + name.len() + 3;
                let value = &line[at..];
                let value = match value.strip_prefix('"') {
                    Some(quoted) => &quoted[..quoted.find('"')?],
                    None => value.split_whitespace().next()?,
                };
                Some(value.to_string())
            };
            let (Some(rule), Some(location)) = (flag("rule"), flag("location")) else {
                continue;
            };
            if let Ok(location) = Regex::new(&location) {
                found.entry(repo(&path)).or_default().push((rule, location));
            }
        }
    }
    found
}

/// The corpus repository `path` is in, or the benchmark design.
fn repo(path: &Path) -> String {
    let first = |inside: &Path| {
        let first = inside.components().next();
        first.map_or_else(String::new, |it| {
            it.as_os_str().to_string_lossy().into_owned()
        })
    };
    match path.strip_prefix(BENCHMARKS) {
        Ok(inside) => format!("{BENCHMARKS}/{}", first(inside)),
        Err(_) => first(path.strip_prefix("corpus").unwrap_or(path)),
    }
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    listed(dir, &["sv", "svh"], out);
}

/// The files under `dir` with one of `extensions`.
fn listed(dir: &Path, extensions: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        // A tool's own copies of sources, such as `.git` or `.bender`, are
        // not part of the pinned corpus, and neither is a link into one, as
        // bender's from `deps/`.
        if entry.file_name().to_string_lossy().starts_with('.') || into_hidden(&entry.path()) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            listed(&path, extensions, out);
        } else if (path.extension().and_then(|e| e.to_str()))
            .is_some_and(|e| extensions.contains(&e))
        {
            out.push(path);
        }
    }
}

/// Whether `path` is a link whose target passes through a hidden directory.
fn into_hidden(path: &std::path::Path) -> bool {
    std::fs::read_link(path).is_ok_and(|target| {
        target.components().any(|part| {
            matches!(part, std::path::Component::Normal(name)
                if name.to_string_lossy().starts_with('.'))
        })
    })
}
