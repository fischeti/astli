//! Name resolution: what each name a file uses refers to.
//!
//! A name is looked for in its scope and then each around it, up to the
//! file's: first what the scope declares, then the nets it declares
//! implicitly, then what it imports by name, then what it imports by
//! wildcard. In a class, what it inherits comes after what it declares;
//! where a class it extends is not known, the name may be one, so a name
//! nothing declares is unknown, but one declared around the class is found
//! there. Past the file, a name may be a module, interface or program of
//! the design. `pkg::name` is looked for in the package alone, and what the
//! package exports; `C::name`, `this.name` and `super.name` in the class and
//! what it inherits.
//!
//! A name declared in a scope is declared from where its declaration
//! stands, or a forward typedef of it, to the end of the scope; a name
//! used above its declaration is looked for around the scope, and refers
//! to the declaration below only if nothing else does. A subroutine, a
//! property or a sequence, a block's name and a cover point are declared in
//! the whole of their scope, and so is everything for a name called, or
//! the head of `a.b`.
//!
//! Only the head of `a.b.c` is resolved: what `b` is depends on `a`'s type,
//! or on the instance tree, which elaboration builds.

use astli_syntax::SyntaxKind::{
    COVERPOINT_KW, INTERFACE_KW, LOCAL_KW, PACKAGE_KW, SUPER_KW, THIS_KW,
};
use astli_text::Span;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::bind;
use crate::design::{BindTarget, Design, FileId, Filter, SymbolRef};
use crate::hir::*;

/// What a name refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// A declaration, in this file or another.
    Declared(SymbolRef),
    /// A net its first use declared, by connecting it to a port or assigning
    /// it continuously: that first use.
    Implicit(ExprId),
    /// Maybe declared where sema cannot see: in a region not lowered, in a
    /// package no file declares or two do, up the instance tree.
    Unknown,
    /// Declared nowhere it could be: where it was looked for says what is
    /// missing.
    Undeclared(Searched),
}

/// Where a name declared nowhere was looked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Searched {
    /// The scopes around it, what they import, and the definitions.
    Scopes,
    /// A package's members: of the package before `::`, `std` among them,
    /// or of `$unit`.
    Package,
    /// A class's members, declared or inherited: of the class before `::`,
    /// of the one a handle holds, or of the one `this` or `super` is in.
    Class(SymbolRef),
}

/// What the names one file uses refer to.
#[derive(Debug, Clone)]
pub struct Names {
    exprs: Vec<Option<Resolution>>,
    loose: Vec<ExprId>,
    /// What each port of a `.*` connects, by where the `.*` is written:
    /// each port the instance does not connect otherwise, and what its
    /// name means where the instance stands.
    wildcards: FxHashMap<Span, Vec<(SymbolRef, Resolution)>>,
    /// Where each name used above its declaration is written, and the
    /// declaration.
    early: Vec<(Span, SymbolId)>,
    /// Each `a.b` left unresolved, a struct's member or a method of a value,
    /// and the scope it stands in, whose body its type is found in.
    members: Vec<(ExprId, ScopeId)>,
}

impl Names {
    /// What `expr` refers to, if it is a name, loose or not, a `pkg::name`,
    /// or a member `a.b` of an instance, a generate block or an interface
    /// port; `None` for any other expression, and for a struct's member.
    pub fn expr(&self, expr: ExprId) -> Option<Resolution> {
        self.exprs.get(expr.index()).copied().flatten()
    }

    /// The names opaque regions spell, each [`Loose`](ExprKind::Loose):
    /// `expr` says what each refers to, never [`Resolution::Undeclared`]
    /// since what it is is not known.
    pub fn loose(&self) -> &[ExprId] {
        &self.loose
    }

    /// The names used above their declaration where nothing declares them
    /// before: where each is written, and the declaration below it.
    pub(crate) fn early(&self) -> &[(Span, SymbolId)] {
        &self.early
    }

    /// What the `.*` written at `at` connects to each port left to it.
    pub fn wildcard(&self, at: Span) -> &[(SymbolRef, Resolution)] {
        self.wildcards.get(&at).map_or(&[], Vec::as_slice)
    }

    /// Each `a.b` that names no symbol, and the scope it stands in.
    pub(crate) fn members(&self) -> &[(ExprId, ScopeId)] {
        &self.members
    }
}

/// Names from the `std` package, which every scope imports.
pub(crate) const STD: &[&str] = &[
    "process",
    "semaphore",
    "mailbox",
    "randomize",
    "weak_reference",
];

/// The methods every class has without declaring them.
pub(crate) const CLASS_METHODS: &[&str] = &[
    "new",
    "randomize",
    "srandom",
    "get_randstate",
    "set_randstate",
    "rand_mode",
    "constraint_mode",
    "pre_randomize",
    "post_randomize",
];

/// The object an object's `randomize` randomizes, whose members its
/// arguments and constraints name.
#[derive(Clone, Copy)]
pub(crate) struct Object<'d> {
    /// The scope the call stands in, whose names come after the object's
    /// members.
    pub call: ScopeId,
    /// The object's class, `None` where it is not known.
    pub class: Option<SymbolRef>,
    /// `with (x)`, the only names that may be the object's members; `None`
    /// where any may be.
    pub only: Option<&'d [Name]>,
}

