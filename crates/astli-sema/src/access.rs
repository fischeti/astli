//! Accesses: which symbols a file reads and writes, where, and what drives
//! each write.
//!
//! An access the analysis cannot be sure of is not
//! [`certain`](Access::certain). One it knows nothing of is both a read and
//! a write: a name in a region not lowered, an argument of a subroutine it
//! does not know, a connection to a port without a direction. A signal a
//! modport or a clocking block lists is read or driven as its direction
//! says, but only by whoever uses it. A lint that asks whether something is
//! read, or written, then errs towards yes; one that asks what drives it
//! counts only what is certain.

use astli_syntax::SyntaxKind::{self, *};
use astli_text::Span;

use crate::bind;
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
    /// How it writes, if it does.
    pub write: Option<Write>,
    /// Whether the access is to all of it, rather than a select or a member.
    pub whole: bool,
    /// What it accesses, if not all of it: the outermost select or member
    /// of the name, whose bits [`Elaboration::reach`] gives. `None` where
    /// the part is not written down, as in a region not lowered.
    ///
    /// [`Elaboration::reach`]: crate::Elaboration::reach
    pub place: Option<ExprId>,
    /// The module, interface, program or package the access stands in.
    pub within: Option<SymbolId>,
    /// What `base.name` reaches the symbol through: an instance, a generate
    /// block, an interface port. `None` for a name alone.
    pub base: Option<ExprId>,
    /// Whether it is known to read or write as it says, rather than assumed
    /// to for want of knowing.
    pub certain: bool,
    /// What a write is part of.
    pub driver: Driver,
}

/// How an access writes what it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Write {
    /// The target of an assignment, `=`, `<=` or `+=`, or the operand of
    /// `++` or `--`.
    Assign,
    /// An argument a subroutine's output or inout, or a system task or
    /// function, writes; one whose formal is not known; the object a
    /// method is called on.
    Argument,
    /// A connection to an instance's port, or a gate's terminal, that
    /// drives it; one whose port is not known.
    Connection,
    /// A declaration's initialiser; a `foreach`'s loop variables, which it
    /// gives each index.
    Init,
    /// A variable a pattern declares, which a match assigns.
    Match,
    /// What a member is written through, as `s` is in `s.f = 1` and `bus`
    /// in `bus.valid = 1`: part of it, or something it reaches.
    Through,
    /// Anything else: a signal a modport or a clocking block lists, which
    /// whoever uses it drives; `alias`, `->`, `release` or `deassign`; a
    /// region not lowered.
    Other,
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
        within: None,
        found: Vec::new(),
    };
    walk.scope(walk.hir.root());
    walk.found
}

/// How an expression is used: read, written, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mode {
    read: bool,
    write: Option<Write>,
    certain: bool,
}

const READ: Mode = Mode {
    read: true,
    write: None,
    certain: true,
};
/// Read by whatever uses a modport or a clocking block, if anything does.
const MAYBE_READ: Mode = Mode {
    read: true,
    write: None,
    certain: false,
};
/// Driven by whatever uses a modport or a clocking block, if anything does.
const MAYBE_WRITE: Mode = Mode {
    read: false,
    write: Some(Write::Other),
    certain: false,
};

const fn write(how: Write) -> Mode {
    Mode {
        read: false,
        write: Some(how),
        certain: true,
    }
}

const fn both(how: Write) -> Mode {
    Mode {
        read: true,
        write: Some(how),
        certain: true,
    }
}

/// Either, for all the analysis knows.
const fn guess(how: Write) -> Mode {
    Mode {
        read: true,
        write: Some(how),
        certain: false,
    }
}

/// How an argument is used, given its index.
type ByArg<'a> = dyn Fn(usize) -> Mode + 'a;

struct Walk<'d> {
    design: &'d Design,
    file: FileId,
    hir: &'d Hir,
    names: &'d Names,
    driver: Driver,
    within: Option<SymbolId>,
    found: Vec<Access>,
}

