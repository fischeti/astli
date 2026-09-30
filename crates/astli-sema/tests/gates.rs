//! The corpus, lowered: every file's expanded tree lowers without a panic.
//!
//! It also says how much of the corpus sema does not see: the names in class
//! bodies, which are opaque by design, and the share of the rest that land
//! in an opaque region.

mod corpus;

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use astli_parse::parse_expanded;
use astli_preproc::{Build, Session};
use astli_sema::{ExprKind, Member, StmtKind, SymbolKind, lower};

#[test]
fn corpus_lowers() {
    let Some(files) = corpus::files() else {
        return;
    };

    // A header is found in any directory of its repository that holds one.
    let mut headers: HashMap<String, BTreeSet<PathBuf>> = HashMap::new();
    for path in files
        .iter()
        .filter(|path| path.extension().is_some_and(|e| e == "svh"))
    {
        if let Some(dir) = path.parent() {
            headers
                .entry(corpus::repo(path))
                .or_default()
                .insert(dir.to_path_buf());
        }
    }

    let (mut lowered, mut symbols, mut names, mut classes, mut opaque) = (0, 0, 0, 0, 0);
    for path in files
        .iter()
        .filter(|path| path.extension().is_some_and(|e| e == "sv"))
    {
        let dirs = headers.get(&corpus::repo(path)).into_iter().flatten();
        let build = dirs.fold(Build::new(), |build, dir| build.include_dir(dir));
        let mut session = Session::new().building(build);
        let Some(file) = session.open(path) else {
            continue; // not UTF-8; not ours to parse
        };
        let expanded = session.expand(file);
        let parsed = parse_expanded(&session, &expanded.tokens);
        let hir = lower(&parsed);
        lowered += 1;

        symbols += hir.symbols().count();
        for (_, symbol) in hir.symbols() {
            match &symbol.kind {
                SymbolKind::Class(body) => classes += body.names.len(),
                SymbolKind::Other(body) => opaque += body.names.len(),
                _ => {}
            }
        }
        opaque += (hir.scopes())
            .flat_map(|(_, scope)| &scope.members)
            .map(|member| match member {
                Member::Opaque(body) => body.names.len(),
                _ => 0,
            })
            .sum::<usize>();
        for (_, expr) in hir.exprs() {
            match &expr.kind {
                ExprKind::Name(_) => names += 1,
                ExprKind::Opaque(body) => opaque += body.names.len(),
                _ => {}
            }
        }
        for (_, stmt) in hir.stmts() {
            if let StmtKind::Opaque(body) = &stmt.kind {
                opaque += body.names.len();
            }
        }
    }
    let share = 100.0 * opaque as f64 / (names + opaque).max(1) as f64;
    eprintln!(
        "{lowered} files lowered: {symbols} symbols, {classes} names in classes; \
         outside them {names} names used and {opaque} in opaque regions ({share:.1}%)"
    );
}