/// Where the names the walk meets are looked up.
#[derive(Clone, Copy)]
enum Within {
    /// Where they are written.
    Written,
    /// The scope of the definition an instance is bound into, whose names
    /// the instance's overrides and connections are.
    Target(FileId, ScopeId),
    /// Nowhere, for an instance bound into a definition no file declares:
    /// each name is unknown.
    Unknown,
}

/// Where an expression stands, which decides what its name may be when it
/// is declared nowhere in sight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    Value,
    /// The head of `a.b`, which may be a name up the instance tree.
    Head,
    /// A task or function called, which may be one up the instance tree.
    Callee,
    /// An assignment pattern's key, which may be a struct member.
    Key,
}

pub(crate) struct Resolver<'d> {
    design: &'d Design,
    file: FileId,
    hir: &'d Hir,
    /// The nets each scope declares implicitly, by their first use.
    implicit: FxHashMap<ScopeId, FxHashMap<Box<str>, ExprId>>,
    /// Where the names the walk meets are looked up: a bound instance's in
    /// its target.
    within: Within,
    /// While resolving an object's `randomize() with {…}` or the arguments
    /// of its `randomize(…)`, where the object's members are looked up.
    object: Option<Object<'d>>,
    /// For each symbol, the member of its scope from which its name is
    /// declared: its own, or a forward typedef's above it.
    since: Vec<u32>,
    /// For each scope among its members, the member the walk is at, below
    /// which nothing is declared yet; `None` for one the walk is not among
    /// the members of, where everything is.
    reached: Vec<Option<u32>>,
    names: Names,
}

