//! Every case under `tests/data/`, expanded and parsed, and compared with
//! the snapshot beside it: `lower/*.sv` lowered, its HIR written out in a
//! `.hir`; `resolve/*.sv` resolved, each name it uses and what it refers to
//! in a `.names`; `access/*.sv` walked, each read and write of a symbol and
//! what drives it in an `.access`; `check/*.sv` checked, each error in a
//! `.check`; `consts/*.sv` elaborated, each parameter's and enum member's
//! type and value in a `.consts`; `types/*.sv` elaborated, each port's,
//! net's, variable's and typedef's type, and those of each continuous
//! assignment's sides, a class's properties and its methods' assignments
//! included, in a `.types`; `tree/*.sv` elaborated from the
//! definition called `top`, its instance tree in a `.tree`; `check_top/*.sv`
//! checked as built from `top`, each error in a `.check`; `top/*.sv`, a top
//! built as each of its `//>` lines asks, what it gives in a `.top`.
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
use std::path::Path;
use std::sync::Arc;

use astli_parse::{Parsed, parse_expanded};
use astli_preproc::Session;
use astli_sema::{
    BodyId, Built, Design, Driver, Elaboration, ExprId, ExprKind, Hir, Member, Resolution, ScopeId,
    StmtKind, SymbolKind, SymbolRef, TopError, TopSetting, TopValue, accesses, check, lower,
};
use astli_syntax::SyntaxKind::VERBATIM;
use astli_testing::snapshots;
use astli_text::Span;

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
            (built.multiple_drivers(&elaboration).into_iter())
                .chain(built.mistyped(&elaboration))
                .chain(built.elaboration_tasks(&elaboration))
                .chain(built.unset_parameters(&elaboration))
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

