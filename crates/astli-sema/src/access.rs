//! Accesses: which symbols a file reads and writes, where, and what drives
//! each write.
//!
//! An access the analysis cannot be sure of is both a read and a write, and
//! not [`certain`](Access::certain): a name in a region not lowered, an
//! argument of a subroutine it does not know, a connection to a port
//! without a direction. A lint that asks whether something is read, or
//! written, then errs towards yes; one that asks what drives it counts
//! only what is certain.

use astli_syntax::SyntaxKind::*;
use astli_text::Span;

use crate::design::{Design, FileId, SymbolRef};
use crate::hir::*;
use crate::resolve::{Names, Resolution};

/// One use of a symbol's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Access {
    pub symbol: SymbolRef,
    /// Where the name is written.
    pub at: Span,
    pub read: bool,
    pub write: bool,
    /// Whether the access is to all of it, rather than a select or a member.
    pub whole: bool,
    /// Whether it is known to read or write as it says, rather than assumed
    /// to do both for want of knowing.
    pub certain: bool,
    /// What a write is part of.
    pub driver: Driver,
}

/// What drives a write: what it is part of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Driver {
    /// An `always`, `initial` or `final` block, by its body.
    Process(StmtId),
    /// A continuous assignment, by its assignment.
    Assign(ExprId),
    /// A port connection of an instance.
    Instance(SymbolId),
    /// A declaration's initialiser: continuous for a net, once for a
    /// variable.
    Init(SymbolId),
    /// The body of a function or task, which runs where it is called.
    Subroutine(SymbolId),
    /// Anything else: a type, a parameter, a generate condition, a region
    /// not lowered.
    Other,
}

/// Every access in `file`, in the order of the HIR's walk.
pub fn accesses(design: &Design, file: FileId, names: &Names) -> Vec<Access> {
    let mut walk = Walk {
        design,
        file,
        hir: &design[file],
        names,
        driver: Driver::Other,
        found: Vec::new(),
    };
    walk.scope(walk.hir.root());
    walk.found
}

/// How an expression is used: read, written, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mode {
    read: bool,
    write: bool,
    certain: bool,
}

const READ: Mode = Mode {
    read: true,
    write: false,
    certain: true,
};
const WRITE: Mode = Mode {
    read: false,
    write: true,
    certain: true,
};
const BOTH: Mode = Mode {
    read: true,
    write: true,
    certain: true,
};
/// Either, for all the analysis knows.
const GUESS: Mode = Mode {
    read: true,
    write: true,
    certain: false,
};

/// How an argument is used, given its place among the positional ones, or
/// its name.
type ByArg<'a> = dyn Fn((usize, Option<&str>)) -> Mode + 'a;

struct Walk<'d> {
    design: &'d Design,
    file: FileId,
    hir: &'d Hir,
    names: &'d Names,
    driver: Driver,
    found: Vec<Access>,
}