impl<'d> Resolver<'d> {
    pub(crate) fn new(design: &'d Design, file: FileId) -> Resolver<'d> {
        let hir = &design[file];
        let mut since = vec![0; hir.symbols.len()];
        for (_, scope) in hir.scopes() {
            let mut first = FxHashMap::default();
            for (at, member) in scope.members.iter().enumerate() {
                if let Member::Declare(symbol) = member {
                    let name = &*hir[*symbol].name.text;
                    since[symbol.index()] = *first.entry(name).or_insert(at as u32);
                }
            }
        }
        Resolver {
            design,
            file,
            hir,
            implicit: FxHashMap::default(),
            within: Within::Written,
            object: None,
            since,
            reached: vec![None; hir.scopes.len()],
            names: Names {
                exprs: vec![None; hir.exprs.len()],
                loose: Vec::new(),
                wildcards: FxHashMap::default(),
                early: Vec::new(),
                members: Vec::new(),
            },
        }
    }

    pub(crate) fn run(mut self) -> Names {
        self.implicit_nets();
        self.scope(self.hir.root());
        self.names
    }

    // ---------------------------------------------------------------- lookup

    fn here(&self, symbol: SymbolId) -> Resolution {
        Resolution::Declared(SymbolRef {
            file: self.file,
            symbol,
        })
    }

    /// What `name`, used anywhere in `scope`, refers to.
    fn lookup(&self, scope: ScopeId, name: &str) -> Resolution {
        self.find(scope, name, false).0
    }

    /// What `name` refers to in `scope`: used where the walk is if
    /// `ordered`, so that what a scope declares below the use comes after
    /// what is declared around the scope. Where that is what it refers to,
    /// as nothing else declares it, nor may, it is also given back.
    fn find(&self, scope: ScopeId, name: &str, ordered: bool) -> (Resolution, Option<SymbolId>) {
        match self.within {
            Within::Written => {}
            // What the target does not declare may be a net it declares
            // implicitly, so it is unknown rather than undeclared.
            Within::Target(file, scope) => {
                let found = match self.design.lookup(file, scope, name, Filter::default()) {
                    Resolution::Undeclared(_) => Resolution::Unknown,
                    found => found,
                };
                return (found, None);
            }
            Within::Unknown => return (Resolution::Unknown, None),
        }
        let declared = |scope, symbol| self.declared_yet(scope, symbol);
        let filter = Filter {
            declared: ordered.then_some(&declared as &dyn Fn(ScopeId, SymbolId) -> bool),
            object: self.object,
            ..Filter::default()
        };
        let found = self.design.lookup(self.file, scope, name, filter);
        if let Resolution::Declared(_) = found {
            return (found, None);
        }
        // A net is declared implicitly only where nothing in or around its
        // scope declares the name, nor may, so a use in that scope or one
        // inside it that finds no declaration refers to it.
        let mut at = Some(scope);
        while let Some(scope) = at {
            if let Some(&first) = self.implicit.get(&scope).and_then(|nets| nets.get(name)) {
                return (Resolution::Implicit(first), None);
            }
            at = self.hir[scope].parent;
        }
        match (found, ordered.then(|| self.below(scope, name)).flatten()) {
            (Resolution::Undeclared(_), Some(symbol)) => (self.here(symbol), Some(symbol)),
            // What sema cannot see may declare it above, but whether it
            // does is not known, and the declaration below is what the
            // name most likely means.
            (Resolution::Unknown, Some(symbol)) => (self.here(symbol), None),
            (found, _) => (found, None),
        }
    }

    /// The innermost declaration of `name` in `scope` or around it that
    /// stands below where the walk is.
    fn below(&self, scope: ScopeId, name: &str) -> Option<SymbolId> {
        let mut at = Some(scope);
        while let Some(scope) = at {
            if let Some(symbol) = self.design.declared(self.file, scope, name)
                && !self.declared_yet(scope, symbol)
            {
                return Some(symbol);
            }
            at = self.hir[scope].parent;
        }
        None
    }

    /// Whether `symbol`, which `scope` declares, is declared where the walk
    /// is in `scope`.
    fn declared_yet(&self, scope: ScopeId, symbol: SymbolId) -> bool {
        let Some(at) = self.reached[scope.index()] else {
            return true;
        };
        let anywhere = matches!(
            self.hir[symbol].kind,
            SymbolKind::Subroutine { .. }
                | SymbolKind::Property { .. }
                | SymbolKind::Block(_)
                | SymbolKind::Label(_)
                | SymbolKind::Coverpoint(_)
        );
        anywhere || self.since[symbol.index()] <= at
    }

    /// Whether `member` of `scope` is a method or a constraint defined
    /// outside its class, `C::f`, which the class declares first by its
    /// prototype.
    fn defined_outside(&self, scope: ScopeId, member: &Member) -> bool {
        let &Member::Declare(symbol) = member else {
            return false;
        };
        self.hir[scope].kind == ScopeKind::Class
            && matches!(
                self.hir[symbol].kind,
                SymbolKind::Subroutine { .. } | SymbolKind::Constraint(_)
            )
            && self
                .design
                .declared(self.file, scope, &self.hir[symbol].name.text)
                != Some(symbol)
    }

    /// The class whose scope `scope` is, if it is one.
    fn class(&self, scope: ScopeId) -> Option<SymbolRef> {
        let owner = self.hir[scope].owner?;
        (self.hir[scope].kind == ScopeKind::Class).then_some(SymbolRef {
            file: self.file,
            symbol: owner,
        })
    }

    /// `this.name` or `super.name`, `base` the keyword, used in `scope`:
    /// a member of the class around it, or one it inherits.
    fn this(&self, scope: ScopeId, base: ExprId, name: &str) -> Resolution {
        let class = self.hir.enclosing(scope, ScopeKind::Class);
        let Some(class) = class.and_then(|it| self.class(it)) else {
            return Resolution::Unknown;
        };
        let found = match self.hir[base].kind {
            ExprKind::Keyword(SUPER_KW) => self.design.inherited(class, name),
            _ => self.design.class_member(class, name),
        };
        match found {
            Resolution::Undeclared(_) if CLASS_METHODS.contains(&name) => Resolution::Unknown,
            found => found,
        }
    }

    /// Declares the nets names declare by their first use: an undeclared
    /// name read in a port's connection, or assigned continuously, where
    /// the definition allows it.
    fn implicit_nets(&mut self) {
        let hir = self.hir;
        for (scope, body) in hir.scopes() {
            if !self.allows_implicit(scope) {
                continue;
            }
            for member in &body.members {
                let mut names = Vec::new();
                match member {
                    Member::Assign(assignments) => {
                        for &assignment in assignments {
                            if let ExprKind::Assign { lhs, .. } = hir[assignment].kind {
                                assigned(hir, lhs, &mut names);
                            }
                        }
                    }
                    Member::Declare(symbol) => {
                        if let SymbolKind::Instance(instance) = &hir[*symbol].kind {
                            read(hir, &instance.connections, &mut names);
                        }
                    }
                    _ => {}
                }
                for first in names {
                    let ExprKind::Name(name) = &hir[first].kind else {
                        continue;
                    };
                    if matches!(self.lookup(scope, name), Resolution::Undeclared(_)) {
                        let nets = self.implicit.entry(scope).or_default();
                        nets.entry(name.clone()).or_insert(first);
                    }
                }
            }
        }
    }

    /// Whether the definition around `scope` lets a net be declared
    /// implicitly.
    fn allows_implicit(&self, scope: ScopeId) -> bool {
        let hir = self.hir;
        let definition = hir.enclosing(scope, ScopeKind::Definition);
        match definition
            .and_then(|it| hir[it].owner)
            .map(|it| &hir[it].kind)
        {
            Some(SymbolKind::Definition { implicit_nets, .. }) => *implicit_nets,
            _ => true,
        }
    }

    // ------------------------------------------------------------------ walk

    fn scope(&mut self, scope: ScopeId) {
        let hir = self.hir;
        for (at, member) in hir[scope].members.iter().enumerate() {
            self.reached[scope.index()] = Some(at as u32);
            if self.defined_outside(scope, member) {
                // It stands below the class, but where is not known, so
                // around the class everything is declared.
                let outer = std::mem::replace(&mut self.reached, vec![None; hir.scopes.len()]);
                self.reached[scope.index()] = Some(at as u32);
                self.member(scope, member);
                self.reached = outer;
            } else {
                self.member(scope, member);
            }
        }
        self.reached[scope.index()] = None;
    }

    fn member(&mut self, scope: ScopeId, member: &Member) {
        match member {
            Member::Declare(symbol) => self.symbol(scope, *symbol),
            // Whether the package exists is `check`'s to say.
            Member::Import(_) => {}
            Member::Assign(exprs) | Member::Defparam(exprs) | Member::Alias(exprs) => {
                for &expr in exprs {
                    self.expr(scope, expr);
                }
            }
            Member::Process(process) => self.stmt(scope, process.body),
            Member::GenerateIf(arms) | Member::GenerateCase { arms, .. } => {
                if let Member::GenerateCase { selector, .. } = member {
                    self.expr(scope, *selector);
                }
                for arm in arms {
                    for &condition in &arm.conditions {
                        self.expr(scope, condition);
                    }
                    self.scope(arm.body);
                }
            }
            Member::GenerateFor { header, body } => {
                self.header(header);
                self.scope(*body);
            }
            Member::Generate(body) => self.scope(*body),
            Member::Bind(id) => {
                let bind = &self.hir[*id];
                for &path in &bind.paths {
                    self.expr_at(scope, path, Place::Head);
                }
                let design = self.design;
                let definition = match design.target(self.file, *id) {
                    BindTarget::Definition(it)
                    | BindTarget::Instances {
                        definition: Some(it),
                    } => Some(it),
                    // An instance a path names is bound into its definition.
                    BindTarget::Instances { definition: None } => match bind.paths.as_slice() {
                        &[path] => match self.names.expr(path) {
                            Some(Resolution::Declared(at)) => match &design.symbol(at).kind {
                                SymbolKind::Instance(instance) => {
                                    design.definition(&instance.definition.text)
                                }
                                _ => None,
                            },
                            _ => None,
                        },
                        _ => None,
                    },
                    BindTarget::Undeclared => None,
                };
                self.within = match definition.map(|at| (at, &design.symbol(at).kind)) {
                    Some((at, SymbolKind::Definition { scope, .. })) => {
                        Within::Target(at.file, *scope)
                    }
                    _ => Within::Unknown,
                };
                for &instance in &bind.instances {
                    self.symbol(scope, instance);
                }
                self.within = Within::Written;
            }
            Member::DefaultDisable(expr)
            | Member::DefaultClocking(expr)
            | Member::ElaborationTask(expr) => self.expr(scope, *expr),
            Member::Opaque(opaque) => self.opaque(scope, opaque),
        }
    }

    fn symbol(&mut self, scope: ScopeId, symbol: SymbolId) {
        let hir = self.hir;
        match &hir[symbol].kind {
            SymbolKind::Definition { scope: body, .. } => self.scope(*body),
            SymbolKind::Class(class) => {
                if let Some((base, args)) = &class.extends {
                    self.ty(class.scope, base);
                    self.args(class.scope, args);
                }
                for ty in &class.implements {
                    self.ty(class.scope, ty);
                }
                self.scope(class.scope);
            }
            SymbolKind::Constraint(body) => self.stmts(scope, [*body]),
            SymbolKind::Other(body) => self.opaque(scope, body),
            // A port of a non-ANSI list is completed by a declaration in the
            // body, below what the body declares before it.
            SymbolKind::Port(port) => {
                let at = self.reached[scope.index()].take();
                self.ty(scope, &port.ty);
                self.value(scope, port.default);
                self.reached[scope.index()] = at;
            }
            SymbolKind::Parameter(parameter) => {
                self.ty(scope, &parameter.ty);
                self.value(scope, parameter.value);
            }
            SymbolKind::Net(data) | SymbolKind::Variable(data) => {
                self.ty(scope, &data.ty);
                if let Some(delay) = &data.delay {
                    self.timing(scope, delay);
                }
                self.value(scope, data.init);
            }
            SymbolKind::Genvar(init) => self.value(scope, *init),
            SymbolKind::Typedef(ty) => self.ty(scope, ty),
            SymbolKind::Forward => {}
            SymbolKind::EnumMember { value, .. } => self.value(scope, *value),
            SymbolKind::Subroutine {
                scope: body,
                returns,
                body: stmts,
                ..
            } => {
                if let Some(returns) = returns {
                    self.ty(scope, returns);
                }
                self.scope(*body);
                for &stmt in stmts {
                    self.stmt(*body, stmt);
                }
            }
            SymbolKind::Property {
                scope: body, spec, ..
            } => {
                self.scope(*body);
                self.spec(*body, spec);
            }
            SymbolKind::Let {
                scope: body, expr, ..
            } => {
                self.scope(*body);
                self.expr(*body, *expr);
            }
            SymbolKind::Modport(items) => {
                for item in items {
                    self.expr(scope, item.value);
                }
            }
            SymbolKind::Clocking(clocking) => {
                self.timing(scope, &clocking.event);
                for skew in &clocking.skews {
                    self.timing(scope, skew);
                }
                for signal in &clocking.signals {
                    self.expr(scope, signal.signal);
                }
                self.scope(clocking.scope);
            }
            SymbolKind::Covergroup(covergroup) => {
                self.scope(covergroup.scope);
                if let Some(event) = &covergroup.event {
                    self.timing(covergroup.scope, event);
                }
                for option in &covergroup.options {
                    self.expr(covergroup.scope, option.value);
                }
                self.scope(covergroup.points);
            }
            // `scope` declares cover points; what one samples is in the
            // covergroup's, around it.
            SymbolKind::Coverpoint(point) => {
                let sampled = match point.keyword {
                    COVERPOINT_KW => hir[scope].parent.unwrap_or(scope),
                    _ => scope,
                };
                if let Some(ty) = &point.ty {
                    self.ty(sampled, ty);
                }
                for &expr in &point.exprs {
                    self.expr(sampled, expr);
                }
                self.value(sampled, point.iff);
                for bins in &point.bins {
                    for &value in &bins.values {
                        self.expr(scope, value);
                    }
                    if let Some(with) = &bins.with {
                        self.with(with);
                    }
                    self.value(scope, bins.iff);
                }
                for option in &point.options {
                    self.expr(scope, option.value);
                }
            }
            SymbolKind::Instance(instance) => {
                self.args(scope, &instance.parameters);
                if let Some(delay) = &instance.delay {
                    self.timing(scope, delay);
                }
                self.dims(scope, &instance.dims);
                self.args(scope, &instance.connections);
                self.wildcard(scope, symbol, instance);
            }
            // Walked where the block or statement stands.
            SymbolKind::Block(_) | SymbolKind::Label(_) | SymbolKind::Iterator => {}
        }
    }

    fn header(&mut self, header: &For) {
        self.scope(header.scope);
        let exprs = header
            .init
            .iter()
            .chain(&header.condition)
            .chain(&header.step);
        for &expr in exprs {
            self.expr(header.scope, expr);
        }
    }

    fn stmt(&mut self, scope: ScopeId, stmt: StmtId) {
        let hir = self.hir;
        match &hir[stmt].kind {
            StmtKind::Block {
                scope: inner,
                stmts,
                ..
            } => {
                self.scope(*inner);
                for &stmt in stmts {
                    self.stmt(*inner, stmt);
                }
            }
            StmtKind::If {
                matched,
                condition,
                then_branch,
                else_branch,
            } => {
                let guarded = matched.unwrap_or(scope);
                self.expr(guarded, *condition);
                self.stmts(guarded, [*then_branch]);
                self.stmts(scope, [*else_branch]);
            }
            StmtKind::Case {
                selector, items, ..
            } => {
                self.value(scope, *selector);
                for item in items {
                    let guarded = item.matched.unwrap_or(scope);
                    for &label in &item.labels {
                        self.expr(guarded, label);
                    }
                    self.stmts(guarded, [item.body]);
                }
            }
            StmtKind::For { header, body } => {
                self.header(header);
                self.stmts(header.scope, [*body]);
            }
            StmtKind::Foreach {
                scope: inner,
                array,
                body,
            } => {
                self.expr(scope, *array);
                self.stmts(*inner, [*body]);
            }
            _ => hir.stmt_children(stmt, |child| self.child(scope, child)),
        }
    }

    fn stmts<const N: usize>(&mut self, scope: ScopeId, stmts: [Option<StmtId>; N]) {
        for stmt in stmts.into_iter().flatten() {
            self.stmt(scope, stmt);
        }
    }

    fn spec(&mut self, scope: ScopeId, spec: &PropertySpec) {
        if let Some(clock) = &spec.clock {
            self.timing(scope, clock);
        }
        self.value(scope, spec.disable);
        self.expr(scope, spec.property);
    }

    fn timing(&mut self, scope: ScopeId, timing: &Timing) {
        let events = match timing {
            Timing::Delay(_, values) => {
                for &value in values {
                    self.expr(scope, value);
                }
                return;
            }
            Timing::Star => return,
            Timing::Events(events) => events,
            Timing::Repeat(count, events) => {
                self.expr(scope, *count);
                events
            }
        };
        for event in events {
            self.expr(scope, event.expr);
            self.value(scope, event.iff);
        }
    }

    fn ty(&mut self, scope: ScopeId, ty: &Type) {
        match &ty.kind {
            TypeKind::Implicit { .. } | TypeKind::Builtin { .. } => {}
            // In a type, `a.b` is only ever an interface's modport, so its
            // head is not a name up the instance tree.
            TypeKind::Named(name) => match self.hir[*name].kind {
                ExprKind::Member { base, .. } => self.expr(scope, base),
                _ => self.expr(scope, *name),
            },
            TypeKind::TypeOf(name) => self.expr(scope, *name),
            // The value is walked where it is matched.
            TypeKind::Matched { .. } => {}
            // Its members are walked where they are declared.
            TypeKind::Enum(id) => self.ty(scope, &self.hir[*id].base),
            TypeKind::Struct { fields, .. } => {
                for field in fields {
                    self.ty(scope, &field.ty);
                    self.value(scope, field.default);
                }
            }
            TypeKind::Opaque(opaque) => self.opaque(scope, opaque),
        }
        self.dims(scope, &ty.dims);
    }

    fn dims(&mut self, scope: ScopeId, dims: &[Dim]) {
        for dim in dims {
            match dim {
                Dim::Range(hi, lo) => {
                    self.expr(scope, *hi);
                    self.expr(scope, *lo);
                }
                Dim::Single(size) => self.expr(scope, *size),
                Dim::Empty | Dim::Wildcard => {}
            }
        }
    }

    fn args(&mut self, scope: ScopeId, args: &[Arg]) {
        for arg in args {
            match arg {
                Arg::Positional(value) | Arg::Named { value, .. } => self.value(scope, *value),
                Arg::Implicit { value, .. } => self.expr(scope, *value),
                Arg::Wildcard(_) => {}
            }
        }
    }

    /// Resolves what a `.*` connects: the name of each port the instance
    /// leaves to it, where the instance stands.
    fn wildcard(&mut self, scope: ScopeId, symbol: SymbolId, instance: &Instance) {
        let Some(written) = (instance.connections.iter()).find_map(|arg| match arg {
            Arg::Wildcard(at) => Some(*at),
            _ => None,
        }) else {
            return;
        };
        let Some(at) = self.design.instantiated(SymbolRef {
            file: self.file,
            symbol,
        }) else {
            return;
        };
        let SymbolKind::Definition { ports, .. } = &self.design.symbol(at).kind else {
            return;
        };
        let hir = &self.design[at.file];
        // A port another connection takes, by its name, is not left to it.
        let taken: FxHashSet<&str> = (bind::formals(hir, ports, &instance.connections).iter())
            .flatten()
            .map(|&port| &*hir[port].name.text)
            .collect();
        let mut connected = Vec::new();
        for &port in ports {
            let name = &hir[port].name.text;
            if !taken.contains(&**name) {
                let port = SymbolRef {
                    file: at.file,
                    symbol: port,
                };
                connected.push((port, self.lookup(scope, name)));
            }
        }
        self.names.wildcards.insert(written, connected);
    }

    fn opaque(&mut self, scope: ScopeId, opaque: &Opaque) {
        for &name in &opaque.names {
            self.expr(scope, name);
        }
    }

    fn with(&mut self, with: &With) {
        self.scope(with.scope);
        self.expr(with.scope, with.expr);
    }

    fn value(&mut self, scope: ScopeId, expr: Option<ExprId>) {
        if let Some(expr) = expr {
            self.expr(scope, expr);
        }
    }

    fn expr(&mut self, scope: ScopeId, expr: ExprId) {
        self.expr_at(scope, expr, Place::Value);
    }

    fn expr_at(&mut self, scope: ScopeId, expr: ExprId, place: Place) {
        let hir = self.hir;
        match &hir[expr].kind {
            ExprKind::Name(name) if place == Place::Value => {
                let (found, below) = self.find(scope, name, true);
                if let Some(symbol) = below {
                    self.names.early.push((hir[expr].span, symbol));
                }
                self.names.exprs[expr.index()] = Some(found);
            }
            ExprKind::Name(name) => {
                let found = match self.lookup(scope, name) {
                    Resolution::Undeclared(_) => Resolution::Unknown,
                    found => found,
                };
                self.names.exprs[expr.index()] = Some(found);
            }
            ExprKind::Conditional {
                matched,
                condition,
                then_value,
                else_value,
            } => {
                let guarded = matched.unwrap_or(scope);
                self.expr(guarded, *condition);
                self.expr(guarded, *then_value);
                self.expr(scope, *else_value);
            }
            // `a[i].b` is `a`'s member as much as `a.b` is.
            ExprKind::Select { base, index, range } => {
                self.expr_at(scope, *base, place);
                self.expr(scope, *index);
                self.value(scope, range.map(|(_, end)| end));
            }
            ExprKind::Member { base, name }
                if matches!(hir[*base].kind, ExprKind::Keyword(THIS_KW | SUPER_KW)) =>
            {
                let found = self.this(scope, *base, &name.text);
                let found = match found {
                    Resolution::Undeclared(_) if place != Place::Value => Resolution::Unknown,
                    found => found,
                };
                self.names.exprs[expr.index()] = Some(found);
            }
            ExprKind::Member { base, name } => {
                self.expr_at(scope, *base, Place::Head);
                match self.hierarchical(*base, &name.text) {
                    Some(found) => self.names.exprs[expr.index()] = Some(found),
                    None => self.names.members.push((expr, scope)),
                }
            }
            ExprKind::Scoped { base, name } => self.scoped(scope, expr, *base, name),
            ExprKind::Call { callee, args } => {
                self.expr_at(scope, *callee, Place::Callee);
                // `obj.randomize(x)` names a member of `obj`.
                let object = self.randomized(scope, expr, None);
                let outer = self.object;
                self.object = object.or(outer);
                self.args(scope, args);
                self.object = outer;
            }
            ExprKind::Pattern { ty, items } => {
                self.value(scope, *ty);
                for item in items {
                    if let Some(key) = item.key {
                        self.expr_at(scope, key, Place::Key);
                    }
                    self.expr(scope, item.value);
                }
            }
            ExprKind::With { base, with } => {
                self.expr(scope, *base);
                self.with(with);
            }
            ExprKind::Constrained {
                call,
                only,
                constraints,
            } => {
                self.expr(scope, *call);
                let object = self.randomized(scope, *call, only.as_deref());
                let outer = std::mem::replace(&mut self.object, object);
                self.stmt(scope, *constraints);
                self.object = outer;
            }
            ExprKind::Type(ty) => self.ty(scope, ty),
            ExprKind::Loose(name) => {
                let found = match self.lookup(scope, name) {
                    Resolution::Undeclared(_) => Resolution::Unknown,
                    found => found,
                };
                self.names.exprs[expr.index()] = Some(found);
                self.names.loose.push(expr);
            }
            _ => hir.expr_children(expr, |child| self.child(scope, child)),
        }
    }

    fn child(&mut self, scope: ScopeId, child: Node) {
        match child {
            Node::Expr(expr) => self.expr(scope, expr),
            Node::Stmt(stmt) => self.stmt(scope, stmt),
        }
    }

    /// The object `call`, standing in `scope`, randomizes, `obj.randomize()`,
    /// or `randomize()` in a class's method, with `only` the names that may
    /// be its members: `None` for a call of anything else.
    fn randomized(
        &self,
        scope: ScopeId,
        call: ExprId,
        only: Option<&'d [Name]>,
    ) -> Option<Object<'d>> {
        let hir = self.hir;
        let method = match hir[call].kind {
            ExprKind::Call { callee, .. } => callee,
            _ => call,
        };
        let this = || (hir.enclosing(scope, ScopeKind::Class)).and_then(|it| self.class(it));
        let object = |class| {
            Some(Object {
                call: scope,
                class,
                only,
            })
        };
        let base = match &hir[method].kind {
            ExprKind::Member { base, name } if &*name.text == "randomize" => *base,
            // A class's own `randomize`, ahead of `std`'s.
            ExprKind::Name(name) if &**name == "randomize" => {
                return this().and_then(|class| object(Some(class)));
            }
            _ => return None,
        };
        let (head, selects) = selected(hir, base);
        if let ExprKind::Keyword(THIS_KW) = hir[head].kind
            && let Some(class) = this()
        {
            return object(Some(class));
        }
        object(match self.names.expr(head) {
            Some(Resolution::Declared(at)) => self.design.class_of(at, selects),
            _ => None,
        })
    }

