//! What is declared and never used: signals, parameters, imports.
//!
//! Only in a module, interface or program: what a package declares is for
//! whatever imports it. A name containing `unused` says it is meant not to
//! be, as the `unused_` signals that silence other tools do. A declaration a
//! macro wrote can be changed only in the macro, so it is not reported.

use astli_sema::{Design, Hir, Member, Resolution, ScopeId, ScopeKind, SymbolKind, SymbolRef};
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
        if !read.contains(&symbol) && !cx.linter.remote(symbol).0 && !excused(cx, symbol) {
            let what = match cx.hir()[symbol.symbol].kind {
                SymbolKind::Port(_) => "input",
                _ => "signal",
            };
            cx.report(cx.diagnostic(name.span, format!("{what} `{}` is never read", name.text)));
        }
    }
}

/// A parameter that nothing reads.
pub(crate) fn unused_parameter(cx: &mut DesignCx) {
    let read = read(cx);
    for symbol in declared(cx, |kind| matches!(kind, SymbolKind::Parameter(_))) {
        let name = &cx.hir()[symbol.symbol].name;
        if !read.contains(&symbol) && !cx.linter.remote(symbol).0 && !excused(cx, symbol) {
            let message = format!("parameter `{}` is never used", name.text);
            cx.report(cx.diagnostic(name.span, message));
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
                    cx.report(cx.diagnostic(item.span, message));
                }
                None if !used.iter().any(|(it, _)| *it == package) => {
                    let message = format!("nothing imported from `{package}` is used");
                    cx.report(cx.diagnostic(import.package.span, message));
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
/// `wanted` says; not the ports of a prototype, a subroutine's or an
/// `extern` definition's, which has no body to use them, nor what a class
/// holds, which any file may reach through a handle
/// and a class extending it may use.
pub(super) fn declared(cx: &DesignCx, wanted: impl Fn(&SymbolKind) -> bool) -> Vec<SymbolRef> {
    let hir = cx.hir();
    let mut found = Vec::new();
    for (id, scope) in hir.scopes() {
        if hir.enclosing(id, ScopeKind::Class).is_some() {
            continue;
        }
        let prototype = scope.owner.is_some_and(|owner| {
            matches!(
                hir[owner].kind,
                SymbolKind::Subroutine {
                    prototype: true,
                    ..
                } | SymbolKind::Definition {
                    prototype: true,
                    ..
                }
            )
        });
        if prototype
            || !matches!(
                definition(hir, id),
                Some(MODULE_KW | INTERFACE_KW | PROGRAM_KW)
            )
        {
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

/// Whether `symbol` is not to be reported however unused: named so, or
/// [`unseen`].
fn excused(cx: &DesignCx, symbol: SymbolRef) -> bool {
    let name = &cx.hir()[symbol.symbol].name;
    name.text.to_lowercase().contains("unused") || unseen(cx, symbol)
}

/// Whether `symbol` may be read or written where resolution does not
/// reach, or a macro wrote it, which its user can change only in the macro:
/// either way, what the design shows of its uses is not to be judged.
pub(super) fn unseen(cx: &DesignCx, symbol: SymbolRef) -> bool {
    let name = &cx.hir()[symbol.symbol].name;
    cx.written_by_macro(name.span) || cx.linter.maybe_used(cx.file, &name.text)
}

/// The keyword of the definition `scope` is in, if any, and if no envelope
/// encrypts it: the plaintext may read, drive or import anything in it.
pub(super) fn definition(hir: &Hir, scope: ScopeId) -> Option<SyntaxKind> {
    let owner = hir[hir.enclosing(scope, ScopeKind::Definition)?].owner?;
    match hir[owner].kind {
        SymbolKind::Definition {
            keyword, encrypted, ..
        } => (!encrypted).then_some(keyword),
        _ => None,
    }
}

/// Each package member a name in the file resolves to, as its package and
/// name: what the file's imports bring in and it uses. A `pkg::name` does
/// not count, since it needs no import.
fn brought<'a>(cx: &DesignCx<'a, '_>) -> FxHashSet<(&'a str, &'a str)> {
    let hir = cx.hir();
    let mut found = FxHashSet::default();
    let mut add = |resolution: Option<Resolution>| {
        if let Some(Resolution::Declared(symbol)) = resolution
            && let Some(package) = package_of(cx.design(), symbol)
        {
            found.insert((package, &*cx.design().symbol(symbol).name.text));
        }
    };
    for (id, expr) in hir.exprs() {
        if let astli_sema::ExprKind::Name(_) | astli_sema::ExprKind::Loose(_) = expr.kind {
            add(cx.names.expr(id));
        }
    }
    found
}

/// The package `symbol` is declared in, if it is a package's member.
fn package_of(design: &Design, symbol: SymbolRef) -> Option<&str> {
    let hir = &design[symbol.file];
    let package = hir[hir[symbol.symbol].scope].owner?;
    match hir[package].kind {
        SymbolKind::Definition {
            keyword: PACKAGE_KW,
            ..
        } => Some(&hir[package].name.text),
        _ => None,
    }
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
    !design.is_ambiguous(package) && !design[at.file][*scope].opaque
}
