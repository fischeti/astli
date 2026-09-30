//! Name resolution: what each name a file uses refers to.
//!
//! A name is looked for in its scope and then each around it, up to the
//! file's: first what the scope declares, then the nets it declares
//! implicitly, then what it imports by name, then what it imports by
//! wildcard. Past the file, a name may be a module, interface or program of
//! the design. `pkg::name` is looked for in the package alone, and what the
//! package exports.
//!
//! Only the head of `a.b.c` is resolved: what `b` is depends on `a`'s type,
//! or on the instance tree, which elaboration builds.

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
}

impl Names {
    /// What `expr` refers to, if it is a name or a `pkg::name`; `None` for
    /// any other expression, and for a member `a.b`, which needs types.
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
}

/// Names from the `std` package, which every scope imports.
const STD: &[&str] = &[
    "process",
    "semaphore",
    "mailbox",
    "randomize",
    "weak_reference",
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
        Resolver {
            design,
            file,
            hir,
            opaque,
            implicit: FxHashMap::default(),
            names: Names {
                exprs: vec![None; hir.exprs.len()],
                loose: Vec::new(),
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

    /// What `name`, used in `scope`, refers to.
    fn lookup(&self, scope: ScopeId, name: &str) -> Resolution {
        let mut unknown = false;
        let mut at = Some(scope);
        while let Some(scope) = at {
            if let Some(symbol) = self.design.declared(self.file, scope, name) {
                return self.here(symbol);
            }
            if let Some(&first) = self.implicit.get(&scope).and_then(|nets| nets.get(name)) {
                return Resolution::Implicit(first);
            }
            match self.design.imported(self.file, scope, name, 0) {
                Resolution::Undeclared => {}
                Resolution::Unknown => unknown = true,
                found => return found,
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

    /// Declares the nets names declare by their first use: an undeclared
    /// name connected to a port, or assigned continuously, where the
    /// definition allows it.
    fn implicit_nets(&mut self) {
        let hir = self.hir;
        for (scope, body) in hir.scopes() {
            if !self.allows_implicit(scope) {
                continue;
            }
            for member in &body.members {
                let firsts: Vec<ExprId> = match member {
                    Member::Assign(assignments) => (assignments.iter())
                        .filter_map(|&it| match hir[it].kind {
                            ExprKind::Assign { lhs, .. } => Some(lhs),
                            _ => None,
                        })
                        .collect(),
                    Member::Declare(symbol) => match &hir[*symbol].kind {
                        SymbolKind::Instance(instance) => (instance.connections.iter())
                            .filter_map(|arg| match arg {
                                Arg::Positional(value) | Arg::Named { value, .. } => *value,
                                _ => None,
                            })
                            .collect(),
                        _ => Vec::new(),
                    },
                    _ => Vec::new(),
                };
                for first in firsts {
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
        for member in &hir[scope].members {
            self.member(scope, member);
        }
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
            Member::Opaque(opaque) => self.opaque(scope, opaque),
        }
    }

    fn symbol(&mut self, scope: ScopeId, symbol: SymbolId) {
        let hir = self.hir;
        match &hir[symbol].kind {
            SymbolKind::Definition { scope: body, .. } => self.scope(*body),
            SymbolKind::Class(body) | SymbolKind::Other(body) => self.opaque(scope, body),
            SymbolKind::Port(port) => {
                self.ty(scope, &port.ty);
                self.value(scope, port.default);
            }
            SymbolKind::Parameter(parameter) => {
                self.ty(scope, &parameter.ty);
                self.value(scope, parameter.value);
            }
            SymbolKind::Net(data) | SymbolKind::Variable(data) => {
                self.ty(scope, &data.ty);
                self.value(scope, data.init);
            }
            SymbolKind::Genvar(init) => self.value(scope, *init),
            SymbolKind::Typedef(ty) => self.ty(scope, ty),
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
            SymbolKind::Instance(instance) => {
                self.args(scope, &instance.parameters);
                self.dims(scope, &instance.dims);
                self.args(scope, &instance.connections);
            }
            // Walked where the block or statement stands.
            SymbolKind::Block(_) | SymbolKind::Label(_) => {}
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
            StmtKind::Opaque(opaque) => self.opaque(scope, opaque),
        }
    }

    fn stmts<const N: usize>(&mut self, scope: ScopeId, stmts: [Option<StmtId>; N]) {
        for stmt in stmts.into_iter().flatten() {
            self.stmt(scope, stmt);
        }
    }

    fn timing(&mut self, scope: ScopeId, timing: &Timing) {
        let events = match timing {
            Timing::Delay(_, value) => return self.expr(scope, *value),
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
                    let found = self.lookup(scope, &name.text);
                    self.names.loose.push((name.clone(), found));
                }
                Arg::Wildcard(_) => {}
            }
        }
    }

    fn opaque(&mut self, scope: ScopeId, opaque: &Opaque) {
        for name in &opaque.names {
            let found = match self.lookup(scope, &name.text) {
                Resolution::Undeclared => Resolution::Unknown,
                found => found,
            };
            self.names.loose.push((name.clone(), found));
        }
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
            ExprKind::Name(name) => {
                let found = match self.lookup(scope, name) {
                    Resolution::Undeclared if place != Place::Value => Resolution::Unknown,
                    found => found,
                };
                self.names.exprs[expr.index()] = Some(found);
            }
            ExprKind::System(_) | ExprKind::Keyword(_) | ExprKind::Literal(_) => {}
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
            ExprKind::Member { base, .. } => self.expr_at(scope, *base, Place::Head),
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
            ExprKind::Tagged { value, .. } => self.value(scope, *value),
            ExprKind::Type(ty) => self.ty(scope, ty),
            ExprKind::Opaque(opaque) => self.opaque(scope, opaque),
        }
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
            ExprKind::Name(package) => {
                // A class in scope comes before a package, and its members
                // are not modelled.
                let class = match self.lookup(scope, package) {
                    Resolution::Declared(at) => {
                        !matches!(self.design.symbol(at).kind, SymbolKind::Definition { .. })
                    }
                    _ => false,
                };
                match self.design.package(package) {
                    Some(at) if !class => {
                        self.names.exprs[base.index()] = Some(Resolution::Declared(at));
                        self.design.member_of(package, &name.text, 0)
                    }
                    _ => {
                        self.expr(scope, base);
                        Resolution::Unknown
                    }
                }
            }
            _ => {
                self.expr(scope, base);
                Resolution::Unknown
            }
        };
        self.names.exprs[expr.index()] = Some(found);
    }
}
