//! What is declared and never used: signals, parameters, imports.
//!
//! Only in a module, interface or program: what a package declares is for
//! whatever imports it. A name containing `unused` says it is meant not to
//! be, as the `unused_` signals that silence other tools do. A declaration a
//! macro wrote can be changed only in the macro, so it is not reported.

use astli_sema::{Hir, Member, Resolution, ScopeId, SymbolKind, SymbolRef};
use astli_syntax::SyntaxKind::{self, INPUT_KW, INTERFACE_KW, MODULE_KW, PACKAGE_KW, PROGRAM_KW};
use rustc_hash::FxHashSet;

use crate::rule::DesignCx;

/// A net, variable or input port that nothing reads.
pub(crate) fn unused_signal(cx: &mut DesignCx) {
    let read = read(cx);
    for symbol in declared(cx, |kind| match kind {
        SymbolKind::Net(_) | SymbolKind::Variable(_) => true,
        SymbolKind::Port(port) => port.direction == Some(INPUT_KW),
        _ => false,
    }) {
        let name = &cx.hir()[symbol.symbol].name;
        if !read.contains(&symbol) && !excused(cx, symbol) {
            let what = match cx.hir()[symbol.symbol].kind {
                SymbolKind::Port(_) => "input",
                _ => "signal",
            };
            cx.report(name.span, format!("{what} `{}` is never read", name.text));
        }
    }
}

/// A parameter that nothing reads.
pub(crate) fn unused_parameter(cx: &mut DesignCx) {
    let read = read(cx);
    for symbol in declared(cx, |kind| matches!(kind, SymbolKind::Parameter(_))) {
        let name = &cx.hir()[symbol.symbol].name;
        if !read.contains(&symbol) && !excused(cx, symbol) {
            cx.report(
                name.span,
                format!("parameter `{}` is never used", name.text),
            );
        }
    }
}

/// An import nothing in the file uses: no name it brings resolves to one of
/// its package's members.
///
/// Whether a use goes through this import or another of the same package is
/// not told apart, so of two imports of one package in a file, one used
/// keeps both quiet.
pub(crate) fn unused_import(cx: &mut DesignCx) {
    let hir = cx.hir();
    let used = brought(cx);
    for (id, scope) in hir.scopes() {
        if !matches!(
            definition(hir, id),
            Some(MODULE_KW | INTERFACE_KW | PROGRAM_KW | PACKAGE_KW)
        ) {
            continue;
        }
        let exported = |package: &str| {
            scope.members.iter().any(|member| {
                matches!(member, Member::Import(it)
                    if it.export && (&*it.package.text == package || &*it.package.text == "*"))
            })
        };
        for member in &scope.members {
            let Member::Import(import) = member else {
                continue;
            };
            let package = &*import.package.text;
            if import.export || exported(package) || !complete(cx, package) {
                continue;
            }
            let at = import.item.as_ref().unwrap_or(&import.package);
            if cx.written_by_macro(at.span) {
                continue;
            }
            match &import.item {
                Some(item) if !used.contains(&(package, &*item.text)) => {
                    let message = format!("`{package}::{}` is imported and never used", item.text);
                    cx.report(item.span, message);
                }
                None if !used.iter().any(|(it, _)| *it == package) => {
                    let message = format!("nothing imported from `{package}` is used");
                    cx.report(import.package.span, message);
                }
                _ => {}
            }
        }
    }
}

/// The symbols of the file something reads.
fn read(cx: &DesignCx) -> FxHashSet<SymbolRef> {
    (cx.accesses.iter())
        .filter(|access| access.read)
        .map(|access| access.symbol)
        .collect()
}

/// The symbols of the file's modules, interfaces and programs whose kind
/// `wanted` says.
pub(super) fn declared(cx: &DesignCx, wanted: impl Fn(&SymbolKind) -> bool) -> Vec<SymbolRef> {
    let hir = cx.hir();
    let mut found = Vec::new();
    for (id, scope) in hir.scopes() {
        if !matches!(
            definition(hir, id),
            Some(MODULE_KW | INTERFACE_KW | PROGRAM_KW)
        ) {
            continue;
        }
        for member in &scope.members {
            if let Member::Declare(symbol) = member
                && wanted(&hir[*symbol].kind)
            {
                found.push(SymbolRef {
                    file: cx.file,
                    symbol: *symbol,
                });
            }
        }
    }
    found
}

/// Whether `symbol` is not to be reported however unused: named so, written
/// by a macro, or maybe used where resolution does not reach.
fn excused(cx: &DesignCx, symbol: SymbolRef) -> bool {
    let name = &cx.hir()[symbol.symbol].name;
    name.text.to_lowercase().contains("unused")
        || cx.written_by_macro(name.span)
        || cx.linter.maybe_used(cx.file, &name.text)
}

/// The keyword of the definition `scope` is in, if any.
pub(super) fn definition(hir: &Hir, scope: ScopeId) -> Option<SyntaxKind> {
    let mut at = Some(scope);
    while let Some(scope) = at {
        if let Some(owner) = hir[scope].owner
            && let SymbolKind::Definition { keyword, .. } = hir[owner].kind
        {
            return Some(keyword);
        }
        at = hir[scope].parent;
    }
    None
}

/// Each package member a name in the file resolves to, as its package and
/// name: what the file's imports bring in and it uses. A `pkg::name` does
/// not count, since it needs no import.
fn brought<'a>(cx: &DesignCx<'a>) -> FxHashSet<(&'a str, &'a str)> {
    let hir = cx.hir();
    let mut found = FxHashSet::default();
    let mut add = |resolution: Option<Resolution>| {
        if let Some(Resolution::Declared(symbol)) = resolution
            && let Some(package) = cx.linter.package_of(symbol)
        {
            found.insert((package, &*cx.design().symbol(symbol).name.text));
        }
    };
    for (id, expr) in hir.exprs() {
        if let astli_sema::ExprKind::Name(_) = expr.kind {
            add(cx.names.expr(id));
        }
    }
    for (_, resolution) in cx.names.loose() {
        add(Some(*resolution));
    }
    found
}

/// Whether sema sees all of `package`: one file declares it, and nothing of
/// it is opaque. Of one it does not, a name nothing resolves to may still
/// come from it.
fn complete(cx: &DesignCx, package: &str) -> bool {
    let design = cx.design();
    let Some(at) = design.package(package) else {
        return false;
    };
    let SymbolKind::Definition { scope, .. } = &design.symbol(at).kind else {
        return false;
    };
    let members = &design[at.file][*scope].members;
    !design.is_ambiguous(package) && !members.iter().any(|it| matches!(it, Member::Opaque(_)))
}
