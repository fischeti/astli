//! The corpus, lowered and resolved: every file's expanded tree lowers
//! without a panic, and its names resolve, each repository one design.
//!
//! A repository is not one design that compiles: it holds stale testbenches,
//! files only a define it lacks makes whole, and uses of repositories it
//! does not hold. So some names are undeclared, and they are a ratchet, as
//! are the names resolution cannot decide. Either count falling is progress
//! to record here; rising is a regression, or needs a reason.
//!
//! It also says how much of the corpus sema does not see: the names in class
//! bodies, which are opaque by design, and the share of the rest that land
//! in an opaque region.

mod corpus;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use astli_parse::parse_expanded;
use astli_preproc::{Build, Session};
use astli_sema::{Design, ExprKind, Member, Resolution, StmtKind, SymbolKind, lower};

/// Names declared nowhere the resolver looked, at the pinned corpus.
const UNDECLARED: usize = 419;
/// Names that may be declared where sema cannot see.
const UNKNOWN: usize = 21_631;

#[test]
fn corpus_lowers_and_resolves() {
    let Some(files) = corpus::files() else {
        return;
    };

    // A header is found in any directory of its repository that holds one.
    let mut headers: HashMap<String, BTreeSet<PathBuf>> = HashMap::new();
    let mut repos: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    // A `.sv` another file includes is part of that file's unit, not a unit
    // of its own, whatever its name says.
    let mut included: HashMap<String, BTreeSet<String>> = HashMap::new();
    for path in &files {
        let repo = corpus::repo(path);
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let names = (text.lines())
            .filter_map(|line| line.trim_start().strip_prefix("`include"))
            .filter_map(|rest| rest.split('"').nth(1))
            .filter_map(|name| name.rsplit('/').next());
        included
            .entry(repo.clone())
            .or_default()
            .extend(names.map(str::to_string));
        match path.extension().and_then(|e| e.to_str()) {
            Some("svh") => {
                let dir = path.parent().expect("a file is in a directory");
                headers.entry(repo).or_default().insert(dir.to_path_buf());
            }
            _ => repos.entry(repo).or_default().push(path.clone()),
        }
    }

    let (mut lowered, mut symbols, mut names, mut classes, mut opaque) = (0, 0, 0, 0, 0);
    let mut resolutions: BTreeMap<&str, usize> = BTreeMap::new();
    let mut undeclared: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for (repo, paths) in &repos {
        let dirs = headers.get(repo).into_iter().flatten();
        let build = dirs.fold(Build::new(), |build, dir| build.include_dir(dir));
        let mut sessions = Vec::new();
        let mut hirs = Vec::new();
        for path in paths {
            let name = path.file_name().map(|it| it.to_string_lossy().to_string());
            if name.is_some_and(|name| included[repo].contains(&name)) {
                continue;
            }
            let mut session = Session::new().building(build.clone());
            let Some(file) = session.open(path) else {
                continue; // not UTF-8; not ours to parse
            };
            let expanded = session.expand(file);
            let parsed = parse_expanded(&session, &expanded.tokens);
            hirs.push(lower(&parsed));
            sessions.push(session);
        }
        lowered += hirs.len();

        let design = Design::new(hirs);
        for ((file, hir), session) in design.files().zip(&sessions) {
            symbols += hir.symbols().count();
            for (_, symbol) in hir.symbols() {
                match &symbol.kind {
                    SymbolKind::Class(body) => classes += body.names.len(),
                    SymbolKind::Other(body) => opaque += body.names.len(),
                    _ => {}
                }
            }
            for (_, scope) in hir.scopes() {
                for member in &scope.members {
                    if let Member::Opaque(body) = member {
                        opaque += body.names.len();
                    }
                }
            }
            for (_, stmt) in hir.stmts() {
                if let StmtKind::Opaque(body) = &stmt.kind {
                    opaque += body.names.len();
                }
            }

            let resolved = design.resolve(file);
            // A package no file declares is a dependency the corpus lacks.
            let packages: BTreeSet<_> = (hir.exprs())
                .filter_map(|(_, expr)| match expr.kind {
                    ExprKind::Scoped { base, .. } => Some(base),
                    _ => None,
                })
                .collect();
            for (id, expr) in hir.exprs() {
                match &expr.kind {
                    ExprKind::Name(_) => names += 1,
                    ExprKind::Opaque(body) => opaque += body.names.len(),
                    _ => {}
                }
                let Some(resolution) = resolved.expr(id) else {
                    continue;
                };
                let kind = match resolution {
                    Resolution::Declared(_) => "declared",
                    Resolution::Implicit(_) => "implicit",
                    Resolution::Unknown => "unknown",
                    Resolution::Undeclared if packages.contains(&id) => "missing package",
                    Resolution::Undeclared => "undeclared",
                };
                *resolutions.entry(kind).or_default() += 1;
                if kind == "undeclared" {
                    let origins = session.origins();
                    let at = origins.spelled(origins.reported_at(expr.span));
                    let text = origins.slice(expr.span).to_string();
                    let path = origins.path(at.src_id).map(|it| it.display().to_string());
                    let place = format!(
                        "{}:{}",
                        path.unwrap_or_default(),
                        origins.line_col(at.src_id, at.start)
                    );
                    let entry = undeclared.entry(text).or_insert((0, place));
                    entry.0 += 1;
                }
            }
        }
    }

    let share = 100.0 * opaque as f64 / (names + opaque).max(1) as f64;
    eprintln!(
        "{lowered} files lowered: {symbols} symbols, {classes} names in classes; \
         outside them {names} names used and {opaque} in opaque regions ({share:.1}%)"
    );
    eprintln!("resolved: {resolutions:?}");
    let mut worst: Vec<_> = undeclared.into_iter().collect();
    worst.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
    for (name, (count, place)) in worst.iter().take(20) {
        eprintln!("{count:6} {name:30} {place}");
    }

    let count = |kind| resolutions.get(kind).copied().unwrap_or(0);
    assert!(
        count("undeclared") <= UNDECLARED,
        "{} names undeclared, up from {UNDECLARED}",
        count("undeclared")
    );
    assert!(
        count("unknown") <= UNKNOWN,
        "{} names unknown, up from {UNKNOWN}",
        count("unknown")
    );
}
