//! Every case under `tests/data/`, expanded and parsed, and compared with
//! the snapshot beside it: `lower/*.sv` lowered, its HIR written out in a
//! `.hir`; `resolve/*.sv` resolved, each name it uses and what it refers to
//! in a `.names`; `access/*.sv` walked, each read and write of a symbol and
//! what drives it in an `.access`; `check/*.sv` checked, each error in a
//! `.check`; `consts/*.sv` elaborated, each parameter's and enum member's
//! type and value in a `.consts`; `types/*.sv` elaborated, each port's,
//! net's, variable's and typedef's type, and those of each continuous
//! assignment's sides, in a `.types`; `tree/*.sv` elaborated from the
//! definition called `top`, its instance tree in a `.tree`; `check_top/*.sv`
//! checked as built from `top`, each error in a `.check`.
//!
//! A case must parse whole, so that sema is tested on the tree it is for,
//! except one named `unparsed*.sv`, for what the parser keeps as written;
//! and must expand without a problem, except one named `missing*.sv`, for an
//! `` `include `` not found.
//!
//! A case is a file, and the reason it exists is a comment inside it. Adding
//! one means writing the `.sv` and running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run -p astli-sema snapshots
//! ```
//!
//! which writes every snapshot that is missing or differs. The diff is the
//! review.

use std::fmt::Write;
use std::panic;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use astli_parse::{Parsed, parse_expanded};
use astli_preproc::Session;
use astli_sema::{
    Built, Design, Driver, Elaboration, ExprKind, Member, Resolution, ScopeId, SymbolKind,
    SymbolRef, accesses, check, lower,
};
use astli_syntax::SyntaxKind::VERBATIM;
use astli_text::Span;
use expect_test::expect_file;

#[test]
fn snapshots_lower() {
    snapshots("lower", "hir", |_, parsed| lower(parsed).to_string());
}

#[test]
fn snapshots_resolve() {
    snapshots("resolve", "names", resolved);
}

#[test]
fn snapshots_access() {
    snapshots("access", "access", accessed);
}

#[test]
fn snapshots_check() {
    snapshots("check", "check", checked);
}

#[test]
fn snapshots_consts() {
    snapshots("consts", "consts", evaluated);
}

#[test]
fn snapshots_types() {
    snapshots("types", "types", typed);
}

#[test]
fn snapshots_check_top() {
    snapshots("check_top", "check", |session, parsed| {
        let elaboration = elaborated(parsed);
        let design = elaboration.design();
        let (file, _) = design.files().next().unwrap();
        let top = elaboration.top("top").expect("a case has a `top`");
        let built = Built::new(&elaboration, &[top]);
        let mut found = check(&elaboration, file, Some(&built));
        found.extend(
            built
                .multiple_drivers(&elaboration)
                .into_iter()
                .map(|(_, it)| it),
        );
        found.sort_by_key(|diagnostic| diagnostic.at.start);
        render(session, &found)
    });
}

#[test]
fn snapshots_tree() {
    snapshots("tree", "tree", |_, parsed| {
        let elaboration = elaborated(parsed);
        let top = elaboration.top("top").expect("a case has a `top`");
        elaboration.dump(top)
    });
}

/// The case as a design of one file.
fn elaborated(parsed: &Parsed) -> Elaboration {
    Elaboration::new(Arc::new(Design::new(vec![Arc::new(lower(parsed))])))
}

/// Checks each `.sv` in `tests/data/<dir>` against its `.<extension>`, as
/// `write` writes it from the case's session and tree.
fn snapshots(dir: &str, extension: &str, write: fn(&Session, &Parsed) -> String) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(dir);
    let mut cases = Vec::new();
    let mut strays = Vec::new();
    for path in entries(&root) {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("sv") => cases.push(path),
            Some(ext) if ext == extension && path.with_extension("sv").exists() => {}
            _ => strays.push(path.display().to_string()),
        }
    }
    assert!(!cases.is_empty(), "no cases under {}", root.display());

    let mut failed = Vec::new();
    for case in &cases {
        let name = case.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(case).expect("a case is UTF-8");
        // One mismatch must not hide the rest, so each is caught and the
        // panic message -- the diff -- is left to the default hook to print.
        let outcome = panic::catch_unwind(|| {
            let (session, parsed) = parsed(&name, text);
            expect_file![case.with_extension(extension)].assert_eq(&write(&session, &parsed));
        });
        if outcome.is_err() {
            failed.push(name);
        }
    }

    assert!(
        strays.is_empty(),
        "not a case or a case's `.{extension}`:\n{}",
        strays.join("\n")
    );
    assert!(
        failed.is_empty(),
        "{} of {} cases differ (UPDATE_EXPECT=1 to accept):\n{}",
        failed.len(),
        cases.len(),
        failed.join("\n")
    );
}

