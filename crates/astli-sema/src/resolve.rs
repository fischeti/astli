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
use rustc_hash::FxHashMap;

use crate::design::{Design, FileId, SymbolRef};
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
    /// Declared nowhere it could be.
    Undeclared,
}

/// What the names one file uses refer to.
#[derive(Debug, Clone)]
pub struct Names {
    exprs: Vec<Option<Resolution>>,
    loose: Vec<(Name, Resolution)>,
    /// Where each loose name is written, and its place in `loose`.
    at: FxHashMap<Span, usize>,
    /// What each port of a `.*` connects, by instance: each port the
    /// instance does not connect by name, and what its name means where the
    /// instance stands.
    wildcards: FxHashMap<SymbolId, Vec<(SymbolRef, Resolution)>>,
    /// Where each name used above its declaration is written, and the
    /// declaration.
    early: Vec<(Span, SymbolId)>,
}

impl Names {
    /// What `expr` refers to, if it is a name, a `pkg::name`, or a member
    /// `a.b` of an instance, a generate block or an interface port; `None`
    /// for any other expression, and for a struct's member.
    pub fn expr(&self, expr: ExprId) -> Option<Resolution> {
        self.exprs.get(expr.index()).copied().flatten()
    }

    /// Names with no expression of their own, and what each refers to: an
    /// implicit connection's `.name`, and the names opaque regions spell,
    /// which are never [`Resolution::Undeclared`] since what they are is not
    /// known.
    pub fn loose(&self) -> &[(Name, Resolution)] {
        &self.loose
    }

    /// What the loose name written at `at` refers to.
    pub fn loose_at(&self, at: Span) -> Option<Resolution> {
        self.at.get(&at).map(|&index| self.loose[index].1)
    }

    /// The names used above their declaration where nothing declares them
    /// before: where each is written, and the declaration below it.
    pub fn early(&self) -> &[(Span, SymbolId)] {
        &self.early
    }

    /// What the `.*` of `instance` connects to each port it leaves to it.
    pub fn wildcard(&self, instance: SymbolId) -> &[(SymbolRef, Resolution)] {
        self.wildcards.get(&instance).map_or(&[], Vec::as_slice)
    }

