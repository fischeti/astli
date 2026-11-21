//! Each place in a frame of what is built where a value is given to
//! something of a type, or two are compared: what typing checks the
//! conversion of, and the width rules the widths of.

use astli_syntax::SyntaxKind::{self, *};

use crate::bind::{Bound, bind};
use crate::built::Frame;
use crate::design::SymbolRef;
use crate::elab::{BodyId, Elaboration, Operator, Walked};
use crate::hir::*;
use crate::resolve::Resolution;

/// A value given to something of a type, or two values compared.
#[derive(Debug, Clone)]
pub struct Conversion {
    pub kind: ConversionKind,
    /// Whether it stands in a class, where nothing is evaluated: typing
    /// reads it, and the width rules do not.
    pub in_class: bool,
    /// Whether a branch around it that a constant condition rules out
    /// never evaluates it, as one parameter's value or another's decides.
    pub unevaluated: bool,
}

#[derive(Debug, Clone)]
pub enum ConversionKind {
    /// `lhs = rhs` or `<=`, or a compound assignment, by its operator.
    Assign {
        op: SyntaxKind,
        lhs: ExprId,
        rhs: ExprId,
    },
    /// A net's or a variable's initialiser.
    Init { symbol: SymbolId, value: ExprId },
    /// A parameter's default, where the frame's body is given no other.
    Default { parameter: SymbolId, value: ExprId },
    /// An override of a parameter of `definition` by the instance, in the
    /// body it builds.
    Override {
        instance: SymbolId,
        definition: SymbolRef,
        body: BodyId,
        parameter: SymbolId,
        value: ExprId,
    },
    /// A connection of the instance to an input or an output of
    /// `definition`, in the body it builds.
    Connection {
        instance: SymbolId,
        definition: SymbolRef,
        body: BodyId,
        port: SymbolId,
        output: bool,
        connection: Bound,
    },
    /// An argument of a call to an input of the subroutine.
    Argument {
        subroutine: SymbolRef,
        formal: SymbolId,
        value: ExprId,
    },
    /// What a function returns.
    Return { function: SymbolId, value: ExprId },
    /// A comparison's operands, by its operator.
    Compare {
        op: SyntaxKind,
        lhs: ExprId,
        rhs: ExprId,
    },
}

impl Elaboration {
    /// Gives `f` each conversion in `frame`, in the order of a walk of its
    /// members, each before what it holds. A property's operators, a type,
    /// a pattern's keys, what `::` follows and constraints hold none.
    /// Which branches a constant condition rules out is read through
    /// `walked`, the frame body's.
    pub fn conversions(&self, frame: &Frame, walked: &Walked, f: impl FnMut(Conversion)) {
        let mut walk = Walk {
            elaboration: self,
            frame,
            walked,
            unevaluated: 0,
            hir: &self.design()[frame.file],
            in_class: false,
            function: None,
            f,
        };
        walk.scope(frame.scope);
    }
}

struct Walk<'a, F> {
    elaboration: &'a Elaboration,
    frame: &'a Frame,
    walked: &'a Walked<'a>,
    /// How many branches around what is walked a constant condition rules
    /// out.
    unevaluated: u32,
    hir: &'a Hir,
    in_class: bool,
    /// The function whose body is walked, if it returns a value.
    function: Option<SymbolId>,
    f: F,
}

