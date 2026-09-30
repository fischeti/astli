//! `check`: the errors in one file of a design that name resolution and the
//! definitions' ports and parameters reveal.

use astli_text::{Diagnostic, Severity, Span};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::design::{Design, FileId};
use crate::diagnostics;
use crate::hir::*;
use crate::resolve::Resolution;

/// The errors `file` of `design` holds, in the order of the file: a name
/// declared nowhere, a definition or package no file declares, a
/// connection or override its definition does not take.
///
/// It reports only what it is sure of. Whatever sema cannot see into, it
/// says nothing about.
pub fn check(design: &Design, file: FileId) -> Vec<Diagnostic> {
    let hir = &design[file];
    let names = design.resolve(file);
    let mut found = Vec::new();

    // The name before `::` is a package, or a class.
    let packages: FxHashSet<ExprId> = (hir.exprs())
        .filter_map(|(_, expr)| match expr.kind {
            ExprKind::Scoped { base, .. } => Some(base),
            _ => None,
        })
        .collect();
    for (id, expr) in hir.exprs() {
        if names.expr(id) != Some(Resolution::Undeclared) {
            continue;
        }
        match &expr.kind {
            ExprKind::Name(name) if packages.contains(&id) => {
                found.push(diagnostics::unknown_package(name, expr.span));
            }
            ExprKind::Name(name) => found.push(diagnostics::undeclared_name(name, expr.span)),
            ExprKind::Scoped { base, name } => {
                if let ExprKind::Name(package) = &hir[*base].kind {
                    found.push(diagnostics::not_in_package(package, &name.text, name.span));
                }
            }
            _ => {}
        }
    }
    for (name, resolution) in names.loose() {
        if *resolution == Resolution::Undeclared {
            found.push(diagnostics::undeclared_name(&name.text, name.span));
        }
    }

    for (id, scope) in hir.scopes() {
        let generated = generated(hir, id);
        for member in &scope.members {
            match member {
                Member::Import(import) => imported(design, import, &mut found),
                Member::Declare(symbol) => {
                    if let SymbolKind::Instance(instance) = &hir[*symbol].kind {
                        let name = &hir[*symbol].name;
                        instantiated(design, instance, name, generated, &mut found);
                    }
                }
                _ => {}
            }
        }
    }

    found.sort_by_key(|diagnostic| (diagnostic.at.src_id, diagnostic.at.start, diagnostic.code));
    found.dedup();
    found
}

/// An import or export of a package no file declares, or of a name its
/// package lacks.
fn imported(design: &Design, import: &Import, found: &mut Vec<Diagnostic>) {
    let package = &import.package;
    // `export *::*` names no package.
    if &*package.text == "*" {
        return;
    }
    if design.package(&package.text).is_none() {
        if !design.spelled_opaque(&package.text) {
            found.push(diagnostics::unknown_package(&package.text, package.span));
        }
        return;
    }
    if let Some(item) = &import.item
        && design.member_of(&package.text, &item.text, 0) == Resolution::Undeclared
    {
        found.push(diagnostics::not_in_package(
            &package.text,
            &item.text,
            item.span,
        ));
    }
}

/// Whether `scope` is inside a generate construct, which elaboration may
/// never build.
fn generated(hir: &Hir, scope: ScopeId) -> bool {
    let mut at = Some(scope);
    while let Some(scope) = at {
        match hir[scope].kind {
            ScopeKind::Generate | ScopeKind::Loop => return true,
            ScopeKind::Definition | ScopeKind::File => return false,
            ScopeKind::Subroutine | ScopeKind::Block => {}
        }
        at = hir[scope].parent;
    }
    false
}