    /// `base.name` where `base` is a scope of the hierarchy: an instance,
    /// a generate block, a definition named from the top, an interface
    /// port or a virtual interface. Its member resolves there, or is
    /// unknown where it is not found, since generate blocks the HIR does
    /// not name may hold it. A class handle's member is one its class
    /// declares or inherits. `None` for anything else, such as a struct's
    /// member.
    fn hierarchical(&self, base: ExprId, name: &str) -> Option<Resolution> {
        let hir = self.hir;
        // `u[2].name` is a member of an element of `u`.
        let (head, selects) = selected(hir, base);
        let Some(Resolution::Declared(at)) = self.names.expr(head) else {
            return match self.names.expr(head) {
                Some(Resolution::Unknown) => Some(Resolution::Unknown),
                _ => None,
            };
        };
        let design = self.design;
        let scope = |found: SymbolRef| {
            if design.is_declared_twice(&design.symbol(found).name.text) {
                return None;
            }
            match &design.symbol(found).kind {
                SymbolKind::Definition { keyword, scope, .. } if *keyword != PACKAGE_KW => {
                    Some((found.file, *scope))
                }
                _ => None,
            }
        };
        // A handle's class declares or inherits its members.
        if let Some(class) = design.class_of(at, selects) {
            return Some(self.class_member(class, name));
        }
        let within = match &design.symbol(at).kind {
            SymbolKind::Instance(_) => design.instantiated(at).and_then(scope),
            SymbolKind::Block(block) => Some((at.file, *block)),
            SymbolKind::Definition { keyword, scope, .. } if *keyword != PACKAGE_KW => {
                Some((at.file, *scope))
            }
            // A clocking block's signal is what it stands for, which this
            // file has resolved if the block stands before the use. One it
            // names and nothing declares is reported where it names it.
            SymbolKind::Clocking(clocking) if at.file == self.file => {
                let signal = (clocking.signals.iter()).find(|it| &*it.name.text == name);
                let found = match signal {
                    Some(signal) => self.names.expr(signal.signal),
                    None => (design.declared(at.file, clocking.scope, name)).map(|symbol| {
                        Resolution::Declared(SymbolRef {
                            file: at.file,
                            symbol,
                        })
                    }),
                };
                return Some(match found {
                    Some(found @ Resolution::Declared(_)) => found,
                    _ => Resolution::Unknown,
                });
            }
            // A port, net or variable whose type names an interface, or a
            // modport of one, as an interface port or a virtual interface
            // does; or a covergroup, whose cover points are its members. A
            // struct's member is no name, through a typedef or a type
            // parameter, whose value only a body gives; one of a type sema
            // cannot see may be anything.
            SymbolKind::Port(Port { ty, .. })
            | SymbolKind::Variable(Data { ty, .. })
            | SymbolKind::Net(Data { ty, .. }) => {
                let TypeKind::Named(named) = ty.kind else {
                    return None;
                };
                let hir = &design[at.file];
                let named = match hir[named].kind {
                    ExprKind::Member { base, .. } => base,
                    _ => named,
                };
                let found = match &hir[named].kind {
                    ExprKind::Name(_) | ExprKind::Scoped { .. } if at.file == self.file => {
                        self.names.expr(named)
                    }
                    ExprKind::Name(name) => design.definition(name).map(Resolution::Declared),
                    _ => None,
                };
                let Some(Resolution::Declared(found)) = found else {
                    return Some(Resolution::Unknown);
                };
                let at = found;
                let file = found.file;
                let found = design.symbol(found);
                match &found.kind {
                    SymbolKind::Definition {
                        keyword: INTERFACE_KW,
                        ..
                    } => scope(at),
                    SymbolKind::Covergroup(covergroup) => Some((file, covergroup.points)),
                    SymbolKind::Typedef(_)
                    | SymbolKind::Parameter(Parameter { is_type: true, .. }) => return None,
                    _ => None,
                }
            }
            _ => return None,
        };
        let Some((file, scope)) = within else {
            return Some(Resolution::Unknown);
        };
        Some(match design.declared(file, scope, name) {
            Some(symbol) => Resolution::Declared(SymbolRef { file, symbol }),
            None => Resolution::Unknown,
        })
    }