impl Walk<'_> {
    fn driven(&mut self, driver: Driver, walk: impl FnOnce(&mut Self)) {
        let outer = std::mem::replace(&mut self.driver, driver);
        walk(self);
        self.driver = outer;
    }

    fn access(&mut self, found: Option<Resolution>, at: Span, mode: Mode, whole: bool) {
        if let Some(Resolution::Declared(symbol)) = found {
            self.found.push(Access {
                symbol,
                at,
                read: mode.read,
                write: mode.write,
                whole,
                certain: mode.certain,
                driver: self.driver,
            });
        }
    }

    fn scope(&mut self, scope: ScopeId) {
        let hir = self.hir;
        for member in &hir[scope].members {
            self.member(member);
        }
    }

    fn member(&mut self, member: &Member) {
        match member {
            Member::Declare(symbol) => self.symbol(*symbol),
            Member::Import(_) => {}
            Member::Assign(assignments) => {
                for &assignment in assignments {
                    self.driven(Driver::Assign(assignment), |walk| {
                        walk.expr(assignment, READ)
                    });
                }
            }
            Member::Process(process) => {
                let body = process.body;
                self.driven(Driver::Process(body), |walk| walk.stmt(body));
            }
            Member::GenerateIf(arms) | Member::GenerateCase { arms, .. } => {
                if let Member::GenerateCase { selector, .. } = member {
                    self.expr(*selector, READ);
                }
                for arm in arms {
                    self.exprs(&arm.conditions, READ);
                    self.scope(arm.body);
                }
            }
            Member::GenerateFor { header, body } => {
                self.header(header);
                self.scope(*body);
            }
            Member::Generate(body) => self.scope(*body),
            Member::Opaque(opaque) => self.opaque(opaque),
        }
    }

    fn symbol(&mut self, id: SymbolId) {
        let hir = self.hir;
        let symbol = &hir[id];
        match &symbol.kind {
            SymbolKind::Definition { scope, .. } => self.scope(*scope),
            SymbolKind::Class(body) | SymbolKind::Other(body) => self.opaque(body),
            SymbolKind::Port(port) => {
                self.ty(&port.ty);
                self.init(id, port.default);
            }
            SymbolKind::Parameter(parameter) => {
                self.ty(&parameter.ty);
                self.value(parameter.value, READ);
            }
            SymbolKind::Net(data) | SymbolKind::Variable(data) => {
                self.ty(&data.ty);
                self.init(id, data.init);
            }
            SymbolKind::Genvar(init) => self.init(id, *init),
            SymbolKind::Typedef(ty) => self.ty(ty),
            SymbolKind::EnumMember { value } => self.value(*value, READ),
            SymbolKind::Subroutine {
                scope,
                returns,
                body,
                ..
            } => {
                if let Some(returns) = returns {
                    self.ty(returns);
                }
                let (scope, body) = (*scope, body);
                self.driven(Driver::Subroutine(id), |walk| {
                    walk.scope(scope);
                    for &stmt in body {
                        walk.stmt(stmt);
                    }
                });
            }
            SymbolKind::Instance(instance) => self.instance(id, instance),
            SymbolKind::Block(_) | SymbolKind::Label(_) => {}
        }
    }

    /// The declaration `id`'s initialiser, which writes it.
    fn init(&mut self, id: SymbolId, value: Option<ExprId>) {
        let Some(value) = value else {
            return;
        };
        self.driven(Driver::Init(id), |walk| {
            let at = walk.hir[id].name.span;
            let symbol = SymbolRef {
                file: walk.file,
                symbol: id,
            };
            walk.access(Some(Resolution::Declared(symbol)), at, WRITE, true);
            walk.expr(value, READ);
        });
    }

    fn instance(&mut self, id: SymbolId, instance: &Instance) {
        self.args(&instance.parameters, &|_| READ);
        self.dims(&instance.dims);
        let definition = self.design.definition(&instance.definition.text);
        let ports: Vec<(&str, Mode)> = match definition.map(|at| (at, &self.design.symbol(at).kind))
        {
            Some((at, SymbolKind::Definition { ports, .. })) => {
                let hir = &self.design[at.file];
                (ports.iter())
                    .map(|&port| (&*hir[port].name.text, direction(&hir[port].kind)))
                    .collect()
            }
            _ => Vec::new(),
        };
        let mode = |arg: (usize, Option<&str>)| match arg {
            (_, Some(name)) => ports
                .iter()
                .find(|(it, _)| *it == name)
                .map_or(GUESS, |it| it.1),
            (index, None) => ports.get(index).map_or(GUESS, |it| it.1),
        };
        self.driven(Driver::Instance(id), |walk| {
            walk.args(&instance.connections, &mode);
            let names = walk.names;
            for &(port, found) in names.wildcard(id) {
                let mode = direction(&walk.design.symbol(port).kind);
                walk.access(Some(found), instance.definition.span, mode, true);
            }
        });
    }

    /// `args`, each used as `mode` says, given its position and its name.
    fn args(&mut self, args: &[Arg], mode: &ByArg) {
        let mut position = 0;
        for arg in args {
            match arg {
                Arg::Positional(value) => {
                    self.value(*value, mode((position, None)));
                    position += 1;
                }
                Arg::Named { name, value } => self.value(*value, mode((0, Some(&name.text)))),
                Arg::Implicit(name) => {
                    let found = self.names.loose_at(name.span);
                    self.access(found, name.span, mode((0, Some(&name.text))), true);
                }
                Arg::Wildcard(_) => {}
            }
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
            self.expr(expr, READ);
        }
    }

    fn stmt(&mut self, stmt: StmtId) {
        let hir = self.hir;
        match &hir[stmt].kind {
            StmtKind::Empty | StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Expr(expr) => self.expr(*expr, READ),
            // `-> ev` triggers the event, which waiting on it reads.
            StmtKind::Trigger(event) => self.expr(*event, BOTH),
            // A procedural continuous assignment overrides what drives its
            // target rather than adding a driver; `release` and `deassign`
            // name what they stop overriding.
            StmtKind::ProceduralAssign { keyword, expr } => {
                let (expr, keyword) = (*expr, *keyword);
                self.driven(Driver::Other, |walk| match keyword {
                    RELEASE_KW | DEASSIGN_KW => walk.expr(expr, WRITE),
                    _ => walk.expr(expr, READ),
                });
            }
            StmtKind::Block { scope, stmts, .. } => {
                self.scope(*scope);
                self.stmts(stmts);
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr(*condition, READ);
                self.branches([*then_branch, *else_branch]);
            }
            StmtKind::Case {
                selector, items, ..
            } => {
                self.value(*selector, READ);
                for item in items {
                    self.exprs(&item.labels, READ);
                    self.branches([item.body]);
                }
            }
            StmtKind::For { header, body } => {
                self.header(header);
                self.branches([*body]);
            }
            StmtKind::Foreach { scope, array, body } => {
                self.expr(*array, READ);
                // The loop writes its variables.
                let hir = self.hir;
                for member in &hir[*scope].members {
                    if let Member::Declare(id) = member {
                        let symbol = SymbolRef {
                            file: self.file,
                            symbol: *id,
                        };
                        let found = Some(Resolution::Declared(symbol));
                        self.access(found, hir[*id].name.span, WRITE, true);
                    }
                }
                self.branches([*body]);
            }
            StmtKind::Loop {
                condition, body, ..
            }
            | StmtKind::Wait { condition, body } => {
                self.value(*condition, READ);
                self.branches([*body]);
            }
            StmtKind::Timed { timing, body } => {
                self.timing(timing);
                self.branches([*body]);
            }
            StmtKind::Disable(target) | StmtKind::Return(target) => self.value(*target, READ),
            StmtKind::Assertion {
                condition,
                pass,
                fail,
                ..
            } => {
                self.expr(*condition, READ);
                self.branches([*pass, *fail]);
            }
            StmtKind::Opaque(opaque) => self.opaque(opaque),
        }
    }

    fn stmts(&mut self, stmts: &[StmtId]) {
        for &stmt in stmts {
            self.stmt(stmt);
        }
    }

    fn branches<const N: usize>(&mut self, stmts: [Option<StmtId>; N]) {
        for stmt in stmts.into_iter().flatten() {
            self.stmt(stmt);
        }
    }

    fn timing(&mut self, timing: &Timing) {
        match timing {
            Timing::Delay(_, value) => self.expr(*value, READ),
            Timing::Star => {}
            Timing::Events(events) => self.events(events),
            Timing::Repeat(count, events) => {
                self.expr(*count, READ);
                self.events(events);
            }
        }
    }

    fn events(&mut self, events: &[Event]) {
        for event in events {
            self.expr(event.expr, READ);
            self.value(event.iff, READ);
        }
    }

    fn ty(&mut self, ty: &Type) {
        match &ty.kind {
            TypeKind::Implicit { .. } | TypeKind::Builtin { .. } => {}
            TypeKind::Named(name) | TypeKind::TypeOf(name) => self.expr(*name, READ),
            TypeKind::Enum { base, .. } => self.ty(base),
            TypeKind::Struct { fields, .. } => {
                for field in fields {
                    self.ty(&field.ty);
                }
            }
            TypeKind::Opaque(opaque) => self.opaque(opaque),
        }
        self.dims(&ty.dims);
    }

    fn dims(&mut self, dims: &[Dim]) {
        for dim in dims {
            match dim {
                Dim::Range(hi, lo) => self.exprs(&[*hi, *lo], READ),
                Dim::Single(size) => self.expr(*size, READ),
                Dim::Empty => {}
            }
        }
    }

    /// The names a region not lowered spells, each of which it may read or
    /// write.
    fn opaque(&mut self, opaque: &Opaque) {
        for name in &opaque.names {
            let found = self.names.loose_at(name.span);
            self.access(found, name.span, GUESS, false);
        }
    }

    fn value(&mut self, expr: Option<ExprId>, mode: Mode) {
        if let Some(expr) = expr {
            self.expr(expr, mode);
        }
    }

    fn exprs(&mut self, exprs: &[ExprId], mode: Mode) {
        for &expr in exprs {
            self.expr(expr, mode);
        }
    }

    fn expr(&mut self, expr: ExprId, mode: Mode) {
        self.part(expr, mode, true);
    }

    /// `expr`, used as `mode`; `whole` when it is all of what it names, not
    /// a select or member of it.
    fn part(&mut self, expr: ExprId, mode: Mode, whole: bool) {
        let hir = self.hir;
        let at = hir[expr].span;
        match &hir[expr].kind {
            ExprKind::Name(_) | ExprKind::Scoped { .. } => {
                self.access(self.names.expr(expr), at, mode, whole);
                if let ExprKind::Scoped { base, .. } = hir[expr].kind {
                    // The package or class before `::` is only named.
                    self.access(self.names.expr(base), hir[base].span, READ, true);
                }
            }
            ExprKind::System(_) | ExprKind::Keyword(_) | ExprKind::Literal(_) => {}
            ExprKind::Unary { op, operand } | ExprKind::Postfix { op, operand } => {
                let mode = match op {
                    PLUS_PLUS | MINUS_MINUS => BOTH,
                    _ => READ,
                };
                self.expr(*operand, mode);
            }
            ExprKind::Binary { lhs, rhs, .. } => self.exprs(&[*lhs, *rhs], READ),
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
            } => self.exprs(&[*condition, *then_value, *else_value], READ),
            ExprKind::Select { base, index, range } => {
                self.part(*base, mode, false);
                self.expr(*index, READ);
                self.value(range.map(|(_, end)| end), READ);
            }
            ExprKind::Member { base, .. } => self.part(*base, mode, false),
            // On the left of an assignment, each part is written.
            ExprKind::Concat(parts) => {
                for &part in parts {
                    self.part(part, mode, whole);
                }
            }
            ExprKind::Replication { count, concat } => self.exprs(&[*count, *concat], READ),
            ExprKind::Stream { slice, concat, .. } => {
                self.value(*slice, READ);
                self.part(*concat, mode, whole);
            }
            ExprKind::Pattern { ty, items } => {
                self.value(*ty, READ);
                for item in items {
                    self.value(item.key, READ);
                    self.part(item.value, mode, whole);
                }
            }
            ExprKind::Cast { ty, operand } => self.exprs(&[*ty, *operand], READ),
            ExprKind::Inside { expr: tested, set } => {
                self.expr(*tested, READ);
                self.exprs(set, READ);
            }
            ExprKind::Assign {
                op,
                lhs,
                rhs,
                timing,
            } => {
                let target = match op {
                    EQ | LT_EQ => WRITE,
                    _ => BOTH,
                };
                self.expr(*lhs, target);
                self.expr(*rhs, READ);
                if let Some(timing) = timing {
                    self.timing(timing);
                }
            }
            ExprKind::Call { callee, args } => self.call(*callee, args),
            ExprKind::Tagged { value, .. } => self.value(*value, READ),
            ExprKind::Type(ty) => self.ty(ty),
            ExprKind::Opaque(opaque) => self.opaque(opaque),
        }
    }

    /// A call: each argument as the port it is passed to says, or both read
    /// and written where that is not known.
    fn call(&mut self, callee: ExprId, args: &[Arg]) {
        let hir = self.hir;
        match &hir[callee].kind {
            ExprKind::System(name) => {
                let name = name.clone();
                self.args(args, &|(index, _)| match system_writes(&name, index) {
                    true => BOTH,
                    false => READ,
                });
            }
            // A method may change the object it is called on.
            ExprKind::Member { base, .. } => {
                self.part(*base, GUESS, false);
                self.args(args, &|_| READ);
            }
            _ => {
                self.expr(callee, READ);
                let ports = subroutine_ports(self.design, self.names, callee);
                let mode = |arg: (usize, Option<&str>)| {
                    let Some(ports) = &ports else {
                        return GUESS;
                    };
                    let port = match arg {
                        (_, Some(name)) => ports.iter().find(|(it, _)| *it == name),
                        (index, None) => ports.get(index),
                    };
                    port.map_or(GUESS, |it| it.1)
                };
                self.args(args, &mode);
            }
        }
    }
}