/// The expanded tree of `text`, which must parse whole unless its name says
/// it may not.
fn parsed(name: &str, text: String) -> (Session<'static>, Parsed) {
    let mut session = Session::new();
    let file = session.add(name, text);
    let expanded = session.expand(file);
    let parsed = parse_expanded(&session, &expanded.tokens);
    let verbatim = parsed
        .root
        .descendants()
        .any(|node| node.kind() == VERBATIM);
    let missing = name.starts_with("missing");
    let expanded_ok =
        (expanded.diagnostics.iter()).all(|it| missing && it.code.as_str() == "include-not-found");
    assert!(
        expanded_ok && parsed.diagnostics.is_empty(),
        "{name}: {:?}",
        (expanded.diagnostics.iter().chain(&parsed.diagnostics)).collect::<Vec<_>>()
    );
    assert!(
        !verbatim || name.starts_with("unparsed"),
        "{name}: not parsed whole"
    );
    (session, parsed)
}

/// Each name the case uses, in the order written, and what it refers to.
fn resolved(session: &Session, parsed: &Parsed) -> String {
    let elaboration = elaborated(parsed);
    let design = elaboration.design();
    let (file, hir) = design.files().next().unwrap();
    let names = design.names(file);
    let origins = session.origins();
    let at = |span: Span| origins.line_col(span.src_id, span.start);

    let mut found = Vec::new();
    for (id, expr) in hir.exprs() {
        if let Some(resolution) = names.expr(id) {
            found.push((expr.span, "", resolution));
        }
    }
    for (name, resolution) in names.loose() {
        found.push((name.span, "loose ", *resolution));
    }
    found.sort_by_key(|(span, _, _)| (span.start, span.end));
    found.dedup();

    let mut out = String::new();
    for (span, loose, resolution) in found {
        let what = match resolution {
            Resolution::Declared(symbol) => {
                let symbol = design.symbol(symbol);
                let kind = match &symbol.kind {
                    SymbolKind::Definition { .. } => "definition",
                    SymbolKind::Class(_) => "class",
                    SymbolKind::Port(_) => "port",
                    SymbolKind::Parameter(_) => "parameter",
                    SymbolKind::Net(_) => "net",
                    SymbolKind::Variable(_) => "variable",
                    SymbolKind::Genvar(_) => "genvar",
                    SymbolKind::Typedef(_) => "typedef",
                    SymbolKind::EnumMember { .. } => "enum member",
                    SymbolKind::Subroutine { .. } => "subroutine",
                    SymbolKind::Instance(_) => "instance",
                    SymbolKind::Block(_) => "block",
                    SymbolKind::Label(_) => "label",
                    SymbolKind::Other(_) => "other",
                };
                format!("{kind} {} at {}", symbol.name.text, at(symbol.name.span))
            }
            Resolution::Implicit(first) => format!("implicit net at {}", at(hir[first].span)),
            Resolution::Unknown => "unknown".to_string(),
            Resolution::Undeclared => "undeclared".to_string(),
        };
        writeln!(
            out,
            "{}: {loose}{} -> {what}",
            at(span),
            origins.slice(span)
        )
        .unwrap();
    }
    out
}

/// Each access the case makes, in the order written: the name, read or
/// written, whether of all of it, and what drives it.
fn accessed(session: &Session, parsed: &Parsed) -> String {
    let elaboration = elaborated(parsed);
    let design = elaboration.design();
    let (file, hir) = design.files().next().unwrap();
    let names = design.names(file);
    let origins = session.origins();
    let at = |span: Span| origins.line_col(span.src_id, span.start);

    let mut found = accesses(design, file, names);
    found.sort_by_key(|access| (access.at.start, access.at.end));
    let mut out = String::new();
    for access in found {
        let how = match (access.read, access.write) {
            (true, true) => "rw",
            (true, false) => "r",
            _ => "w",
        };
        let part = if access.whole { "" } else { " part" };
        let part = if access.certain {
            part.to_string()
        } else {
            format!("{part} guessed")
        };
        let driver = match access.driver {
            Driver::Process(body) => format!("process at {}", at(hir[body].span)),
            Driver::Assign(assign) => format!("assign at {}", at(hir[assign].span)),
            Driver::Instance(it) => match &*hir[it].name.text {
                "" => format!("unnamed instance at {}", at(hir[it].name.span)),
                name => format!("instance {name}"),
            },
            Driver::Init(it) => format!("init of {}", hir[it].name.text),
            Driver::Subroutine(it) => format!("in {}", hir[it].name.text),
            Driver::Other => "other".to_string(),
        };
        let name = &design.symbol(access.symbol).name.text;
        writeln!(out, "{}: {name} {how}{part} by {driver}", at(access.at)).unwrap();
    }
    out
}