#[test]
fn snapshots_top() {
    snapshots("top", "top", |_, parsed| {
        let elaboration = elaborated(parsed);
        let text = parsed.root.text().to_string();
        let mut out = String::new();
        for line in text.lines() {
            let Some(asked) = line.trim().strip_prefix("//> ") else {
                continue;
            };
            let answer = match asked.split_whitespace().collect::<Vec<_>>().as_slice() {
                ["values", top, name, set @ ..] => top_set(&elaboration, top, set)
                    .and_then(|(body, _)| elaboration.top_values(body, name))
                    .map(|values| (values.iter().map(spelled)).collect::<Vec<_>>().join(", ")),
                ["set", top, set @ ..] => top_set(&elaboration, top, set).map(|(body, values)| {
                    (values.iter())
                        .map(|(name, value)| {
                            let held = top_parameter(&elaboration, body, name);
                            format!("{name} = {} is {held}", spelled(value))
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                }),
                ["literal", top, set @ ..] => {
                    let set: Vec<_> = (set.iter())
                        .map(|it| it.split_once('=').unwrap())
                        .map(|(name, text)| (name, TopSetting::Literal(text.into())))
                        .collect();
                    elaboration.top_with(top, &set).map(|body| {
                        let names = set.iter().map(|(name, _)| name);
                        (names.map(|name| {
                            format!("{name} = {}", top_parameter(&elaboration, body, name))
                        }))
                        .collect::<Vec<_>>()
                        .join(", ")
                    })
                }
                _ => panic!("not a request: {asked}"),
            };
            match answer {
                Ok(answer) => writeln!(out, "{asked}\n  {answer}").unwrap(),
                Err(err) => writeln!(out, "{asked}\n  {err:?}").unwrap(),
            }
        }
        out
    });
}

/// Parameters of a top by name, each with a value read for it.
type Values<'a> = Vec<(&'a str, TopValue)>;

/// The body of `top` with each of `set`, `Name=text`, read as a value in
/// the body the ones before it give, and the values read.
fn top_set<'a>(
    elaboration: &Elaboration,
    top: &str,
    set: &[&'a str],
) -> Result<(BodyId, Values<'a>), TopError> {
    let mut values: Values = Vec::new();
    let settings = |values: &[(&'a str, TopValue)]| -> Vec<(&'a str, TopSetting)> {
        (values.iter())
            .map(|(name, value)| (*name, TopSetting::Value(value.clone())))
            .collect()
    };
    for it in set {
        let (name, text) = it.split_once('=').unwrap();
        let body = elaboration.top_with(top, &settings(&values))?;
        values.push((name, elaboration.top_value(body, name, text)?));
    }
    Ok((elaboration.top_with(top, &settings(&values))?, values))
}

/// A value by both its spellings, short and as an integer.
fn spelled(value: &TopValue) -> String {
    format!("{} ({})", value.short(), value.integer())
}

/// The value the parameter `name` of a top's `body` has.
fn top_parameter(elaboration: &Elaboration, body: BodyId, name: &str) -> String {
    let (file, scope) = elaboration.scope(body);
    let hir = &elaboration.design()[file];
    let symbol = (hir[scope].members.iter())
        .find_map(|member| match member {
            Member::Declare(it) if &*hir[*it].name.text == name => Some(*it),
            _ => None,
        })
        .expect("a parameter the top declares");
    format!("{}", elaboration.value(body, symbol))
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
    snapshots::check(&root, extension, |name, text| {
        let (session, parsed) = parsed(name, text);
        write(&session, &parsed)
    });
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
    let unparsed = name.starts_with("unparsed");
    let parsed_ok =
        (parsed.diagnostics.iter()).all(|it| unparsed && it.code.as_str() == "not-parsed");
    assert!(
        expanded_ok && parsed_ok,
        "{name}: {:?}",
        (expanded.diagnostics.iter().chain(&parsed.diagnostics)).collect::<Vec<_>>()
    );
    assert!(!verbatim || unparsed, "{name}: not parsed whole");
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
                    SymbolKind::Constraint(_) => "constraint",
                    SymbolKind::Port(_) => "port",
                    SymbolKind::Parameter(_) => "parameter",
                    SymbolKind::Net(_) => "net",
                    SymbolKind::Variable(_) => "variable",
                    SymbolKind::Genvar(_) => "genvar",
                    SymbolKind::Typedef(_) | SymbolKind::Forward => "typedef",
                    SymbolKind::EnumMember { .. } => "enum member",
                    SymbolKind::Subroutine { .. } => "subroutine",
                    SymbolKind::Property { .. } => "property",
                    SymbolKind::Let { .. } => "let",
                    SymbolKind::Modport(_) => "modport",
                    SymbolKind::Clocking(_) => "clocking",
                    SymbolKind::Covergroup(_) => "covergroup",
                    SymbolKind::Coverpoint(_) => "cover point",
                    SymbolKind::Iterator => "iterator",
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
        let how = match (access.read, access.write.is_some()) {
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
        types_in(&elaboration, session, body, hir, scope, &mut lines);
        lines.sort_by_key(|(start, _)| *start);
        for (_, line) in lines {
            writeln!(out, "{line}").unwrap();
        }
    }
    out
}

/// The types `scope` gives its declarations and the sides of its
/// assignments, and those of each class it declares: its properties', and
/// the sides of the assignments its methods make.
fn types_in(
    elaboration: &Elaboration,
    session: &Session,
    body: BodyId,
    hir: &Hir,
    scope: ScopeId,
    lines: &mut Vec<(u32, String)>,
) {
    let origins = session.origins();
    let at = |span: Span| origins.line_col(span.src_id, span.start);
    let sides = |lines: &mut Vec<(u32, String)>, assignment: ExprId| {
        let ExprKind::Assign { lhs, rhs, .. } = hir[assignment].kind else {
            return;
        };
        for side in [lhs, rhs] {
            let span = hir[side].span;
            let ty = elaboration.expr_type(body, side);
            let text = origins.slice(span);
            lines.push((span.start, format!("  {}: {text}: {ty}", at(span))));
        }
    };
    for member in &hir[scope].members {
        match member {
            Member::Declare(symbol) => {
                let symbol_at = &hir[*symbol];
                match &symbol_at.kind {
                    SymbolKind::Port(_)
                    | SymbolKind::Net(_)
                    | SymbolKind::Variable(_)
                    | SymbolKind::Typedef(_) => {
                        let ty = elaboration.type_of(body, *symbol);
                        let span = symbol_at.name.span;
                        lines.push((
                            span.start,
                            format!("  {}: {}: {ty}", at(span), symbol_at.name.text),
                        ));
                    }
                    SymbolKind::Class(class) => {
                        types_in(elaboration, session, body, hir, class.scope, lines)
                    }
                    SymbolKind::Subroutine { body: stmts, .. } => {
                        let mut stack = stmts.clone();
                        while let Some(stmt) = stack.pop() {
                            match &hir[stmt].kind {
                                StmtKind::Expr(expr) => sides(lines, *expr),
                                StmtKind::Block { stmts, .. } => stack.extend(stmts),
                                StmtKind::If {
                                    then_branch,
                                    else_branch,
                                    ..
                                } => stack.extend(then_branch.iter().chain(else_branch)),
                                StmtKind::Case { items, .. } => {
                                    stack.extend(items.iter().filter_map(|item| item.body));
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            Member::Assign(assignments) => {
                for &assignment in assignments {
                    sides(lines, assignment);
                }
            }
            _ => {}
        }
    }
}