impl Walk<'_> {
    fn driven(&mut self, driver: Driver, walk: impl FnOnce(&mut Self)) {
        let outer = std::mem::replace(&mut self.driver, driver);
        walk(self);
        self.driver = outer;
    }

    fn access(&mut self, found: Option<Resolution>, at: Span, mode: Mode, whole: bool) {
        self.access_part(found, at, mode, whole, None, None);
    }

    fn access_part(
        &mut self,
        found: Option<Resolution>,
        at: Span,
        mode: Mode,
        whole: bool,
        place: Option<ExprId>,
        base: Option<ExprId>,
    ) {
        if let Some(Resolution::Declared(symbol)) = found {
            self.found.push(Access {
                symbol,
                at,
                read: mode.read,
                write: mode.write,
                whole,
                place: place.filter(|_| !whole),
                certain: mode.certain,
                driver: self.driver,
                within: self.within,
                base,
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
            Member::Bind(bind) => {
                for &instance in &bind.instances {
                    self.symbol(instance);
                }
            }
            // A `defparam` gives a value rather than driving one.
            Member::Defparam(assignments) => {
                for &assignment in assignments {
                    if let ExprKind::Assign { rhs, .. } = self.hir[assignment].kind {
                        self.expr(rhs, READ);
                    }
                }
            }
            // A net joined to another is driven and read wherever that one
            // is, which this walk does not follow.
            Member::Alias(nets) => self.exprs(nets, guess(Write::Other)),
            Member::DefaultDisable(expr)
            | Member::DefaultClocking(expr)
            | Member::ElaborationTask(expr) => self.expr(*expr, READ),
            Member::Opaque(opaque) => self.opaque(opaque),
        }
    }

    fn symbol(&mut self, id: SymbolId) {
        let hir = self.hir;
        let symbol = &hir[id];
        match &symbol.kind {
            SymbolKind::Definition { scope, .. } => {
                let outer = self.within.replace(id);
                self.scope(*scope);
                self.within = outer;
            }
            SymbolKind::Class(class) => {
                if let Some((base, args)) = &class.extends {
                    self.ty(base);
                    self.args(args, &|_| READ);
                }
                for ty in &class.implements {
                    self.ty(ty);
                }
                self.scope(class.scope);
            }
            // Each constraint reads what it names.
            SymbolKind::Constraint(body) => {
                if let Some(body) = body {
                    self.stmt(*body);
                }
            }
            SymbolKind::Other(body) => self.opaque(body),
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
                if let Some(delay) = &data.delay {
                    self.timing(delay);
                }
                self.init(id, data.init);
            }
            SymbolKind::Genvar(init) => self.init(id, *init),
            SymbolKind::Typedef(ty) => self.ty(ty),
            SymbolKind::Forward => {}
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
            SymbolKind::Property { scope, spec, .. } => {
                self.scope(*scope);
                self.spec(spec);
            }
            SymbolKind::Let { scope, expr, .. } => {
                self.scope(*scope);
                self.expr(*expr, READ);
            }
            SymbolKind::Modport(items) => {
                for item in items {
                    let mode = match item.direction {
                        INPUT_KW => MAYBE_READ,
                        OUTPUT_KW => MAYBE_WRITE,
                        CLOCKING_KW => READ,
                        _ => guess(Write::Other),
                    };
                    self.expr(item.value, mode);
                }
            }
            // An input is sampled on every event; an output drives only
            // when a procedure drives it through the block.
            SymbolKind::Clocking(clocking) => {
                self.timing(&clocking.event);
                for skew in &clocking.skews {
                    self.timing(skew);
                }
                for signal in &clocking.signals {
                    let mode = match signal.direction {
                        INPUT_KW => READ,
                        OUTPUT_KW => MAYBE_WRITE,
                        _ => guess(Write::Other),
                    };
                    self.expr(signal.signal, mode);
                }
                self.scope(clocking.scope);
            }
            SymbolKind::Covergroup(covergroup) => {
                self.scope(covergroup.scope);
                if let Some(event) = &covergroup.event {
                    self.timing(event);
                }
                for option in &covergroup.options {
                    self.expr(option.value, READ);
                }
                self.scope(covergroup.points);
            }
            SymbolKind::Coverpoint(point) => {
                if let Some(ty) = &point.ty {
                    self.ty(ty);
                }
                self.exprs(&point.exprs, READ);
                self.value(point.iff, READ);
                for bins in &point.bins {
                    self.exprs(&bins.values, READ);
                    if let Some(with) = &bins.with {
                        self.expr(with.expr, READ);
                    }
                    self.value(bins.iff, READ);
                }
                for option in &point.options {
                    self.expr(option.value, READ);
                }
            }
            SymbolKind::Instance(instance) => self.instance(id, instance),
            SymbolKind::Block(_) | SymbolKind::Label(_) | SymbolKind::Iterator => {}
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
            walk.access(
                Some(Resolution::Declared(symbol)),
                at,
                write(Write::Init),
                true,
            );
            walk.expr(value, READ);
        });
    }

    fn instance(&mut self, id: SymbolId, instance: &Instance) {
        self.args(&instance.parameters, &|_| READ);
        if let Some(delay) = &instance.delay {
            self.timing(delay);
        }
        self.dims(&instance.dims);
        let definition = self.design.instantiated(SymbolRef {
            file: self.file,
            symbol: id,
        });
        let definition = definition.map(|at| (at, &self.design.symbol(at).kind));
        let modes = match (instance.gate, definition) {
            (Some(gate), _) => terminals(gate, instance.connections.len()),
            (None, Some((at, SymbolKind::Definition { ports, .. }))) => {
                let hir = &self.design[at.file];
                modes(hir, ports, &instance.connections, Write::Connection)
            }
            _ => Vec::new(),
        };
        self.driven(Driver::Instance(id), |walk| {
            walk.args(&instance.connections, &|index| {
                (modes.get(index).copied()).unwrap_or(guess(Write::Connection))
            });
            for arg in &instance.connections {
                let Arg::Wildcard(at) = arg else {
                    continue;
                };
                for &(port, found) in walk.names.wildcard(*at) {
                    let kind = &walk.design.symbol(port).kind;
                    let mode = direction(kind, Write::Connection);
                    walk.access(Some(found), instance.definition.span, mode, true);
                }
            }
        });
    }

    /// `args`, each used as `mode` says, given its index.
    fn args(&mut self, args: &[Arg], mode: &ByArg) {
        for (index, arg) in args.iter().enumerate() {
            match arg {
                Arg::Positional(value) | Arg::Named { value, .. } => {
                    self.value(*value, mode(index));
                }
                Arg::Implicit(name) => {
                    let found = self.names.loose_at(name.span);
                    self.access(found, name.span, mode(index), true);
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
            StmtKind::Trigger(event) => self.expr(*event, both(Write::Other)),
            // A procedural continuous assignment overrides what drives its
            // target rather than adding a driver; `release` and `deassign`
            // name what they stop overriding.
            StmtKind::ProceduralAssign { keyword, expr } => {
                let (expr, keyword) = (*expr, *keyword);
                self.driven(Driver::Other, |walk| match keyword {
                    RELEASE_KW | DEASSIGN_KW => walk.expr(expr, write(Write::Other)),
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
                ..
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
                        let mode = write(Write::Init);
                        self.access(found, hir[*id].name.span, mode, true);
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
            StmtKind::ConcurrentAssertion {
                spec, pass, fail, ..
            } => {
                self.spec(spec);
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

    fn spec(&mut self, spec: &PropertySpec) {
        if let Some(clock) = &spec.clock {
            self.timing(clock);
        }
        self.value(spec.disable, READ);
        self.expr(spec.property, READ);
    }

    fn timing(&mut self, timing: &Timing) {
        match timing {
            Timing::Delay(_, values) => {
                for &value in values {
                    self.expr(value, READ);
                }
            }
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
            // The value is read where it is matched.
            TypeKind::Matched { .. } => {}
            TypeKind::Enum { base, .. } => self.ty(base),
            TypeKind::Struct { fields, .. } => {
                for field in fields {
                    self.ty(&field.ty);
                    if let Some(default) = field.default {
                        self.expr(default, READ);
                    }
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
                Dim::Empty | Dim::Wildcard => {}
            }
        }
    }

    /// The names a region not lowered spells, each of which it may read or
    /// write.
    fn opaque(&mut self, opaque: &Opaque) {
        for name in &opaque.names {
            let found = self.names.loose_at(name.span);
            self.access(found, name.span, guess(Write::Other), false);
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
        self.part(expr, mode, None);
    }

    /// `expr`, used as `mode`; `place` the outermost select or member it is
    /// the base of, when it is not all of what it names.
    fn part(&mut self, expr: ExprId, mode: Mode, place: Option<ExprId>) {
        let hir = self.hir;
        let at = hir[expr].span;
        let whole = place.is_none();
        let outer = Some(place.unwrap_or(expr));
        match &hir[expr].kind {
            ExprKind::Name(_) | ExprKind::Scoped { .. } => {
                self.access_part(self.names.expr(expr), at, mode, whole, place, None);
                if let ExprKind::Scoped { base, .. } = hir[expr].kind {
                    // The package or class before `::` is only named.
                    self.access(self.names.expr(base), hir[base].span, READ, true);
                }
            }
            ExprKind::System(_)
            | ExprKind::Keyword(_)
            | ExprKind::Literal(_)
            | ExprKind::Time { .. }
            | ExprKind::Bind(None) => {}
            // A match assigns the variable what it matches.
            ExprKind::Bind(Some(symbol)) => {
                let at = hir[*symbol].name.span;
                let symbol = SymbolRef {
                    file: self.file,
                    symbol: *symbol,
                };
                let mode = write(Write::Match);
                self.access(Some(Resolution::Declared(symbol)), at, mode, true);
            }
            ExprKind::Unary { op, operand } | ExprKind::Postfix { op, operand } => {
                let mode = match op {
                    PLUS_PLUS | MINUS_MINUS => both(Write::Assign),
                    _ => READ,
                };
                self.expr(*operand, mode);
            }
            ExprKind::Binary { lhs, rhs, .. } => self.exprs(&[*lhs, *rhs], READ),
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
                ..
            } => self.exprs(&[*condition, *then_value, *else_value], READ),
            ExprKind::Select { base, index, range } => {
                self.part(*base, mode, outer);
                self.expr(*index, READ);
                self.value(range.map(|(_, end)| end), READ);
            }
            ExprKind::Member { base, name } => {
                // A member of an instance, a generate block or an
                // interface is a symbol of its own, accessed from here.
                if let found @ Some(Resolution::Declared(_)) = self.names.expr(expr) {
                    self.access_part(found, name.span, mode, whole, place, Some(*base));
                }
                let through = Mode {
                    write: mode.write.map(|_| Write::Through),
                    ..mode
                };
                self.part(*base, through, outer);
            }
            // On the left of an assignment, each part is written.
            ExprKind::Concat(parts) => {
                for &part in parts {
                    self.part(part, mode, place);
                }
            }
            ExprKind::Replication { count, concat } => self.exprs(&[*count, *concat], READ),
            ExprKind::Stream { slice, concat, .. } => {
                self.value(*slice, READ);
                self.part(*concat, mode, place);
            }
            ExprKind::Pattern { ty, items } => {
                self.value(*ty, READ);
                for item in items {
                    self.value(item.key, READ);
                    self.part(item.value, mode, place);
                }
            }
            ExprKind::Cast { ty, operand } => self.exprs(&[*ty, *operand], READ),
            ExprKind::Range { lo, hi, .. } => self.exprs(&[*lo, *hi], READ),
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
                    EQ | LT_EQ => write(Write::Assign),
                    _ => both(Write::Assign),
                };
                self.expr(*lhs, target);
                self.expr(*rhs, READ);
                if let Some(timing) = timing {
                    self.timing(timing);
                }
            }
            ExprKind::Call { callee, args } => self.call(*callee, args),
            ExprKind::With { base, with } => self.exprs(&[*base, with.expr], READ),
            ExprKind::Constrained {
                call, constraints, ..
            } => {
                self.expr(*call, READ);
                self.stmt(*constraints);
            }
            ExprKind::Dist { expr: tested, set } => {
                self.expr(*tested, READ);
                self.exprs(set, READ);
            }
            ExprKind::Binsof { bins, intersect } => {
                self.expr(*bins, READ);
                self.exprs(intersect, READ);
            }
            ExprKind::Tagged { value, .. } => self.value(*value, READ),
            ExprKind::Type(ty) => self.ty(ty),
            // What a match assigns is an assignment of its own.
            ExprKind::Property(property) => {
                if let Property::Clocked { clock, .. } = property {
                    self.timing(clock);
                }
                self.exprs(&property.operands(), READ);
            }
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
                self.args(args, &|index| match system_writes(&name, index) {
                    true => both(Write::Argument),
                    false => READ,
                });
                return;
            }
            // A method may change the object it is called on.
            ExprKind::Member { base, .. } => self.part(*base, guess(Write::Argument), Some(callee)),
            _ => self.expr(callee, READ),
        }
        // An argument to a subroutine not found, a method through a handle
        // among them, may be an output.
        let modes = match self.names.expr(callee) {
            Some(Resolution::Declared(at)) => match &self.design.symbol(at).kind {
                SymbolKind::Subroutine { ports, .. }
                | SymbolKind::Property { ports, .. }
                | SymbolKind::Let { ports, .. } => {
                    modes(&self.design[at.file], ports, args, Write::Argument)
                }
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        let unknown = guess(Write::Argument);
        self.args(args, &|index| modes.get(index).copied().unwrap_or(unknown));
    }
}

/// How each of `args` is used, as the formal of `formals`, declared in
/// `hir`, it is bound to says; a write is `how`.
fn modes(hir: &Hir, formals: &[SymbolId], args: &[Arg], how: Write) -> Vec<Mode> {
    (bind::formals(hir, formals, args).into_iter())
        .map(|formal| formal.map_or(guess(how), |formal| direction(&hir[formal].kind, how)))
        .collect()
}

/// How a port is used from where it is connected or passed; a write is
/// `how`.
fn direction(kind: &SymbolKind, how: Write) -> Mode {
    let SymbolKind::Port(port) = kind else {
        return guess(how);
    };
    match port.direction {
        Some(INPUT_KW) => READ,
        Some(OUTPUT_KW) => write(how),
        Some(_) => both(how),
        // An interface port, which may carry either way.
        None => guess(how),
    }
}

/// How a gate or a switch uses each of its `count` terminals: a `buf` or a
/// `not` drives all but its last, a pass switch carries both ways between
/// its first two, and any other drives its first.
fn terminals(gate: SyntaxKind, count: usize) -> Vec<Mode> {
    let driven = match gate {
        BUF_KW | NOT_KW => count.saturating_sub(1),
        PULLUP_KW | PULLDOWN_KW => count,
        _ => 1,
    };
    let carried = match gate {
        TRAN_KW | RTRAN_KW | TRANIF0_KW | TRANIF1_KW | RTRANIF0_KW | RTRANIF1_KW => 2,
        _ => 0,
    };
    (0..count)
        .map(|index| match index {
            _ if index < carried => both(Write::Connection),
            _ if index < driven => write(Write::Connection),
            _ => READ,
        })
        .collect()
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