    /// `base::name`: a package's member, a class's, or `$unit`'s.
    fn scoped(&mut self, scope: ScopeId, expr: ExprId, base: ExprId, name: &Name) {
        let hir = self.hir;
        let found = match &hir[base].kind {
            ExprKind::System(unit) if &**unit == "$unit" => {
                let root = hir.root();
                match self.design.declared(self.file, root, &name.text) {
                    Some(symbol) => self.here(symbol),
                    None if hir[root].opaque => Resolution::Unknown,
                    None => Resolution::Undeclared(Searched::Package),
                }
            }
            // In an object's `randomize() with {…}`, a name around the call
            // rather than the object's member.
            ExprKind::Keyword(LOCAL_KW) => {
                let object = self.object.take();
                let found = self.lookup(scope, &name.text);
                self.object = object;
                found
            }
            ExprKind::Name(package) => {
                // A class in scope comes before a package, and what else
                // a name in scope may be has no members modelled.
                let found = self.lookup(scope, package);
                let class = match found {
                    Resolution::Declared(at) => {
                        !matches!(self.design.symbol(at).kind, SymbolKind::Definition { .. })
                    }
                    _ => false,
                };
                let named = match found {
                    Resolution::Declared(at) => self.design.class_at(at),
                    _ => None,
                };
                match (self.design.package(package), named) {
                    (_, Some(at)) => {
                        self.names.exprs[base.index()] = Some(found);
                        self.class_member(at, &name.text)
                    }
                    (Some(at), _) if !class => {
                        self.names.exprs[base.index()] = Some(Resolution::Declared(at));
                        self.design.member_of(package, &name.text, 0)
                    }
                    (None, _) if !class && &**package == "std" => {
                        self.names.exprs[base.index()] = Some(Resolution::Unknown);
                        self.design.member_of(package, &name.text, 0)
                    }
                    _ => {
                        self.expr(scope, base);
                        Resolution::Unknown
                    }
                }
            }
            // `C #(8)::name`, or `C::type_id::name`.
            _ => {
                self.expr(scope, base);
                let head = match hir[base].kind {
                    ExprKind::Call { callee, .. } => callee,
                    _ => base,
                };
                match self.names.expr(head) {
                    Some(Resolution::Declared(at)) => match self.design.class_at(at) {
                        Some(class) => self.class_member(class, &name.text),
                        None => Resolution::Unknown,
                    },
                    _ => Resolution::Unknown,
                }
            }
        };
        self.names.exprs[expr.index()] = Some(found);
    }

