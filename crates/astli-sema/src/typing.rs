//! What typing rejects: a value given where its type does not convert to
//! the type it is given to, and an operator given an operand it does not
//! take.
//!
//! What the tops build is checked in each body with its parameters, so a
//! port has the type its instance gives it. A definition no top reaches is
//! checked as its parameters' defaults build it, each generate block in its
//! own body; since it may be built with other parameters, what it finds is
//! a warning. A package, and each class and function it declares, has one
//! body, and is checked in it.
//!
//! What sema cannot type is silent, and so is what takes its type from
//! where it stands: an untyped pattern, a concatenation assigned to an
//! unpacked array, a fill given to anything but an enum.

use astli_syntax::SyntaxKind::{self, *};
use astli_text::{Diagnostic, Severity, Span};
use rustc_hash::FxHashSet;

use crate::built::{Built, Frame};
use crate::design::{FileId, SymbolRef};
use crate::diagnostics;
use crate::elab::{BodyId, ChildKind, Elaboration};
use crate::hir::*;
use crate::resolve::Resolution;
use crate::types::{Packed, Shape, Ty};
use crate::value::{Literal, literal};

/// What is mistyped in what `built` builds, once however many bodies
/// build it.
pub(crate) fn built(built: &Built, elaboration: &Elaboration) -> Vec<(FileId, Diagnostic)> {
    let mut found = Vec::new();
    let mut seen = FxHashSet::default();
    let mut frames: Vec<&Frame> = built.frames().iter().collect();
    while let Some(frame) = frames.pop() {
        let mut walk = Walk::new(elaboration, frame.file, frame.body, false);
        walk.scope(frame.scope);
        for diagnostic in walk.found {
            if seen.insert((frame.file, diagnostic.at, diagnostic.code)) {
                found.push((frame.file, diagnostic));
            }
        }
        frames.extend(&frame.blocks);
    }
    found
}

/// What is mistyped in `file` outside what the tops build: each package,
/// and, as warnings, each definition no top reaches, at its defaults.
pub(crate) fn at_defaults(
    elaboration: &Elaboration,
    file: FileId,
    reached: impl Fn(SymbolId) -> bool,
) -> Vec<Diagnostic> {
    let hir = &elaboration.design()[file];
    let mut found = Vec::new();
    for member in &hir[hir.root()].members {
        let Member::Declare(symbol) = member else {
            continue;
        };
        let SymbolKind::Definition { keyword, scope, .. } = &hir[*symbol].kind else {
            continue;
        };
        let package = *keyword == PACKAGE_KW;
        if !package && reached(*symbol) {
            continue;
        }
        let Some(body) = elaboration.body(SymbolRef {
            file,
            symbol: *symbol,
        }) else {
            continue;
        };
        // The definition's scope, then each generate block its defaults
        // build.
        let mut bodies = vec![(body, *scope)];
        while let Some((body, scope)) = bodies.pop() {
            let mut walk = Walk::new(elaboration, file, body, !package);
            walk.scope(scope);
            found.extend(walk.found);
            for child in elaboration.children(body).iter() {
                if let ChildKind::Block { body } = child.kind {
                    bodies.push((body, elaboration.scope(body).1));
                }
            }
        }
    }
    found
}

/// Where a value is given: what the message says it is given to.
enum Target<'a> {
    Assign,
    /// An input's connection, or an output's port.
    Port {
        name: &'a str,
        definition: &'a str,
        output: bool,
    },
    Argument {
        name: &'a str,
        function: &'a str,
    },
    Return,
}

struct Walk<'a> {
    elaboration: &'a Elaboration,
    file: FileId,
    body: BodyId,
    /// At a definition's defaults, where what is found is a warning.
    defaults: bool,
    found: Vec<Diagnostic>,
}

impl<'a> Walk<'a> {
    fn new(elaboration: &'a Elaboration, file: FileId, body: BodyId, defaults: bool) -> Self {
        Walk {
            elaboration,
            file,
            body,
            defaults,
            found: Vec::new(),
        }
    }