impl<F: FnMut(Conversion)> Walk<'_, F> {
    fn give(&mut self, kind: ConversionKind) {
        let (in_class, unevaluated) = (self.in_class, self.unevaluated > 0);
        (self.f)(Conversion {
            kind,
            in_class,
            unevaluated,
        });
    }

    /// Walks what `walk` does as a branch that is `evaluated`, or not.
    fn branch(&mut self, evaluated: bool, walk: impl FnOnce(&mut Self)) {
        self.unevaluated += u32::from(!evaluated);
        walk(self);
        self.unevaluated -= u32::from(!evaluated);
    }

    fn scope(&mut self, scope: ScopeId) {
        let hir = self.hir;
        for member in &hir[scope].members {
            match member {
                Member::Assign(assignments) => {
                    for &assignment in assignments {
                        self.expr(assignment);
                    }
                }
                Member::Process(process) => self.stmt(process.body),
                Member::Declare(symbol) => self.declared(*symbol),
                _ => {}
            }
        }
    }

    fn declared(&mut self, symbol: SymbolId) {
        let hir = self.hir;
        match &hir[symbol].kind {
            SymbolKind::Net(data) | SymbolKind::Variable(data) => {
                if let Some(value) = data.init {
                    self.give(ConversionKind::Init { symbol, value });
                    self.expr(value);
                }
            }
            // An override replaces the default, which is then never
            // evaluated, and may not fit the parameters the instance gives
            // the rest. The override is given where it stands.
            SymbolKind::Parameter(Parameter {
                is_type: false,
                value: Some(value),
                ..
            }) if !self.elaboration.is_given(self.frame.body, symbol) => {
                let value = *value;
                self.give(ConversionKind::Default {
                    parameter: symbol,
                    value,
                });
                self.expr(value);
            }
            SymbolKind::Instance(instance) if instance.dims.is_empty() => self.instance(symbol),
            SymbolKind::Subroutine {
                scope,
                returns,
                body,
                ..
            } => {
                let outer = std::mem::replace(&mut self.function, returns.as_ref().map(|_| symbol));
                self.locals(*scope);
                for &stmt in body {
                    self.stmt(stmt);
                }
                self.function = outer;
            }
            SymbolKind::Class(class) => {
                let outer = std::mem::replace(&mut self.in_class, true);
                self.scope(class.scope);
                self.in_class = outer;
            }
            _ => {}
        }
    }

    /// The initialisers of the variables a block, a loop or a function
    /// declares.
    fn locals(&mut self, scope: ScopeId) {
        let hir = self.hir;
        for member in &hir[scope].members {
            if let Member::Declare(symbol) = member
                && let SymbolKind::Variable(_) = hir[*symbol].kind
            {
                self.declared(*symbol);
            }
        }
    }

    /// Each override of `instance` and each of its connections to an input
    /// or an output, in the body it builds.
    fn instance(&mut self, instance: SymbolId) {
        let elaboration = self.elaboration;
        let at = SymbolRef {
            file: self.frame.file,
            symbol: instance,
        };
        let Some((definition, body)) = elaboration.child(self.frame.body, at) else {
            return;
        };
        let theirs = &elaboration.design()[definition.file];
        for (parameter, value) in elaboration.overrides(at) {
            if let SymbolKind::Parameter(Parameter { is_type: false, .. }) = theirs[parameter].kind
            {
                self.give(ConversionKind::Override {
                    instance,
                    definition,
                    body,
                    parameter,
                    value,
                });
            }
        }
        for (port, connection) in elaboration.connections(at) {
            let SymbolKind::Port(Port {
                direction: Some(direction @ (INPUT_KW | OUTPUT_KW)),
                ..
            }) = theirs[port].kind
            else {
                continue;
            };
            self.give(ConversionKind::Connection {
                instance,
                definition,
                body,
                port,
                output: direction == OUTPUT_KW,
                connection,
            });
        }
    }

    fn stmt(&mut self, stmt: StmtId) {
        let hir = self.hir;
        match &hir[stmt].kind {
            StmtKind::Block { scope, .. } => self.locals(*scope),
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.expr(*condition);
                let taken = self.walked.taken(*condition);
                for (stmt, way) in [(then_branch, true), (else_branch, false)] {
                    if let Some(stmt) = stmt {
                        self.branch(taken != Some(!way), |walk| walk.stmt(*stmt));
                    }
                }
                return;
            }
            StmtKind::For { header, .. } => self.locals(header.scope),
            StmtKind::Return(Some(value)) => {
                if let Some(function) = self.function {
                    let value = *value;
                    self.give(ConversionKind::Return { function, value });
                }
            }
            // What a property's operators build holds none.
            StmtKind::ConcurrentAssertion { pass, fail, .. } => {
                for stmt in [pass, fail].into_iter().flatten() {
                    self.stmt(*stmt);
                }
                return;
            }
            _ => {}
        }
        hir.stmt_children(stmt, |child| self.child(child));
    }

    fn expr(&mut self, expr: ExprId) {
        let hir = self.hir;
        match &hir[expr].kind {
            ExprKind::Assign { op, lhs, rhs, .. } => {
                let (op, lhs, rhs) = (*op, *lhs, *rhs);
                self.give(ConversionKind::Assign { op, lhs, rhs });
            }
            ExprKind::Binary { op, lhs, rhs } if Operator::of(*op) == Some(Operator::Compare) => {
                let (op, lhs, rhs) = (*op, *lhs, *rhs);
                self.give(ConversionKind::Compare { op, lhs, rhs });
            }
            // The right of `&&`, `||` or `->` is not evaluated where the
            // left decides.
            ExprKind::Binary {
                op: op @ (AMP_AMP | PIPE_PIPE | MINUS_GT),
                lhs,
                rhs,
            } => {
                self.expr(*lhs);
                let decides = self.walked.taken(*lhs) == Some(*op == PIPE_PIPE);
                self.branch(!decides, |walk| walk.expr(*rhs));
                return;
            }
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
                ..
            } => {
                self.expr(*condition);
                let taken = self.walked.taken(*condition);
                self.branch(taken != Some(false), |walk| walk.expr(*then_value));
                self.branch(taken != Some(true), |walk| walk.expr(*else_value));
                return;
            }
            ExprKind::Call { callee, args } => self.arguments(*callee, args),
            // A type holds none, and neither does a pattern's key.
            ExprKind::Pattern { items, .. } => {
                for item in items {
                    self.expr(item.value);
                }
                return;
            }
            ExprKind::Cast { operand, .. } => return self.expr(*operand),
            // What `::` follows names a package or a class; a property's
            // operators and constraints hold none.
            ExprKind::Type(_)
            | ExprKind::Scoped { .. }
            | ExprKind::Property(_)
            | ExprKind::Constrained { .. } => return,
            _ => {}
        }
        hir.expr_children(expr, |child| self.child(child));
    }

    fn child(&mut self, child: Node) {
        match child {
            Node::Expr(expr) => self.expr(expr),
            Node::Stmt(stmt) => self.stmt(stmt),
        }
    }

    /// Each argument of a call of a subroutine the design declares, given
    /// to one of its inputs.
    fn arguments(&mut self, callee: ExprId, args: &[Arg]) {
        let elaboration = self.elaboration;
        let names = elaboration.names(self.frame.file);
        let Some(Resolution::Declared(subroutine)) = names.expr(callee) else {
            return;
        };
        let SymbolKind::Subroutine { ports, .. } = &elaboration.design().symbol(subroutine).kind
        else {
            return;
        };
        let theirs = &elaboration.design()[subroutine.file];
        for (formal, actual) in bind(theirs, ports, args, names) {
            let Bound::Expr(value) = actual else {
                continue;
            };
            // An argument written without a direction has the one before
            // it, and the first an input's.
            let place = ports.iter().position(|&it| it == formal).unwrap_or(0);
            let direction = (ports[..=place].iter().rev()).find_map(|&id| match theirs[id].kind {
                SymbolKind::Port(Port { direction, .. }) => direction,
                _ => None,
            });
            if direction.is_none_or(|direction| direction == INPUT_KW) {
                self.give(ConversionKind::Argument {
                    subroutine,
                    formal,
                    value,
                });
            }
        }
    }
}