    fn push_loose(&mut self, name: &Name, found: Resolution) {
        self.at.insert(name.span, self.loose.len());
        self.loose.push((name.clone(), found));
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
const CLASS_METHODS: &[&str] = &[
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
    /// The scopes holding a region not lowered, which may declare anything.
    opaque: Vec<bool>,
    /// The nets each scope declares implicitly, by their first use.
    implicit: FxHashMap<ScopeId, FxHashMap<Box<str>, ExprId>>,
    /// While resolving a bound instance, where its names are looked up: the
    /// target definition's scope, or `None` if no file declares it.
    bound: Option<Option<(FileId, ScopeId)>>,
    /// While resolving an object's `randomize() with {…}`, the scope the
    /// call stands in, whose names come after the object's members, and
    /// the object's class, `None` where it is not known.
    object: Option<(ScopeId, Option<SymbolRef>)>,
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
        let mut opaque: Vec<bool> = (hir.scopes())
            .map(|(_, scope)| {
                scope
                    .members
                    .iter()
                    .any(|it| matches!(it, Member::Opaque(_)))
            })
            .collect();
        let is_opaque = |stmt: &StmtId| matches!(hir[*stmt].kind, StmtKind::Opaque(_));
        for (_, stmt) in hir.stmts() {
            if let StmtKind::Block { scope, stmts, .. } = &stmt.kind {
                opaque[scope.index()] |= stmts.iter().any(is_opaque);
            }
        }
        for (_, symbol) in hir.symbols() {
            if let SymbolKind::Subroutine { scope, body, .. } = &symbol.kind {
                opaque[scope.index()] |= body.iter().any(is_opaque);
            }
        }
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
            opaque,
            implicit: FxHashMap::default(),
            bound: None,
            object: None,
            since,
            reached: vec![None; hir.scopes.len()],
            names: Names {
                exprs: vec![None; hir.exprs.len()],
                loose: Vec::new(),
                at: FxHashMap::default(),
                wildcards: FxHashMap::default(),
                early: Vec::new(),
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
        if let Some(target) = self.bound {
            return (self.lookup_bound(target, name), None);
        }
        let mut below = None;
        let found = self.find_from(scope, name, ordered, &mut below);
        match (found, below) {
            (Resolution::Undeclared, Some(symbol)) => (self.here(symbol), Some(symbol)),
            // What sema cannot see may declare it above, but whether it
            // does is not known, and the declaration below is what the
            // name most likely means.
            (Resolution::Unknown, Some(symbol)) => (self.here(symbol), None),
            (found, _) => (found, None),
        }
    }

    /// What `name` refers to in `scope` or around it, recording in `below`
    /// the innermost declaration it skipped for standing below the use.
    fn find_from(
        &self,
        scope: ScopeId,
        name: &str,
        ordered: bool,
        below: &mut Option<SymbolId>,
    ) -> Resolution {
        let mut unknown = false;
        let mut at = Some(scope);
        while let Some(scope) = at {
            if let Some((call, class)) = self.object
                && call == scope
            {
                match class.map(|class| self.class_member(class, name)) {
                    Some(Resolution::Undeclared) => {}
                    Some(found @ Resolution::Declared(_)) => return found,
                    _ => unknown = true,
                }
            }
            if let Some(symbol) = self.design.declared(self.file, scope, name) {
                if !ordered || self.declared_yet(scope, symbol) {
                    return self.here(symbol);
                }
                below.get_or_insert(symbol);
            }
            if let Some(&first) = self.implicit.get(&scope).and_then(|nets| nets.get(name)) {
                return Resolution::Implicit(first);
            }
            match self.design.imported(self.file, scope, name, 0) {
                Resolution::Undeclared => {}
                Resolution::Unknown => unknown = true,
                found => return found,
            }
            if let Some(class) = self.class(scope) {
                match self.design.inherited(class, name) {
                    Resolution::Undeclared => {}
                    Resolution::Unknown => unknown = true,
                    found => return found,
                }
            }
            unknown |= self.opaque[scope.index()];
            at = self.hir[scope].parent;
        }
        if let Some(definition) = self.design.definition(name) {
            return Resolution::Declared(definition);
        }
        match unknown || STD.contains(&name) || self.design.spelled_opaque(name) {
            true => Resolution::Unknown,
            false => Resolution::Undeclared,
        }
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
        let mut at = Some(scope);
        let class = loop {
            let Some(scope) = at else {
                return Resolution::Unknown;
            };
            if let Some(class) = self.class(scope) {
                break class;
            }
            at = self.hir[scope].parent;
        };
        let found = match self.hir[base].kind {
            ExprKind::Keyword(SUPER_KW) => self.design.inherited(class, name),
            _ => self.design.class_member(class, name),
        };
        match found {
            Resolution::Undeclared if CLASS_METHODS.contains(&name) => Resolution::Unknown,
            found => found,
        }
    }

    /// What `name`, used by an instance bound into `target`, refers to:
    /// what the target declares or imports, or a design element. What it
    /// does not may be a net the target declares implicitly, or be in a
    /// region it leaves opaque, so it is unknown rather than undeclared.
    fn lookup_bound(&self, target: Option<(FileId, ScopeId)>, name: &str) -> Resolution {
        let Some((file, scope)) = target else {
            return Resolution::Unknown;
        };
        let mut at = Some(scope);
        while let Some(scope) = at {
            if let Some(symbol) = self.design.declared(file, scope, name) {
                return Resolution::Declared(SymbolRef { file, symbol });
            }
            if let found @ Resolution::Declared(_) = self.design.imported(file, scope, name, 0) {
                return found;
            }
            at = self.design[file][scope].parent;
        }
        match self.design.definition(name) {
            Some(definition) => Resolution::Declared(definition),
            None => Resolution::Unknown,
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
                    if self.lookup(scope, name) == Resolution::Undeclared {
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
        let mut at = Some(scope);
        while let Some(scope) = at {
            let definition = self.hir[scope].owner.map(|owner| &self.hir[owner].kind);
            if let Some(SymbolKind::Definition { implicit_nets, .. }) = definition {
                return *implicit_nets;
            }
            at = self.hir[scope].parent;
        }
        true
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
            Member::Assign(assignments) => {
                for &assignment in assignments {
                    self.expr(scope, assignment);
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
            Member::Bind(bind) => {
                let design = self.design;
                let target = (bind.target.as_ref())
                    .and_then(|target| design.definition(&target.text))
                    .and_then(|at| match &design.symbol(at).kind {
                        SymbolKind::Definition { scope, .. } => Some((at.file, *scope)),
                        _ => None,
                    });
                self.bound = Some(target);
                for &instance in &bind.instances {
                    self.symbol(scope, instance);
                }
                self.bound = None;
            }
            Member::DefaultDisable(expr) | Member::DefaultClocking(expr) => self.expr(scope, *expr),
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
            SymbolKind::EnumMember { value } => self.value(scope, *value),
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
            StmtKind::Empty | StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Expr(expr) | StmtKind::Trigger(expr) => self.expr(scope, *expr),
            StmtKind::ProceduralAssign { expr, .. } => self.expr(scope, *expr),
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
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr(scope, *condition);
                self.stmts(scope, [*then_branch, *else_branch]);
            }
            StmtKind::Case {
                selector, items, ..
            } => {
                self.value(scope, *selector);
                for item in items {
                    for &label in &item.labels {
                        self.expr(scope, label);
                    }
                    self.stmts(scope, [item.body]);
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
            StmtKind::Loop {
                condition, body, ..
            }
            | StmtKind::Wait { condition, body } => {
                self.value(scope, *condition);
                self.stmts(scope, [*body]);
            }
            StmtKind::Timed { timing, body } => {
                self.timing(scope, timing);
                self.stmts(scope, [*body]);
            }
            StmtKind::Disable(target) | StmtKind::Return(target) => self.value(scope, *target),
            StmtKind::Assertion {
                condition,
                pass,
                fail,
                ..
            } => {
                self.expr(scope, *condition);
                self.stmts(scope, [*pass, *fail]);
            }
            StmtKind::ConcurrentAssertion {
                spec, pass, fail, ..
            } => {
                self.spec(scope, spec);
                self.stmts(scope, [*pass, *fail]);
            }
            StmtKind::Opaque(opaque) => self.opaque(scope, opaque),
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
            // Its members are walked where they are declared.
            TypeKind::Enum { base, .. } => self.ty(scope, base),
            TypeKind::Struct { fields, .. } => {
                for field in fields {
                    self.ty(scope, &field.ty);
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
                Dim::Empty => {}
            }
        }
    }

    fn args(&mut self, scope: ScopeId, args: &[Arg]) {
        for arg in args {
            match arg {
                Arg::Positional(value) | Arg::Named { value, .. } => self.value(scope, *value),
                Arg::Implicit(name) => {
                    let (found, below) = self.find(scope, &name.text, true);
                    if let Some(symbol) = below {
                        self.names.early.push((name.span, symbol));
                    }
                    self.names.push_loose(name, found);
                }
                Arg::Wildcard(_) => {}
            }
        }
    }

    /// Resolves what a `.*` connects: the name of each port the instance
    /// leaves to it, where the instance stands.
    fn wildcard(&mut self, scope: ScopeId, symbol: SymbolId, instance: &Instance) {
        if !instance
            .connections
            .iter()
            .any(|arg| matches!(arg, Arg::Wildcard(_)))
        {
            return;
        }
        let Some(at) = self.design.definition(&instance.definition.text) else {
            return;
        };
        let SymbolKind::Definition { ports, .. } = &self.design.symbol(at).kind else {
            return;
        };
        let named = |name: &str| {
            instance.connections.iter().any(|arg| match arg {
                Arg::Named { name: it, .. } | Arg::Implicit(it) => &*it.text == name,
                _ => false,
            })
        };
        let hir = &self.design[at.file];
        let mut connected = Vec::new();
        for &port in ports {
            let name = &hir[port].name.text;
            if !named(name) {
                let port = SymbolRef {
                    file: at.file,
                    symbol: port,
                };
                connected.push((port, self.lookup(scope, name)));
            }
        }
        self.names.wildcards.insert(symbol, connected);
    }

    fn opaque(&mut self, scope: ScopeId, opaque: &Opaque) {
        for name in &opaque.names {
            let found = match self.lookup(scope, &name.text) {
                Resolution::Undeclared => Resolution::Unknown,
                found => found,
            };
            self.names.push_loose(name, found);
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
                    Resolution::Undeclared => Resolution::Unknown,
                    found => found,
                };
                self.names.exprs[expr.index()] = Some(found);
            }
            ExprKind::System(_)
            | ExprKind::Keyword(_)
            | ExprKind::Literal(_)
            | ExprKind::Time { .. } => {}
            ExprKind::Unary { operand, .. } | ExprKind::Postfix { operand, .. } => {
                self.expr(scope, *operand);
            }
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(scope, *lhs);
                self.expr(scope, *rhs);
            }
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
            } => {
                for &part in [condition, then_value, else_value] {
                    self.expr(scope, part);
                }
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
                    Resolution::Undeclared if place != Place::Value => Resolution::Unknown,
                    found => found,
                };
                self.names.exprs[expr.index()] = Some(found);
            }
            ExprKind::Member { base, name } => {
                self.expr_at(scope, *base, Place::Head);
                if let Some(found) = self.hierarchical(*base, &name.text) {
                    self.names.exprs[expr.index()] = Some(found);
                }
            }
            ExprKind::Scoped { base, name } => self.scoped(scope, expr, *base, name),
            ExprKind::Call { callee, args } => {
                self.expr_at(scope, *callee, Place::Callee);
                self.args(scope, args);
            }
            ExprKind::Concat(parts) => {
                for &part in parts {
                    self.expr(scope, part);
                }
            }
            ExprKind::Replication { count, concat } => {
                self.expr(scope, *count);
                self.expr(scope, *concat);
            }
            ExprKind::Stream { slice, concat, .. } => {
                self.value(scope, *slice);
                self.expr(scope, *concat);
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
            ExprKind::Cast { ty, operand } => {
                self.expr(scope, *ty);
                self.expr(scope, *operand);
            }
            ExprKind::Range { lo, hi, .. } => {
                self.expr(scope, *lo);
                self.expr(scope, *hi);
            }
            ExprKind::Inside { expr: tested, set } => {
                self.expr(scope, *tested);
                for &value in set {
                    self.expr(scope, value);
                }
            }
            ExprKind::Assign {
                lhs, rhs, timing, ..
            } => {
                self.expr(scope, *lhs);
                self.expr(scope, *rhs);
                if let Some(timing) = timing {
                    self.timing(scope, timing);
                }
            }
            ExprKind::With { base, with } => {
                self.expr(scope, *base);
                self.with(with);
            }
            ExprKind::Constrained { call, constraints } => {
                self.expr(scope, *call);
                let object = self.object_of(scope, *call).map(|class| (scope, class));
                let outer = std::mem::replace(&mut self.object, object);
                self.stmt(scope, *constraints);
                self.object = outer;
            }
            ExprKind::Dist { expr: tested, set } => {
                self.expr(scope, *tested);
                for &value in set {
                    self.expr(scope, value);
                }
            }
            ExprKind::Binsof { bins, intersect } => {
                self.expr(scope, *bins);
                for &value in intersect {
                    self.expr(scope, value);
                }
            }
            ExprKind::Tagged { value, .. } => self.value(scope, *value),
            ExprKind::Type(ty) => self.ty(scope, ty),
            ExprKind::Property(property) => {
                if let Property::Clocked { clock, .. } = property {
                    self.timing(scope, clock);
                }
                for operand in property.operands() {
                    self.expr(scope, operand);
                }
            }
            ExprKind::Opaque(opaque) => self.opaque(scope, opaque),
        }
    }

    /// The class of the object `call` randomizes, `obj.randomize()`: `None`
    /// for a call of no object's method, `Some(None)` where the object's
    /// class is not known.
    fn object_of(&self, scope: ScopeId, call: ExprId) -> Option<Option<SymbolRef>> {
        let hir = self.hir;
        let method = match hir[call].kind {
            ExprKind::Call { callee, .. } => callee,
            _ => call,
        };
        let ExprKind::Member { base, .. } = hir[method].kind else {
            return None;
        };
        let (head, selects) = selected(hir, base);
        if let ExprKind::Keyword(THIS_KW) = hir[head].kind {
            let mut at = Some(scope);
            while let Some(scope) = at {
                if let Some(class) = self.class(scope) {
                    return Some(Some(class));
                }
                at = hir[scope].parent;
            }
        }
        Some(match self.names.expr(head) {
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
        let scope = |definition: &str| {
            if design.is_declared_twice(definition) {
                return None;
            }
            let found = design.definition(definition)?;
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
            SymbolKind::Instance(instance) => scope(&instance.definition.text),
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
            // struct's member is no name; one of a type sema cannot see may
            // be anything.
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
                let file = found.file;
                let found = design.symbol(found);
                match &found.kind {
                    SymbolKind::Definition {
                        keyword: INTERFACE_KW,
                        ..
                    } => scope(&found.name.text),
                    SymbolKind::Covergroup(covergroup) => Some((file, covergroup.points)),
                    SymbolKind::Typedef(_) => return None,
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
                    None if self.opaque[root.index()] => Resolution::Unknown,
                    None => Resolution::Undeclared,
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
            Resolution::Undeclared if CLASS_METHODS.contains(&name) => Resolution::Unknown,
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