/// An instance of a definition no file declares, or whose connections or
/// overrides its definition does not take. One `generated` may stand in a
/// branch elaboration never takes, for a definition no file needs to
/// declare, so its definition is not looked for and what its connections
/// get wrong is a warning.
fn instantiated(
    design: &Design,
    instance: &Instance,
    name: &Name,
    generated: bool,
    found: &mut Vec<Diagnostic>,
) {
    let definition = &instance.definition;
    let Some(at) = design.definition(&definition.text) else {
        if !generated && !design.spelled_opaque(&definition.text) {
            found.push(diagnostics::unknown_definition(
                &definition.text,
                definition.span,
            ));
        }
        return;
    };
    let SymbolKind::Definition {
        scope,
        ports,
        ports_known,
        parameters,
        parameters_known,
        ..
    } = &design.symbol(at).kind
    else {
        return;
    };
    let hir = &design[at.file];
    let names = |ids: &[SymbolId]| ids.iter().map(|&id| &*hir[id].name.text).collect();
    // A parameter port list may hold local parameters too.
    let overridable: Vec<SymbolId> = (parameters.iter().copied())
        .filter(|&id| matches!(&hir[id].kind, SymbolKind::Parameter(it) if !it.local))
        .collect();
    let local = (hir[*scope].members.iter())
        .filter_map(|member| match member {
            Member::Declare(id) => Some(&hir[*id]),
            _ => None,
        })
        .filter(|symbol| matches!(&symbol.kind, SymbolKind::Parameter(it) if it.local))
        .map(|symbol| &*symbol.name.text)
        .collect();

    let ports = Takes {
        what: "port",
        definition: &definition.text,
        names: names(ports),
        local: Vec::new(),
        known: *ports_known,
    };
    let mut wrong = Vec::new();
    ports.given(&instance.connections, name.span, &mut wrong);
    let parameters = Takes {
        what: "parameter",
        definition: &definition.text,
        names: names(&overridable),
        local,
        known: *parameters_known,
    };
    parameters.given(&instance.parameters, definition.span, &mut wrong);
    // The standard makes it an error once the instance is built, which an
    // instance in a generate branch may never be.
    if generated {
        for diagnostic in &mut wrong {
            diagnostic.severity = Severity::Warning;
            let note = "in a generate construct, so an error only if elaboration builds it";
            diagnostic.notes.push(note.to_string());
        }
    }
    found.append(&mut wrong);
}

/// What a definition takes: its ports, or the parameters an instance may
/// override.
struct Takes<'a> {
    what: &'static str,
    definition: &'a str,
    names: Vec<&'a str>,
    /// Local parameters, which exist and cannot be overridden.
    local: Vec<&'a str>,
    /// Whether `names` is all of them.
    known: bool,
}

impl Takes<'_> {
    /// Checks `args` against what is taken; `at` is where a count is
    /// reported.
    fn given(&self, args: &[Arg], at: Span, found: &mut Vec<Diagnostic>) {
        let positional = args
            .iter()
            .filter(|arg| matches!(arg, Arg::Positional(_)))
            .count();
        if self.known && positional > self.names.len() {
            let has = self.names.len();
            found.push(diagnostics::too_many(
                self.what,
                self.definition,
                positional,
                has,
                at,
            ));
        }
        let mut seen: FxHashMap<&str, Span> = FxHashMap::default();
        for arg in args {
            let name = match arg {
                Arg::Named { name, .. } | Arg::Implicit(name) => name,
                Arg::Positional(_) | Arg::Wildcard(_) => continue,
            };
            if let Some(&first) = seen.get(&*name.text) {
                found.push(diagnostics::connected_twice(
                    self.what, &name.text, name.span, first,
                ));
                continue;
            }
            seen.insert(&name.text, name.span);
            let text = &*name.text;
            if self.names.contains(&text) {
                continue;
            }
            if self.local.contains(&text) {
                found.push(diagnostics::local_parameter(
                    self.definition,
                    text,
                    name.span,
                ));
            } else if self.known {
                let definition = self.definition;
                found.push(diagnostics::unknown_connection(
                    self.what, definition, text, name.span,
                ));
            }
        }
    }
}
