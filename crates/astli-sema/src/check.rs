//! `check`: the errors in one file of a design that name resolution, the
//! definitions' ports and parameters, and the types reveal.

use astli_syntax::SyntaxKind::{self, *};
use astli_text::{Diagnostic, Severity, Span};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::access::{Driver, accesses};
use crate::built::{Built, Fate};
use crate::design::{Design, FileId, SymbolRef};
use crate::elab::Elaboration;
use crate::hir::*;
use crate::resolve::Resolution;
use crate::{diagnostics, drivers, typing};

/// The errors `file` of the design holds, in the order of the file: a name
/// declared nowhere, or only below its use, or twice in one scope, a
/// definition or package no file declares, a connection or override its
/// definition does not take, a net a procedure assigns, a `return` or a
/// wait where it may not stand, a variable driven
/// twice, a value whose type does not convert to where it is given, an
/// operand its operator does not take, a hierarchical name where a constant
/// is needed.
///
/// It reports only what it is sure of. Whatever sema cannot see into, it
/// says nothing about. Given what the tops build, an instance in a generate
/// block is judged by whether it is built: its errors are errors if it is,
/// and it says nothing if the definition is built and the block is not.
/// Without, such an instance may never be built, so its definition is not
/// looked for and what its connections get wrong is a warning. So is a
/// variable driven twice, or a type that does not convert, in a definition
/// no top reaches, found at its defaults; what the tops build is checked by
/// [`Built::multiple_drivers`] and [`Built::mistyped`].
pub fn check(elaboration: &Elaboration, file: FileId, built: Option<&Built>) -> Vec<Diagnostic> {
    let design = elaboration.design();
    let hir = &design[file];
    let names = design.names(file);
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
                // `C #(8)::name` names the class `C`.
                let head = match hir[*base].kind {
                    ExprKind::Call { callee, .. } => callee,
                    _ => *base,
                };
                let class = match names.expr(head) {
                    Some(Resolution::Declared(at)) => design.class_at(at),
                    _ => None,
                };
                match (&hir[head].kind, class) {
                    (_, Some(class)) => {
                        let class = Some(&*design.symbol(class).name.text);
                        found.push(diagnostics::not_in_class(class, &name.text, name.span));
                    }
                    (ExprKind::Name(package), None) => {
                        found.push(diagnostics::not_in_package(package, &name.text, name.span));
                    }
                    _ => {}
                }
            }
            // Of `this`, `super`, or a handle, `h[i].name` among them.
            ExprKind::Member { base, name } => {
                let (head, selects) = crate::resolve::selected(hir, *base);
                let class = match names.expr(head) {
                    Some(Resolution::Declared(at)) => design.class_of(at, selects),
                    _ => None,
                };
                let class = class.map(|class| &*design.symbol(class).name.text);
                found.push(diagnostics::not_in_class(class, &name.text, name.span));
            }
            _ => {}
        }
    }
    for (name, resolution) in names.loose() {
        if *resolution == Resolution::Undeclared {
            found.push(diagnostics::undeclared_name(&name.text, name.span));
        }
    }
    for &(at, symbol) in names.early() {
        let declared = &hir[symbol].name;
        found.push(diagnostics::used_before_declaration(
            &declared.text,
            at,
            declared.span,
        ));
    }

    // A packed struct's member given a default, wherever the struct is
    // written.
    let mut types: Vec<&Type> = Vec::new();
    for (_, symbol) in hir.symbols() {
        match &symbol.kind {
            SymbolKind::Typedef(ty)
            | SymbolKind::Port(Port { ty, .. })
            | SymbolKind::Parameter(Parameter { ty, .. })
            | SymbolKind::Net(Data { ty, .. })
            | SymbolKind::Variable(Data { ty, .. })
            | SymbolKind::Subroutine {
                returns: Some(ty), ..
            } => types.push(ty),
            _ => {}
        }
    }
    for (_, expr) in hir.exprs() {
        if let ExprKind::Type(ty) = &expr.kind {
            types.push(ty);
        }
    }
    while let Some(ty) = types.pop() {
        if let TypeKind::Struct { packed, fields, .. } = &ty.kind {
            for field in fields {
                if *packed && let Some(default) = field.default {
                    let at = hir[default].span;
                    found.push(diagnostics::packed_member_default(&field.name.text, at));
                }
                types.push(&field.ty);
            }
        }
    }

    // A `specparam` named in the value of a parameter that is not one.
    let values: Vec<Span> = (hir.symbols())
        .filter_map(|(_, symbol)| match &symbol.kind {
            SymbolKind::Parameter(Parameter {
                specparam: false,
                value: Some(value),
                ..
            }) => Some(hir[*value].span),
            _ => None,
        })
        .collect();
    for (id, expr) in hir.exprs() {
        let (ExprKind::Name(_), Some(Resolution::Declared(at))) = (&expr.kind, names.expr(id))
        else {
            continue;
        };
        let SymbolKind::Parameter(Parameter {
            specparam: true, ..
        }) = design.symbol(at).kind
        else {
            continue;
        };
        if values.iter().any(|value| value.contains(expr.span)) {
            let declared = &design.symbol(at).name;
            found.push(diagnostics::specparam_outside_specify(
                &declared.text,
                expr.span,
                declared.span,
            ));
        }
    }

    // Where each procedure stands, whose writes of a net are errors, and
    // each expression that must be constant.
    let mut procedures = FxHashMap::default();
    let mut constants: Vec<(ExprId, Generated)> = Vec::new();
    for (id, scope) in hir.scopes() {
        let generated = match generated(hir, id) {
            None => Generated::No,
            Some(block) => fate(design, file, block, built),
        };
        if generated == Generated::Never {
            continue;
        }
        let mut declared = FxHashMap::default();
        for member in &scope.members {
            match member {
                Member::Import(import) => imported(design, import, &mut found),
                Member::Process(process) => {
                    procedures.insert(Driver::Process(process.body), generated);
                }
                Member::GenerateIf(arms) => {
                    let conditions = arms.iter().flat_map(|arm| &arm.conditions);
                    constants.extend(conditions.map(|&expr| (expr, generated)));
                }
                Member::GenerateCase { selector, arms } => {
                    let labels = arms.iter().flat_map(|arm| &arm.conditions);
                    let exprs = std::iter::once(selector).chain(labels);
                    constants.extend(exprs.map(|&expr| (expr, generated)));
                }
                Member::GenerateFor { header, .. } => {
                    let exprs = header
                        .init
                        .iter()
                        .chain(&header.condition)
                        .chain(&header.step);
                    constants.extend(exprs.map(|&expr| (expr, generated)));
                }
                Member::Declare(symbol) => {
                    match &hir[*symbol].kind {
                        SymbolKind::Parameter(Parameter {
                            value: Some(value), ..
                        }) => constants.push((*value, generated)),
                        SymbolKind::Instance(instance) => {
                            let name = &hir[*symbol].name;
                            instantiated(design, instance, name, generated, &mut found);
                            let overrides =
                                instance.parameters.iter().filter_map(|arg| match arg {
                                    Arg::Positional(value) | Arg::Named { value, .. } => *value,
                                    _ => None,
                                });
                            constants.extend(overrides.map(|expr| (expr, generated)));
                        }
                        SymbolKind::Subroutine { .. } => {
                            procedures.insert(Driver::Subroutine(*symbol), generated);
                        }
                        _ => {}
                    }
                    if let Some(mut diagnostic) = redeclared(hir, *symbol, &mut declared) {
                        if generated == Generated::Maybe {
                            diagnostic.severity = Severity::Warning;
                        }
                        found.push(diagnostic);
                    }
                }
                Member::Bind(bind) => {
                    if let Some(target) = &bind.target
                        && generated != Generated::Maybe
                        && design.definition(&target.text).is_none()
                        && !design.spelled_opaque(&target.text)
                        && !names_instance(design, file, bind)
                    {
                        found.push(diagnostics::unknown_definition(&target.text, target.span));
                    }
                    for &symbol in &bind.instances {
                        if let SymbolKind::Instance(instance) = &hir[symbol].kind {
                            let name = &hir[symbol].name;
                            instantiated(design, instance, name, generated, &mut found);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // A name through the hierarchy in an expression that must be constant,
    // the whole of `a.b.c` rather than `a.b` too.
    for (id, expr) in hir.exprs() {
        let ExprKind::Member { base, name } = &expr.kind else {
            continue;
        };
        if !through_hierarchy(design, file, *base) || through_hierarchy(design, file, id) {
            continue;
        }
        let constant = constants
            .iter()
            .find(|(at, _)| hir[*at].span.contains(expr.span));
        if let Some(&(_, generated)) = constant {
            let mut diagnostic = diagnostics::not_constant(&name.text, expr.span);
            if generated == Generated::Maybe {
                diagnostic.severity = Severity::Warning;
            }
            found.push(diagnostic);
        }
    }

    // The names assignments write, rather than an argument a task may.
    let mut assigned = FxHashSet::default();
    for (_, expr) in hir.exprs() {
        match expr.kind {
            ExprKind::Assign { lhs, .. } => targets(hir, lhs, &mut assigned),
            ExprKind::Unary {
                op: PLUS_PLUS | MINUS_MINUS,
                operand,
            }
            | ExprKind::Postfix {
                op: PLUS_PLUS | MINUS_MINUS,
                operand,
            } => targets(hir, operand, &mut assigned),
            _ => {}
        }
    }
    for (&driver, &generated) in &procedures {
        for mut diagnostic in crate::control::control(hir, driver) {
            if generated == Generated::Maybe {
                diagnostic.severity = Severity::Warning;
            }
            found.push(diagnostic);
        }
    }
    for access in accesses(design, file, names) {
        let Some(&generated) = procedures.get(&access.driver) else {
            continue;
        };
        // What a clocking block or a hierarchical name writes is not told
        // apart here.
        if !access.write
            || !access.certain
            || access.base.is_some()
            || !assigned.contains(&access.at)
        {
            continue;
        }
        if let Some(direction) = net(design, access.symbol) {
            let declared = &design.symbol(access.symbol).name;
            let mut diagnostic = diagnostics::procedural_net_assignment(
                &declared.text,
                direction,
                access.at,
                declared.span,
            );
            if generated == Generated::Maybe {
                diagnostic.severity = Severity::Warning;
            }
            found.push(diagnostic);
        }
    }

    let reached = |symbol| built.is_some_and(|built| built.reaches(SymbolRef { file, symbol }));
    found.extend(drivers::at_defaults(elaboration, file, reached));
    found.extend(typing::at_defaults(elaboration, file, reached));

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
    // No file declares the built-in `std`.
    if design.package(&package.text).is_none() && &*package.text != "std" {
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

/// Where each name `expr`, the target of an assignment, writes is written:
/// a name, a select of one, or one in a concatenation or a pattern.
fn targets(hir: &Hir, expr: ExprId, found: &mut FxHashSet<Span>) {
    match &hir[expr].kind {
        ExprKind::Name(_) => {
            found.insert(hir[expr].span);
        }
        ExprKind::Select { base, .. } => targets(hir, *base, found),
        ExprKind::Concat(parts) => {
            for &part in parts {
                targets(hir, part, found);
            }
        }
        ExprKind::Pattern { items, .. } => {
            for item in items {
                targets(hir, item.value, found);
            }
        }
        ExprKind::Stream { concat, .. } => targets(hir, *concat, found),
        _ => {}
    }
}

/// `symbol`'s declaration, if it is the second of its name in a scope whose
/// first of each is in `declared`. A declaration that announces another, a
/// forward `typedef`, an `extern` or a prototype, is none; nor is a
/// constraint, which a class may declare with no body and define outside
/// it, nor a generate block's label, which each arm of one construct may
/// share, nor what sema leaves opaque.
fn redeclared(
    hir: &Hir,
    symbol: SymbolId,
    declared: &mut FxHashMap<Box<str>, SymbolId>,
) -> Option<Diagnostic> {
    let name = &hir[symbol].name;
    let exempt = |symbol: SymbolId| {
        matches!(
            hir[symbol].kind,
            SymbolKind::Forward
                | SymbolKind::Definition {
                    prototype: true,
                    ..
                }
                | SymbolKind::Subroutine {
                    prototype: true,
                    ..
                }
                | SymbolKind::Constraint(_)
                | SymbolKind::Block(_)
                | SymbolKind::Other(_)
        )
    };
    if name.text.is_empty() || exempt(symbol) {
        return None;
    }
    let first = *declared.entry(name.text.clone()).or_insert(symbol);
    (first != symbol && !exempt(first))
        .then(|| diagnostics::declared_twice(&name.text, name.span, hir[first].name.span))
}

/// Whether `at` is a net, and if so the direction of the port it is: a net
/// declared as one, or a port of a module, interface or program that is
/// not a variable, as an input or inout is but with `var`, and an output
/// only with a data type and no net type.
fn net(design: &Design, at: SymbolRef) -> Option<Option<SyntaxKind>> {
    let hir = &design[at.file];
    match &hir[at.symbol].kind {
        SymbolKind::Net(_) => Some(None),
        SymbolKind::Port(port) => {
            let home = design.home(at);
            let owner = hir[home]
                .owner
                .filter(|_| hir[home].kind == ScopeKind::Definition)?;
            let SymbolKind::Definition {
                keyword: MODULE_KW | INTERFACE_KW | PROGRAM_KW,
                ..
            } = hir[owner].kind
            else {
                return None;
            };
            let implicit = matches!(port.ty.kind, TypeKind::Implicit { .. });
            let direction = port.direction?;
            let net = match direction {
                INPUT_KW | INOUT_KW => port.keyword != Some(VAR_KW),
                OUTPUT_KW => match port.keyword {
                    Some(VAR_KW) => false,
                    Some(_) => true,
                    None => implicit,
                },
                _ => false,
            };
            net.then_some(Some(direction))
        }
        _ => None,
    }
}

/// Whether `expr`, past its selects, names a scope of the hierarchy: an
/// instance, a generate block, or a definition named from the top. An
/// interface port's member is not one, as it is the interface's.
fn through_hierarchy(design: &Design, file: FileId, expr: ExprId) -> bool {
    let (head, _) = crate::resolve::selected(&design[file], expr);
    let Some(Resolution::Declared(at)) = design.names(file).expr(head) else {
        return false;
    };
    match design.symbol(at).kind {
        SymbolKind::Instance(Instance { gate, .. }) => gate.is_none(),
        SymbolKind::Block(_) => true,
        SymbolKind::Definition { keyword, .. } => keyword != PACKAGE_KW,
        _ => false,
    }
}

/// Whether an instance stands in a generate block, and whether that is
/// built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Generated {
    No,
    Built,
    /// Maybe, for all sema knows.
    Maybe,
    /// Never, in a definition the tops build.
    Never,
}

/// The innermost generate block `scope` is inside, if any.
fn generated(hir: &Hir, scope: ScopeId) -> Option<ScopeId> {
    let mut at = Some(scope);
    while let Some(scope) = at {
        match hir[scope].kind {
            ScopeKind::Generate => return Some(scope),
            ScopeKind::Definition | ScopeKind::File => return None,
            ScopeKind::Loop
            | ScopeKind::Subroutine
            | ScopeKind::Block
            | ScopeKind::Property
            | ScopeKind::Clocking
            | ScopeKind::Covergroup
            | ScopeKind::Class => {}
        }
        at = hir[scope].parent;
    }
    None
}

/// Whether the generate block `block` of `file` is built: if it or one
/// around it is undecided, maybe; otherwise never, if its definition is
/// built.
fn fate(design: &Design, file: FileId, block: ScopeId, built: Option<&Built>) -> Generated {
    let Some(built) = built else {
        return Generated::Maybe;
    };
    let hir = &design[file];
    if built.fate(file, block) == Some(Fate::Built) {
        return Generated::Built;
    }
    let mut at = Some(block);
    while let Some(scope) = at {
        match hir[scope].kind {
            ScopeKind::Definition => {
                let reached = hir[scope]
                    .owner
                    .is_some_and(|symbol| built.reaches(SymbolRef { file, symbol }));
                return if reached {
                    Generated::Never
                } else {
                    Generated::Maybe
                };
            }
            ScopeKind::File => return Generated::Maybe,
            _ if built.fate(file, scope) == Some(Fate::Undecided) => return Generated::Maybe,
            _ => {}
        }
        at = hir[scope].parent;
    }
    Generated::Maybe
}

/// An instance of a definition no file declares, or whose connections or
/// overrides its definition does not take. One that may not be built may
/// stand in a branch elaboration never takes, for a definition no file
/// needs to declare, so its definition is not looked for and what its
/// connections get wrong is a warning.
fn instantiated(
    design: &Design,
    instance: &Instance,
    name: &Name,
    generated: Generated,
    found: &mut Vec<Diagnostic>,
) {
    if instance.gate.is_some() {
        return;
    }
    let generated = generated == Generated::Maybe;
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
        keyword,
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
    // What a UDP's instance gives in `#( … )` is its delay.
    if *keyword != PRIMITIVE_KW {
        parameters.given(&instance.parameters, definition.span, &mut wrong);
    }
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

/// Whether the target of `bind`, a name alone that names no definition,
/// names an instance.
fn names_instance(design: &Design, file: FileId, bind: &Bind) -> bool {
    let &[path] = bind.paths.as_slice() else {
        return false;
    };
    match design.names(file).expr(path) {
        Some(Resolution::Declared(at)) => {
            matches!(design.symbol(at).kind, SymbolKind::Instance(_))
        }
        _ => false,
    }
}