/// Each error `check` finds in the case, with its code.
fn checked(session: &Session, parsed: &Parsed) -> String {
    let elaboration = elaborated(parsed);
    let design = elaboration.design();
    let (file, _) = design.files().next().unwrap();
    render(session, &check(&elaboration, file, None))
}

/// Each diagnostic a line, with its severity and code.
fn render(session: &Session, found: &[astli_text::Diagnostic]) -> String {
    let origins = session.origins();
    let mut out = String::new();
    for diagnostic in found {
        let at = origins.line_col(diagnostic.at.src_id, diagnostic.at.start);
        let (severity, code) = (diagnostic.severity, diagnostic.code);
        writeln!(out, "{at}: {severity}[{code}] {}", diagnostic.message).unwrap();
    }
    out
}

/// Each definition the case declares, and in the order written, each
/// parameter and enum member inside it, with its type and value at the
/// definition's defaults.
fn evaluated(session: &Session, parsed: &Parsed) -> String {
    let elaboration = elaborated(parsed);
    let design = elaboration.design();
    let (file, hir) = design.files().next().unwrap();
    let origins = session.origins();

    let mut homes = vec![hir.root(); hir.symbols().count()];
    for (id, scope) in hir.scopes() {
        for member in &scope.members {
            if let Member::Declare(symbol) = member {
                homes[symbol.index()] = id;
            }
        }
    }
    let within = |mut scope: ScopeId, outer: ScopeId| loop {
        if scope == outer {
            return true;
        }
        match hir[scope].parent {
            Some(parent) => scope = parent,
            None => return false,
        }
    };

    let mut out = String::new();
    for member in &hir[hir.root()].members {
        let Member::Declare(definition) = member else {
            continue;
        };
        let SymbolKind::Definition { scope, .. } = hir[*definition].kind else {
            continue;
        };
        let at = SymbolRef {
            file,
            symbol: *definition,
        };
        let body = elaboration.body(at).expect("a definition has a body");
        writeln!(out, "{}", hir[*definition].name.text).unwrap();
        for (id, symbol) in hir.symbols() {
            if !within(homes[id.index()], scope)
                || !matches!(
                    symbol.kind,
                    SymbolKind::Parameter(_) | SymbolKind::EnumMember { .. }
                )
            {
                continue;
            }
            let at = origins.line_col(symbol.name.span.src_id, symbol.name.span.start);
            writeln!(
                out,
                "  {at}: {}: {} = {}",
                symbol.name.text,
                elaboration.type_of(body, id),
                elaboration.value(body, id)
            )
            .unwrap();
        }
    }
    out
}

/// Each definition the case declares, and in the order written, the type of
/// each port, net, variable and typedef inside it, and of both sides of
/// each continuous assignment.
fn typed(session: &Session, parsed: &Parsed) -> String {
    let elaboration = elaborated(parsed);
    let design = elaboration.design();
    let (file, hir) = design.files().next().unwrap();
    let origins = session.origins();
    let at = |span: Span| origins.line_col(span.src_id, span.start);

    let mut out = String::new();
    for member in &hir[hir.root()].members {
        let Member::Declare(definition) = member else {
            continue;
        };
        let SymbolKind::Definition { scope, .. } = hir[*definition].kind else {
            continue;
        };
        let body = (elaboration.body(SymbolRef {
            file,
            symbol: *definition,
        }))
        .expect("a definition has a body");
        writeln!(out, "{}", hir[*definition].name.text).unwrap();
        let mut lines = Vec::new();
        for member in &hir[scope].members {
            match member {
                Member::Declare(symbol) => {
                    let symbol_at = &hir[*symbol];
                    if matches!(
                        symbol_at.kind,
                        SymbolKind::Port(_)
                            | SymbolKind::Net(_)
                            | SymbolKind::Variable(_)
                            | SymbolKind::Typedef(_)
                    ) {
                        let ty = elaboration.type_of(body, *symbol);
                        let span = symbol_at.name.span;
                        lines.push((
                            span.start,
                            format!("  {}: {}: {ty}", at(span), symbol_at.name.text),
                        ));
                    }
                }
                Member::Assign(assignments) => {
                    for &assignment in assignments {
                        let ExprKind::Assign { lhs, rhs, .. } = hir[assignment].kind else {
                            continue;
                        };
                        for side in [lhs, rhs] {
                            let span = hir[side].span;
                            let ty = elaboration.expr_type(body, side);
                            let text = origins.slice(span);
                            lines.push((span.start, format!("  {}: {text}: {ty}", at(span))));
                        }
                    }
                }
                _ => {}
            }
        }
        lines.sort_by_key(|(start, _)| *start);
        for (_, line) in lines {
            writeln!(out, "{line}").unwrap();
        }
    }
    out
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("a readable directory")
        .map(|entry| entry.expect("a readable entry").path())
        .collect();
    entries.sort();
    entries
}
