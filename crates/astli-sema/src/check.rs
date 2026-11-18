//! `check`: the errors in one file of a design that name resolution, the
//! definitions' ports and parameters, and the types reveal.

use astli_syntax::SyntaxKind::{self, *};
use astli_text::{Diagnostic, Severity, Span};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::access::{Driver, Write, accesses};
use crate::built::{Built, Fate};
use crate::design::{Design, FileId, SymbolRef};
use crate::elab::{BodyId, ChildKind, Elaboration};
use crate::hir::*;
use crate::resolve::{Resolution, Searched};
use crate::{bind, diagnostics, drivers, typing};

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
/// [`check_built`].
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
        let Some(Resolution::Undeclared(searched)) = names.expr(id) else {
            continue;
        };
        let class = |class: SymbolRef| &*design.symbol(class).name.text;
        match (&expr.kind, searched) {
            (ExprKind::Name(name), _) if packages.contains(&id) => {
                found.push(diagnostics::unknown_package(name, expr.span));
            }
            (ExprKind::Name(name), _) => {
                found.push(diagnostics::undeclared_name(name, expr.span));
            }
            (ExprKind::Scoped { name, .. }, Searched::Class(at)) => {
                found.push(diagnostics::not_in_class(
                    Some(class(at)),
                    &name.text,
                    name.span,
                ));
            }
            // Not `$unit`, which a file declares in part.
            (ExprKind::Scoped { base, name }, Searched::Package) => {
                if let ExprKind::Name(package) = &hir[*base].kind {
                    found.push(diagnostics::not_in_package(package, &name.text, name.span));
                }
            }
            // A handle's member, `h[i].name` among them, or one of `this`
            // or `super`, which names no class.
            (ExprKind::Member { base, name }, Searched::Class(at)) => {
                let class = match hir[*base].kind {
                    ExprKind::Keyword(_) => None,
                    _ => Some(class(at)),
                };
                found.push(diagnostics::not_in_class(class, &name.text, name.span));
            }
            _ => {}
        }
    }
    for (name, resolution) in names.loose() {
        if let Resolution::Undeclared(_) = resolution {
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
                kind,
                value: Some(value),
                ..
            }) if *kind != ParameterKind::Specparam => Some(hir[*value].span),
            _ => None,
        })
        .collect();
    for (id, expr) in hir.exprs() {
        let (ExprKind::Name(_), Some(Resolution::Declared(at))) = (&expr.kind, names.expr(id))
        else {
            continue;
        };
        let SymbolKind::Parameter(Parameter {
            kind: ParameterKind::Specparam,
            ..
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
        let generated = match hir.enclosing(id, ScopeKind::Generate) {
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
                            let at = SymbolRef {
                                file,
                                symbol: *symbol,
                            };
                            instantiated(design, at, instance, generated, &mut found);
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
                            let at = SymbolRef { file, symbol };
                            instantiated(design, at, instance, generated, &mut found);
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
        // What an assignment writes, rather than an argument a task may;
        // what a clocking block or a hierarchical name writes is not told
        // apart here.
        if access.write != Some(Write::Assign) || !access.certain || access.base.is_some() {
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

/// The errors in what `built` builds that only its parameters' values
/// reveal, each with its file: a variable two things drive where the
/// standard allows one, a value whose type does not convert to where it is
/// given or an operand its operator does not take, an `$error` the design
/// runs while elaborating, and a parameter without a default left without
/// a value. Each body is checked with its parameters, each generate block
/// built a place of its own, so a loop that writes all of a variable drives
/// it once per iteration.
pub fn check_built(elaboration: &Elaboration, built: &Built) -> Vec<(FileId, Diagnostic)> {
    let mut found = drivers::built(built, elaboration);
    found.extend(typing::built(built, elaboration, false));
    found.extend(elaboration_tasks(elaboration, built));
    found.extend(unset_parameters(elaboration, built));
    found
}

/// Each elaboration system task in what is built, with what its
/// arguments say in the body it runs in: a `$fatal` or an `$error` an
/// error, a `$warning` a warning. An `$info` reports nothing wrong, so
/// it is not told.
fn elaboration_tasks(elaboration: &Elaboration, built: &Built) -> Vec<(FileId, Diagnostic)> {
    let mut found = Vec::new();
    let mut told = FxHashSet::default();
    for frame in built.frames() {
        let hir = &elaboration.design()[frame.file];
        for member in &hir[frame.scope].members {
            let Member::ElaborationTask(task) = member else {
                continue;
            };
            let (callee, args) = match &hir[*task].kind {
                ExprKind::Call { callee, args } => (*callee, args.as_slice()),
                _ => (*task, &[][..]),
            };
            let ExprKind::System(name) = &hir[callee].kind else {
                continue;
            };
            // `$fatal`'s first argument says how the simulation ends.
            let args = match &**name {
                "$info" => continue,
                "$fatal" => args.get(1..).unwrap_or_default(),
                _ => args,
            };
            let message = elaboration.message(frame.body, args);
            let at = hir[*task].span;
            // Bodies built with other parameters may say the same.
            if told.insert((frame.file, at, message.clone())) {
                let diagnostic = diagnostics::elaboration_task(name, message, &frame.path, at);
                found.push((frame.file, diagnostic));
            }
        }
    }
    found
}

/// Each parameter without a default that what is built leaves without
/// a value: one no instance overrides and no `defparam` sets, and a
/// top's that is not set from outside the design. A `defparam` whose
/// path sema cannot follow, through an index it cannot evaluate, may
/// still set it, so a parameter of a name any `defparam` sets is not
/// told.
fn unset_parameters(elaboration: &Elaboration, built: &Built) -> Vec<(FileId, Diagnostic)> {
    let design = elaboration.design();
    let defparams = defparam_names(elaboration);
    let mut found = Vec::new();
    for &top in built.tops() {
        let (file, scope) = elaboration.scope(top);
        let Some(owner) = design[file][scope].owner else {
            continue;
        };
        let definition = SymbolRef {
            file,
            symbol: owner,
        };
        let name = &design.symbol(definition).name.text;
        for parameter in unset(elaboration, top, definition, &defparams) {
            let parameter = &design[file][parameter].name;
            let diagnostic =
                diagnostics::unset_top_parameter(name, &parameter.text, parameter.span);
            found.push((file, diagnostic));
        }
    }
    // Instances built with other parameters around them may leave out
    // the same.
    let mut told = FxHashSet::default();
    for frame in built.frames() {
        for child in elaboration.children(frame.body).iter() {
            let ChildKind::Instance {
                symbol,
                definition,
                body,
                certain: true,
                ..
            } = child.kind
            else {
                continue;
            };
            let instance = &design.symbol(symbol).name;
            for parameter in unset(elaboration, body, definition, &defparams) {
                if !told.insert((symbol, parameter)) {
                    continue;
                }
                let parameter = &design[definition.file][parameter].name;
                let diagnostic = diagnostics::unset_parameter(
                    &instance.text,
                    &parameter.text,
                    instance.span,
                    parameter.span,
                );
                found.push((symbol.file, diagnostic));
            }
        }
    }
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
        && let Resolution::Undeclared(_) = design.member_of(&package.text, &item.text, 0)
    {
        found.push(diagnostics::not_in_package(
            &package.text,
            &item.text,
            item.span,
        ));
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
            let home = design.symbol(at).scope;
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
    symbol: SymbolRef,
    instance: &Instance,
    generated: Generated,
    found: &mut Vec<Diagnostic>,
) {
    if instance.gate.is_some() {
        return;
    }
    let name = &design.symbol(symbol).name;
    let generated = generated == Generated::Maybe;
    let definition = &instance.definition;
    let Some(at) = design.instantiated(symbol) else {
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
        overridable,
        parameters_known,
        ..
    } = &design.symbol(at).kind
    else {
        return;
    };
    let hir = &design[at.file];
    // A local parameter exists, but cannot be overridden.
    let local = |name: &str| {
        design.declared(at.file, *scope, name).is_some_and(
            |symbol| matches!(&hir[symbol].kind, SymbolKind::Parameter(it) if it.is_local()),
        )
    };

    let ports = Takes {
        what: "port",
        definition: &definition.text,
        hir,
        formals: ports,
        local: &|_| false,
        known: *ports_known,
    };
    let mut wrong = Vec::new();
    ports.given(&instance.connections, name.span, &mut wrong);
    let parameters = Takes {
        what: "parameter",
        definition: &definition.text,
        hir,
        formals: overridable,
        local: &local,
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
    /// Where `formals` are declared.
    hir: &'a Hir,
    formals: &'a [SymbolId],
    /// Whether a name is a local parameter, which exists and cannot be
    /// overridden.
    local: &'a dyn Fn(&str) -> bool,
    /// Whether `formals` is all of them.
    known: bool,
}

impl Takes<'_> {
    /// Checks `args` against what is taken; `at` is where a count is
    /// reported.
    fn given(&self, args: &[Arg], at: Span, found: &mut Vec<Diagnostic>) {
        let bound = bind::formals(self.hir, self.formals, args);
        let has = self.formals.len();
        let extra = (args.iter().zip(&bound))
            .filter(|(arg, formal)| matches!(arg, Arg::Positional(_)) && formal.is_none())
            .count();
        if self.known && extra > 0 {
            found.push(diagnostics::too_many(
                self.what,
                self.definition,
                has + extra,
                has,
                at,
            ));
        }
        let mut seen: FxHashMap<&str, Span> = FxHashMap::default();
        for (arg, formal) in args.iter().zip(&bound) {
            let (Arg::Named { name, .. } | Arg::Implicit(name)) = arg else {
                continue;
            };
            if let Some(&first) = seen.get(&*name.text) {
                found.push(diagnostics::connected_twice(
                    self.what, &name.text, name.span, first,
                ));
                continue;
            }
            seen.insert(&name.text, name.span);
            if formal.is_some() {
                continue;
            }
            let text = &*name.text;
            if (self.local)(text) {
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

/// The parameters of `definition` without a default that `body`, built of
/// it, is given no value for. A prototype's are its definition's, which
/// another file declares.
fn unset(
    elaboration: &Elaboration,
    body: BodyId,
    definition: SymbolRef,
    defparams: &FxHashSet<Box<str>>,
) -> Vec<SymbolId> {
    let design = elaboration.design();
    let SymbolKind::Definition {
        overridable,
        parameters_known: true,
        prototype: false,
        ..
    } = &design.symbol(definition).kind
    else {
        return Vec::new();
    };
    let hir = &design[definition.file];
    (overridable.iter().copied())
        .filter(|&id| {
            matches!(
                &hir[id].kind,
                SymbolKind::Parameter(Parameter { value: None, .. })
            )
        })
        .filter(|&id| !defparams.contains(&hir[id].name.text))
        .filter(|&id| !elaboration.is_given(body, id))
        .collect()
}

/// The name each `defparam` in the design sets, the last of its path.
fn defparam_names(elaboration: &Elaboration) -> FxHashSet<Box<str>> {
    let mut names = FxHashSet::default();
    for (_, hir) in elaboration.design().files() {
        for (_, scope) in hir.scopes() {
            for member in &scope.members {
                let Member::Defparam(assignments) = member else {
                    continue;
                };
                for &assignment in assignments {
                    let ExprKind::Assign { lhs, .. } = hir[assignment].kind else {
                        continue;
                    };
                    match &hir[lhs].kind {
                        ExprKind::Member { name, .. } => names.insert(name.text.clone()),
                        ExprKind::Name(name) => names.insert(name.clone()),
                        _ => continue,
                    };
                }
            }
        }
    }
    names
}