    /// `C::name`: a member of the class `class`, or one every class has.
    fn class_member(&self, class: SymbolRef, name: &str) -> Resolution {
        match self.design.class_member(class, name) {
            Resolution::Undeclared(_) if CLASS_METHODS.contains(&name) => Resolution::Unknown,
            found => found,
        }
    }
}

/// `expr` past the selects of its elements, and how many there are; `None`
/// if one is a range, which leaves an array.
pub(crate) fn selected(hir: &Hir, expr: ExprId) -> (ExprId, Option<usize>) {
    let (mut head, mut selects) = (expr, Some(0));
    while let ExprKind::Select { base, range, .. } = hir[head].kind {
        selects = selects.filter(|_| range.is_none()).map(|it| it + 1);
        head = base;
    }
    (head, selects)
}

/// The names a continuous assignment's target is made of: itself, or each
/// element of a concatenation, past its selects. Any of them may be a net
/// that use declares, as in `assign {carry, sum[3:0]} = a + b`.
fn assigned(hir: &Hir, expr: ExprId, names: &mut Vec<ExprId>) {
    let (head, _) = selected(hir, expr);
    match &hir[head].kind {
        ExprKind::Concat(elements) => {
            for &element in elements {
                assigned(hir, element, names);
            }
        }
        ExprKind::Replication { concat, .. } => assigned(hir, *concat, names),
        _ => names.push(head),
    }
}