    fn hir(&self) -> &'a Hir {
        &self.elaboration.design()[self.file]
    }

    fn report(&mut self, mut diagnostic: Diagnostic) {
        if self.defaults {
            diagnostic.severity = Severity::Warning;
        }
        self.found.push(diagnostic);
    }

    fn scope(&mut self, scope: ScopeId) {
        let hir = self.hir();
        for member in &hir[scope].members {
            match member {
                Member::Assign(assignments) => {
                    for &assignment in assignments {
                        self.expr(assignment);
                    }
                }
                Member::Process(process) => self.stmt(process.body, None),
                Member::Declare(symbol) => self.declared(*symbol),
                _ => {}
            }
        }
    }

    fn declared(&mut self, symbol: SymbolId) {
        let hir = self.hir();
        match &hir[symbol].kind {
            SymbolKind::Net(data) | SymbolKind::Variable(data) => {
                if let Some(init) = data.init {
                    let target = self.symbol_type(symbol);
                    self.convert(&target, init, Target::Assign);
                    self.expr(init);
                }
            }
            SymbolKind::Instance(instance) if instance.dims.is_empty() => {
                self.instance(symbol, instance);
            }
            SymbolKind::Subroutine {
                scope,
                returns,
                body,
                ..
            } => {
                let returns = returns.as_ref().map(|_| self.symbol_type(symbol));
                self.locals(*scope);
                for &stmt in body {
                    self.stmt(stmt, returns.as_ref());
                }
            }
            SymbolKind::Class(class) => self.scope(class.scope),
            _ => {}
        }
    }

    /// The initialisers of the variables a block, a loop or a function
    /// declares.
    fn locals(&mut self, scope: ScopeId) {
        let hir = self.hir();
        for member in &hir[scope].members {
            if let Member::Declare(symbol) = member
                && let SymbolKind::Variable(_) = hir[*symbol].kind
            {
                self.declared(*symbol);
            }
        }
    }

    fn symbol_type(&self, symbol: SymbolId) -> Ty {
        let at = SymbolRef {
            file: self.file,
            symbol,
        };
        self.elaboration.symbol_type(self.body, at)
    }

    fn stmt(&mut self, stmt: StmtId, returns: Option<&Ty>) {
        let hir = self.hir();
        let each = |walk: &mut Self, stmt: &Option<StmtId>| {
            if let Some(stmt) = stmt {
                walk.stmt(*stmt, returns);
            }
        };
        match &hir[stmt].kind {
            StmtKind::Expr(expr) | StmtKind::ProceduralAssign { expr, .. } => self.expr(*expr),
            StmtKind::Block { scope, stmts, .. } => {
                self.locals(*scope);
                for &stmt in stmts {
                    self.stmt(stmt, returns);
                }
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr(*condition);
                each(self, then_branch);
                each(self, else_branch);
            }
            StmtKind::Case {
                selector, items, ..
            } => {
                if let Some(selector) = selector {
                    self.expr(*selector);
                }
                for item in items {
                    each(self, &item.body);
                }
            }
            StmtKind::For { header, body } => {
                self.locals(header.scope);
                for &expr in header
                    .init
                    .iter()
                    .chain(&header.condition)
                    .chain(&header.step)
                {
                    self.expr(expr);
                }
                each(self, body);
            }
            StmtKind::Loop {
                condition, body, ..
            }
            | StmtKind::Wait { condition, body } => {
                if let Some(condition) = condition {
                    self.expr(*condition);
                }
                each(self, body);
            }
            StmtKind::Foreach { body, .. } | StmtKind::Timed { body, .. } => each(self, body),
            StmtKind::Return(Some(value)) => {
                if let Some(returns) = returns {
                    self.convert(returns, *value, Target::Return);
                }
                self.expr(*value);
            }
            StmtKind::Assertion {
                condition,
                pass,
                fail,
                ..
            } => {
                self.expr(*condition);
                each(self, pass);
                each(self, fail);
            }
            _ => {}
        }
    }

    fn expr(&mut self, expr: ExprId) {
        let hir = self.hir();
        let mut children = Vec::new();
        match &hir[expr].kind {
            ExprKind::Assign { op, lhs, rhs, .. } => {
                if matches!(op, EQ | LT_EQ) {
                    let target = self.elaboration.expr_type(self.body, *lhs);
                    self.convert(&target, *rhs, Target::Assign);
                }
                children.extend([*lhs, *rhs]);
            }
            ExprKind::Binary { op, lhs, rhs } => {
                self.binary(*op, *lhs, *rhs);
                children.extend([*lhs, *rhs]);
            }
            ExprKind::Unary { op, operand } => {
                self.unary(*op, *operand, hir[expr].span);
                children.push(*operand);
            }
            ExprKind::Postfix { operand, .. } | ExprKind::Cast { operand, .. } => {
                children.push(*operand)
            }
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
            } => children.extend([*condition, *then_value, *else_value]),
            ExprKind::Select { base, index, range } => {
                children.extend([*base, *index]);
                children.extend(range.map(|(_, end)| end));
            }
            ExprKind::Member { base, .. } => children.push(*base),
            ExprKind::Call { callee, args } => {
                self.arguments(*callee, args);
                children.extend(args.iter().filter_map(value));
            }
            ExprKind::Concat(parts) => children.extend(parts),
            ExprKind::Replication { count, concat } => children.extend([*count, *concat]),
            ExprKind::Inside { expr, set } => {
                children.push(*expr);
                children.extend(set);
            }
            ExprKind::Pattern { items, .. } => children.extend(items.iter().map(|it| it.value)),
            _ => {}
        }
        for child in children {
            self.expr(child);
        }
    }

    /// Each connection of `instance` to a port of the body it builds: an
    /// input's connection converted to the port's type, the port's type to
    /// an output's connection.
    fn instance(&mut self, symbol: SymbolId, instance: &Instance) {
        if instance.gate.is_some() {
            return;
        }
        let at = SymbolRef {
            file: self.file,
            symbol,
        };
        let children = self.elaboration.children(self.body);
        let Some((definition, body)) = children.iter().find_map(|child| match child.kind {
            ChildKind::Instance {
                symbol,
                definition,
                body,
                ..
            } if symbol == at => Some((definition, body)),
            _ => None,
        }) else {
            return;
        };
        let design = self.elaboration.design();
        let defined = design.symbol(definition);
        let SymbolKind::Definition { ports, .. } = &defined.kind else {
            return;
        };
        let theirs = &design[definition.file];
        for (index, arg) in instance.connections.iter().enumerate() {
            let Some((port, connection)) = given(theirs, ports, index, arg) else {
                continue;
            };
            let SymbolKind::Port(Port {
                direction: Some(direction),
                ..
            }) = theirs[port].kind
            else {
                continue;
            };
            let ty = self.elaboration.type_of(body, port);
            let target = |output| Target::Port {
                name: &theirs[port].name.text,
                definition: &defined.name.text,
                output,
            };
            match direction {
                INPUT_KW => self.convert(&ty, connection, target(false)),
                OUTPUT_KW => {
                    let into = self.elaboration.expr_type(self.body, connection);
                    if let Some(reason) = mismatch(&into, &ty, None, self.hir()) {
                        let at = self.hir()[connection].span;
                        self.report(incompatible(reason, &ty, &into, target(true), at));
                    }
                }
                _ => {}
            }
        }
    }

    /// Each argument of a call of a function or task the design declares,
    /// converted to its input's type.
    fn arguments(&mut self, callee: ExprId, args: &[Arg]) {
        let Some(Resolution::Declared(at)) = self.elaboration.names(self.file).expr(callee) else {
            return;
        };
        let design = self.elaboration.design();
        let routine = design.symbol(at);
        let SymbolKind::Subroutine { ports, .. } = &routine.kind else {
            return;
        };
        let theirs = &design[at.file];
        for (index, arg) in args.iter().enumerate() {
            let Some((formal, actual)) = given(theirs, ports, index, arg) else {
                continue;
            };
            // An argument written without a direction has the one before
            // it, and the first an input's.
            let place = ports.iter().position(|&it| it == formal).unwrap_or(0);
            let direction = (ports[..=place].iter().rev()).find_map(|&id| match theirs[id].kind {
                SymbolKind::Port(Port { direction, .. }) => direction,
                _ => None,
            });
            if direction.is_some_and(|direction| direction != INPUT_KW) {
                continue;
            }
            let ty = self.elaboration.symbol_type(
                self.body,
                SymbolRef {
                    file: at.file,
                    symbol: formal,
                },
            );
            let target = Target::Argument {
                name: &theirs[formal].name.text,
                function: &routine.name.text,
            };
            self.convert(&ty, actual, target);
        }
    }

    /// Reports `source` where its type does not convert to `target`.
    fn convert(&mut self, target: &Ty, source: ExprId, to: Target) {
        let ty = self.elaboration.expr_type(self.body, source);
        let hir = self.hir();
        if let Some(reason) = mismatch(target, &ty, Some(source), hir) {
            self.report(incompatible(reason, &ty, target, to, hir[source].span));
        }
    }

    fn binary(&mut self, op: SyntaxKind, lhs: ExprId, rhs: ExprId) {
        let Some(spelled) = spelled(op) else {
            return;
        };
        let shift = matches!(op, LT_LT | LT_LT_LT | GT_GT | GT_GT_GT);
        // A shift's amount is an integer whatever it shifts.
        let operands: &[ExprId] = if shift { &[lhs] } else { &[lhs, rhs] };
        for &operand in operands {
            let ty = self.elaboration.expr_type(self.body, operand);
            let refused = match ty {
                Ty::Real => {
                    matches!(op, PERCENT | AMP | PIPE | CARET | TILDE_CARET | CARET_TILDE) || shift
                }
                Ty::String => !matches!(op, EQ_EQ | BANG_EQ | LT | LT_EQ | GT | GT_EQ),
                Ty::Class(_) | Ty::Null => !matches!(op, EQ_EQ | BANG_EQ | EQ_EQ_EQ | BANG_EQ_EQ),
                _ => false,
            };
            if refused {
                let at = self.hir()[operand].span;
                self.report(diagnostics::invalid_operand(spelled, &ty, at));
                return;
            }
        }
    }

    fn unary(&mut self, op: SyntaxKind, operand: ExprId, at: Span) {
        let Some(spelled) = spelled(op) else {
            return;
        };
        let ty = self.elaboration.expr_type(self.body, operand);
        let refused = match ty {
            Ty::Real => matches!(
                op,
                TILDE | AMP | PIPE | CARET | TILDE_AMP | TILDE_PIPE | TILDE_CARET | CARET_TILDE
            ),
            Ty::String | Ty::Class(_) | Ty::Null => op != BANG,
            _ => false,
        };
        if refused {
            self.report(diagnostics::invalid_operand(spelled, &ty, at));
        }
    }
}