/// The ports of the subroutine `callee` names, with how each is used.
fn subroutine_ports<'d>(
    design: &'d Design,
    names: &Names,
    callee: ExprId,
) -> Option<Vec<(&'d str, Mode)>> {
    let Some(Resolution::Declared(at)) = names.expr(callee) else {
        return None;
    };
    let SymbolKind::Subroutine { ports, .. } = &design.symbol(at).kind else {
        return None;
    };
    let hir = &design[at.file];
    let ports = ports
        .iter()
        .map(|&port| (&*hir[port].name.text, direction(&hir[port].kind)));
    Some(ports.collect())
}

/// How a port is used from where it is connected or passed.
fn direction(kind: &SymbolKind) -> Mode {
    let SymbolKind::Port(port) = kind else {
        return GUESS;
    };
    match port.direction {
        Some(INPUT_KW) => READ,
        Some(OUTPUT_KW) => WRITE,
        Some(_) => BOTH,
        // An interface port, which may carry either way.
        None => GUESS,
    }
}

/// Whether the system task or function `name` may write its argument at
/// `index`.
fn system_writes(name: &str, index: usize) -> bool {
    match name {
        "$fscanf" | "$sscanf" => index >= 2,
        "$readmemb" | "$readmemh" | "$value$plusargs" => index == 1,
        "$fread" | "$fgets" | "$cast" | "$swrite" | "$sformat" | "$random" | "$urandom" => {
            index == 0
        }
        name => name.starts_with("$dist_") && index == 0,
    }
}