/// The names the connections of an instance read, in the order written.
/// Any of them may be a net that use declares, wherever it stands in its
/// connection, as in `.q({unused, q[4:0]})` or `.push(valid & ~full)`. The
/// base of a member is a hierarchical name, and a callee or a type names no
/// net.
fn read(hir: &Hir, args: &[Arg], names: &mut Vec<ExprId>) {
    for arg in args {
        if let Arg::Positional(Some(value))
        | Arg::Named {
            value: Some(value), ..
        } = arg
        {
            reads(hir, *value, names);
        }
    }
}

fn reads(hir: &Hir, expr: ExprId, names: &mut Vec<ExprId>) {
    let each = |exprs: &[ExprId], names: &mut Vec<ExprId>| {
        for &expr in exprs {
            reads(hir, expr, names);
        }
    };
    match &hir[expr].kind {
        ExprKind::Name(_) => names.push(expr),
        ExprKind::Unary { operand, .. }
        | ExprKind::Postfix { operand, .. }
        | ExprKind::Cast { operand, .. } => reads(hir, *operand, names),
        ExprKind::Binary { lhs, rhs, .. } => each(&[*lhs, *rhs], names),
        ExprKind::Conditional {
            condition,
            then_value,
            else_value,
            ..
        } => each(&[*condition, *then_value, *else_value], names),
        ExprKind::Select { base, index, range } => {
            each(&[*base, *index], names);
            if let Some((_, end)) = range {
                reads(hir, *end, names);
            }
        }
        ExprKind::Concat(parts) => each(parts, names),
        ExprKind::Replication { count, concat } => each(&[*count, *concat], names),
        ExprKind::Inside { expr, set } => {
            reads(hir, *expr, names);
            each(set, names);
        }
        ExprKind::Call { args, .. } => read(hir, args, names),
        _ => {}
    }
}