/// The port or formal of `ports`, declared in `hir`, that `arg`, the
/// `index`th given, is given to, and what is given.
fn given(hir: &Hir, ports: &[SymbolId], index: usize, arg: &Arg) -> Option<(SymbolId, ExprId)> {
    match arg {
        Arg::Positional(Some(value)) => Some((*ports.get(index)?, *value)),
        Arg::Named {
            name,
            value: Some(value),
        } => {
            let port = ports.iter().find(|&&id| hir[id].name.text == name.text)?;
            Some((*port, *value))
        }
        _ => None,
    }
}

fn value(arg: &Arg) -> Option<ExprId> {
    match arg {
        Arg::Positional(value) | Arg::Named { value, .. } => *value,
        _ => None,
    }
}

/// Why a value of type `source`, written as `expr` where it is known, does
/// not convert to `target`; `None` where it does, or sema cannot tell.
fn mismatch(target: &Ty, source: &Ty, expr: Option<ExprId>, hir: &Hir) -> Option<&'static str> {
    if *source == Ty::Unknown || *target == Ty::Unknown {
        return None;
    }
    let kind = expr.map(|expr| &hir[expr].kind);
    // A concatenation, a fill or a pattern takes the type of an unpacked
    // target, and a string literal is text.
    let shaped = matches!(
        kind,
        Some(ExprKind::Concat(_) | ExprKind::Replication { .. } | ExprKind::Pattern { .. })
    );
    let fill =
        matches!(kind, Some(ExprKind::Literal(text)) if matches!(literal(text), Literal::Fill(_)));
    let text = expr.is_some_and(|expr| textual(hir, expr));
    match target {
        Ty::Packed(Packed {
            shape: Shape::Enum(_),
            ..
        }) => (source != target)
            .then_some("an enum takes only a value of its own type, or one cast to it"),
        Ty::Packed(_) => match source {
            Ty::String => Some("a string converts to an integral type only by a cast"),
            Ty::Array(_) | Ty::Dynamic(_) | Ty::Struct(_) => {
                Some("an unpacked value converts to a packed one only by a cast")
            }
            Ty::Class(_) | Ty::Null => Some("a handle converts to no integral type"),
            _ => None,
        },
        Ty::Real => match source {
            Ty::String
            | Ty::Array(_)
            | Ty::Dynamic(_)
            | Ty::Struct(_)
            | Ty::Class(_)
            | Ty::Null => Some("a real takes only a number"),
            _ => None,
        },
        Ty::String => match source {
            Ty::String => None,
            Ty::Packed(_) if text || fill => None,
            Ty::Packed(_) => Some("a string takes an integral value only by a cast"),
            _ => Some("a string takes only text"),
        },
        Ty::Struct(_) if shaped || fill => None,
        Ty::Struct(_) => {
            (source != target).then_some("an unpacked struct takes only a value of its own type")
        }
        Ty::Array(_) if shaped || fill => None,
        Ty::Array(array) => match source {
            Ty::Array(other)
                if other.left.abs_diff(other.right) == array.left.abs_diff(array.right)
                    && equivalent(&other.element, &array.element) =>
            {
                None
            }
            Ty::Dynamic(_) => None,
            _ => Some(
                "an unpacked array takes only an array of as many elements of an equivalent type",
            ),
        },
        Ty::Class(_) => match source {
            Ty::Class(_) | Ty::Null => None,
            _ => Some("a handle takes only a handle, or `null`"),
        },
        Ty::Dynamic(_) | Ty::Null | Ty::Unknown => None,
    }
}

/// Whether two element types are equivalent: packed ones of one width,
/// signing and states, an enum or struct only to itself; unpacked ones of
/// one shape.
fn equivalent(a: &Ty, b: &Ty) -> bool {
    match (a, b) {
        (Ty::Packed(a), Ty::Packed(b)) => match (&a.shape, &b.shape) {
            (Shape::Enum(_) | Shape::Struct(_), _) | (_, Shape::Enum(_) | Shape::Struct(_)) => {
                a == b
            }
            _ => a.width == b.width && a.signed == b.signed && a.four_state == b.four_state,
        },
        (Ty::Array(a), Ty::Array(b)) => {
            a.left.abs_diff(a.right) == b.left.abs_diff(b.right)
                && equivalent(&a.element, &b.element)
        }
        (Ty::Unknown, _) | (_, Ty::Unknown) => true,
        _ => a == b,
    }
}

/// Whether `expr` is text: a string literal, or a concatenation,
/// replication or choice of them.
fn textual(hir: &Hir, expr: ExprId) -> bool {
    match &hir[expr].kind {
        ExprKind::Literal(text) => matches!(literal(text), Literal::String(_)),
        ExprKind::Concat(parts) => parts.iter().all(|&part| textual(hir, part)),
        ExprKind::Replication { concat, .. } => textual(hir, *concat),
        ExprKind::Conditional {
            then_value,
            else_value,
            ..
        } => textual(hir, *then_value) && textual(hir, *else_value),
        _ => false,
    }
}

fn incompatible(reason: &str, source: &Ty, target: &Ty, to: Target, at: Span) -> Diagnostic {
    let pointing = match to {
        Target::Assign => format!("assigned to a `{target}`"),
        Target::Return => format!("returned as a `{target}`"),
        Target::Port {
            name,
            definition,
            output: false,
        } => format!("connected to port `{name}` of `{definition}`, a `{target}`"),
        Target::Port {
            name,
            definition,
            output: true,
        } => format!("driven by port `{name}` of `{definition}`, a `{source}`"),
        Target::Argument { name, function } => {
            format!("passed to argument `{name}` of `{function}`, a `{target}`")
        }
    };
    diagnostics::incompatible_type(source, target, reason, pointing, at)
}

/// How `op` is written, for the operators a check refuses an operand of.
fn spelled(op: SyntaxKind) -> Option<&'static str> {
    Some(match op {
        PLUS => "+",
        MINUS => "-",
        STAR => "*",
        SLASH => "/",
        PERCENT => "%",
        STAR_STAR => "**",
        AMP => "&",
        PIPE => "|",
        CARET => "^",
        TILDE_CARET => "~^",
        CARET_TILDE => "^~",
        TILDE => "~",
        TILDE_AMP => "~&",
        TILDE_PIPE => "~|",
        BANG => "!",
        LT_LT => "<<",
        LT_LT_LT => "<<<",
        GT_GT => ">>",
        GT_GT_GT => ">>>",
        EQ_EQ => "==",
        BANG_EQ => "!=",
        EQ_EQ_EQ => "===",
        BANG_EQ_EQ => "!==",
        LT => "<",
        LT_EQ => "<=",
        GT => ">",
        GT_EQ => ">=",
        _ => return None,
    })
}
