//! Widths that differ where the standard extends or truncates without a
//! word: an assignment's sides, a port and its connection, a comparison's
//! operands, in what the tops build.

use astli_syntax::SyntaxKind::*;
use astli_text::Span;
use rustc_hash::FxHashSet;

use super::{Built, Frame};
use crate::design::{FileId, SymbolRef};
use crate::elab::{ChildKind, Cx, Elaboration, Operator};
use crate::hir::*;
use crate::resolve::Resolution;
use crate::types::Ty;
use crate::value::{Bits, Literal, Value, literal};

/// Two widths that differ where one is extended or truncated to the other.
#[derive(Debug, Clone)]
pub struct Width {
    pub file: FileId,
    pub kind: WidthKind,
    /// What is extended or truncated: an assignment's right side, a port's
    /// connection, a comparison's narrower operand.
    pub source: Span,
    /// What it is taken to: the left side, the connection again, the wider
    /// operand.
    pub target: Span,
    pub source_width: u32,
    pub target_width: u32,
    /// Where in the tree it is built, the first place if several build it.
    pub path: Box<str>,
}

#[derive(Debug, Clone)]
pub enum WidthKind {
    Assign,
    /// A connection to the port `port` of an instance of `definition`. An
    /// input's connection is the source; an output's port is.
    Connection {
        port: Box<str>,
        definition: Box<str>,
        output: bool,
    },
    Compare,
}

impl Width {
    /// Whether the source loses bits, rather than gains them.
    pub fn truncates(&self) -> bool {
        self.source_width > self.target_width
    }
}

impl Built {
    /// Each place in what is built where two widths differ, once however
    /// many bodies build it: an assignment, a port connection, a
    /// comparison. A constant needs only the bits of its value and takes
    /// any wider width but for a literal written with a size, a sum may
    /// take its carry, and what sema cannot size is silent.
    pub fn widths(&self, elaboration: &Elaboration) -> Vec<Width> {
        let mut walk = Walk {
            elaboration,
            seen: FxHashSet::default(),
            found: Vec::new(),
        };
        for body in &self.bodies {
            walk.frame(body);
        }
        walk.found
    }
}

struct Walk<'a, 'd> {
    elaboration: &'a Elaboration<'d>,
    /// What was reported, by its source and kind.
    seen: FxHashSet<(FileId, Span, u8)>,
    found: Vec<Width>,
}

impl Walk<'_, '_> {
    fn frame(&mut self, frame: &Frame) {
        let cx = self.elaboration.cx(frame.body);
        let hir = &self.elaboration.design()[frame.file];
        for member in &hir[frame.scope].members {
            match member {
                Member::Assign(assignments) => {
                    for &assignment in assignments {
                        self.expr(cx, frame, assignment);
                    }
                }
                Member::Process(process) => self.stmt(cx, frame, process.body),
                Member::Declare(symbol) => match &hir[*symbol].kind {
                    SymbolKind::Net(data) | SymbolKind::Variable(data) => {
                        if let Some(init) = data.init {
                            let span = hir[*symbol].name.span;
                            let at = SymbolRef {
                                file: frame.file,
                                symbol: *symbol,
                            };
                            let target = self.elaboration.symbol_type(cx, at);
                            self.assignment(cx, frame, span, &target, init);
                            self.expr(cx, frame, init);
                        }
                    }
                    SymbolKind::Instance(instance) if instance.dims.is_empty() => {
                        self.instance(cx, frame, *symbol, instance);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        for block in &frame.blocks {
            self.frame(block);
        }
    }

    fn report(&mut self, frame: &Frame, width: Width) {
        let kind = match width.kind {
            WidthKind::Assign => 0,
            WidthKind::Connection { .. } => 1,
            WidthKind::Compare => 2,
        };
        if self.seen.insert((frame.file, width.source, kind)) {
            self.found.push(width);
        }
    }

    /// `target = value`, the target of type `target`.
    fn assignment(&mut self, cx: Cx, frame: &Frame, at: Span, target: &Ty, value: ExprId) {
        let (Some(target_width), Some(range)) = (target.width(), self.range(cx, value)) else {
            return;
        };
        let Some(source_width) = outside(range, target_width) else {
            return;
        };
        let hir = &self.elaboration.design()[frame.file];
        self.report(
            frame,
            Width {
                file: frame.file,
                kind: WidthKind::Assign,
                source: hir[value].span,
                target: at,
                source_width,
                target_width,
                path: frame.path.clone(),
            },
        );
    }

    /// The widths `expr` may be taken at without a word. `None` if it is
    /// not packed.
    fn range(&self, cx: Cx, expr: ExprId) -> Option<Range> {
        let elaboration = self.elaboration;
        let hir = &elaboration.design()[cx.file()];
        let own = elaboration.self_type(cx, expr).width()?;
        let ExprKind::Literal(text) = &hir[expr].kind else {
            return Some(match elaboration.eval_self(cx, expr) {
                Value::Bits(bits) if !bits.has_unknown() => Range::constant(needed(&bits), own),
                // A constant sema cannot evaluate may need any of its bits.
                Value::Unknown if self.constant(cx, expr) => Range {
                    least: 1,
                    own,
                    room: u32::MAX,
                    grows: u32::MAX,
                },
                _ => self.operation(cx, expr, own),
            });
        };
        Some(match literal(text) {
            Literal::Fill(_) => Range::constant(1, own),
            // The width written is the one it has.
            Literal::Bits(bits)
                if text
                    .split_once('\'')
                    .is_some_and(|(size, _)| !size.is_empty()) =>
            {
                Range {
                    room: own,
                    ..Range::constant(needed(&bits), own)
                }
            }
            Literal::Bits(bits) => Range::constant(needed(&bits), own),
            _ => Range::constant(own, own),
        })
    }

    /// The range of `expr`, not a constant, of width `own` by itself.
    /// Constants in it count only the bits of their values, as they would
    /// by themselves: `a + 1` is as wide as `a`.
    fn operation(&self, cx: Cx, expr: ExprId, own: u32) -> Range {
        let hir = &self.elaboration.design()[cx.file()];
        let range = |expr: ExprId| self.range(cx, expr).unwrap_or(Range::of(own));
        let (least, room) = match &hir[expr].kind {
            ExprKind::Binary { op, lhs, rhs } => {
                let (lhs, rhs) = (range(*lhs), range(*rhs));
                let least = lhs.least.max(rhs.least);
                match *op {
                    PLUS | MINUS => (least, lhs.grows.max(rhs.grows).saturating_add(1)),
                    STAR => (least, lhs.grows.saturating_add(rhs.grows)),
                    // A shift left fills what it is taken at, as a power may.
                    LT_LT | LT_LT_LT | STAR_STAR => (lhs.least, u32::MAX),
                    GT_GT | GT_GT_GT => (lhs.least, lhs.grows),
                    _ if Operator::of(*op) == Some(Operator::Context) => {
                        (least, lhs.grows.max(rhs.grows))
                    }
                    _ => (own, own),
                }
            }
            ExprKind::Conditional {
                then_value,
                else_value,
                ..
            } => {
                let (then_value, else_value) = (range(*then_value), range(*else_value));
                let least = then_value.least.max(else_value.least);
                let grows = then_value.grows.max(else_value.grows);
                // Either constant it takes is one by itself.
                if then_value.room == u32::MAX && else_value.room == u32::MAX {
                    return Range {
                        least,
                        own,
                        room: u32::MAX,
                        grows,
                    };
                }
                (least, grows)
            }
            ExprKind::Unary { op: PLUS, operand } => {
                let operand = range(*operand);
                (operand.least, operand.grows)
            }
            ExprKind::Unary {
                op: MINUS | TILDE,
                operand,
            } => {
                let least = range(*operand).least;
                (least, least)
            }
            _ => (own, own),
        };
        let least = least.min(own);
        let room = room.max(least);
        Range {
            least,
            own: least,
            room,
            grows: room,
        }
    }

    /// Whether `expr` is a constant: of literals, parameters, enum members
    /// and genvars, and names sema cannot see, which may be any of those. A
    /// loop's variable counts, as a loop with constant bounds is unrolled
    /// into one constant per iteration.
    fn constant(&self, cx: Cx, expr: ExprId) -> bool {
        let elaboration = self.elaboration;
        let hir = &elaboration.design()[cx.file()];
        let all = |exprs: &[ExprId]| exprs.iter().all(|&expr| self.constant(cx, expr));
        match &hir[expr].kind {
            ExprKind::Literal(_) => true,
            ExprKind::Name(_) | ExprKind::Scoped { .. } => match elaboration.resolution(cx, expr) {
                Some(Resolution::Declared(at)) => match elaboration.design().symbol(at).kind {
                    SymbolKind::Parameter(Parameter { is_type: false, .. })
                    | SymbolKind::EnumMember { .. }
                    | SymbolKind::Genvar(_) => true,
                    SymbolKind::Variable(_) => {
                        let home = elaboration.home(at.file, at.symbol);
                        elaboration.design()[at.file][home].kind == ScopeKind::Loop
                    }
                    _ => false,
                },
                Some(Resolution::Unknown) => true,
                _ => false,
            },
            ExprKind::Unary { operand, .. } | ExprKind::Cast { operand, .. } => {
                self.constant(cx, *operand)
            }
            ExprKind::Binary { lhs, rhs, .. } => all(&[*lhs, *rhs]),
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
            } => all(&[*condition, *then_value, *else_value]),
            ExprKind::Concat(parts) => all(parts),
            ExprKind::Replication { count, concat } => all(&[*count, *concat]),
            ExprKind::Select { base, index, range } => {
                all(&[*base, *index]) && range.is_none_or(|(_, end)| self.constant(cx, end))
            }
            ExprKind::Member { base, .. } => self.constant(cx, *base),
            ExprKind::Call { callee, args } => {
                matches!(hir[*callee].kind, ExprKind::System(_))
                    && args.iter().all(|arg| match arg {
                        Arg::Positional(Some(arg)) => self.constant(cx, *arg),
                        _ => false,
                    })
            }
            _ => false,
        }
    }

    fn stmt(&mut self, cx: Cx, frame: &Frame, stmt: StmtId) {
        let hir = &self.elaboration.design()[frame.file];
        let stmts = |list: &[Option<StmtId>], walk: &mut Self| {
            for &stmt in list.iter().flatten() {
                walk.stmt(cx, frame, stmt);
            }
        };
        match &hir[stmt].kind {
            StmtKind::Expr(expr) => self.expr(cx, frame, *expr),
            StmtKind::Block { stmts: list, .. } => {
                for &stmt in list {
                    self.stmt(cx, frame, stmt);
                }
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr(cx, frame, *condition);
                stmts(&[*then_branch, *else_branch], self);
            }
            StmtKind::Case {
                selector, items, ..
            } => {
                if let Some(selector) = selector {
                    self.expr(cx, frame, *selector);
                }
                for item in items {
                    stmts(&[item.body], self);
                }
            }
            StmtKind::For { header, body } => {
                for &expr in header.init.iter().chain(&header.condition) {
                    self.expr(cx, frame, expr);
                }
                stmts(&[*body], self);
            }
            StmtKind::Loop {
                condition, body, ..
            }
            | StmtKind::Wait { condition, body } => {
                if let Some(condition) = condition {
                    self.expr(cx, frame, *condition);
                }
                stmts(&[*body], self);
            }
            StmtKind::Foreach { body, .. } | StmtKind::Timed { body, .. } => {
                stmts(&[*body], self);
            }
            StmtKind::Assertion {
                condition,
                pass,
                fail,
                ..
            } => {
                self.expr(cx, frame, *condition);
                stmts(&[*pass, *fail], self);
            }
            _ => {}
        }
    }

    /// The assignments and comparisons in `expr`.
    fn expr(&mut self, cx: Cx, frame: &Frame, expr: ExprId) {
        let hir = &self.elaboration.design()[frame.file];
        match &hir[expr].kind {
            ExprKind::Assign {
                op: EQ | LT_EQ,
                lhs,
                rhs,
                ..
            } => {
                let target = self.elaboration.self_type(cx, *lhs);
                self.assignment(cx, frame, hir[*lhs].span, &target, *rhs);
                self.expr(cx, frame, *rhs);
            }
            ExprKind::Binary { op, lhs, rhs } => {
                if Operator::of(*op) == Some(Operator::Compare) {
                    self.comparison(cx, frame, *lhs, *rhs);
                }
                self.expr(cx, frame, *lhs);
                self.expr(cx, frame, *rhs);
            }
            ExprKind::Unary { operand, .. } => self.expr(cx, frame, *operand),
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
            } => {
                for &part in [condition, then_value, else_value] {
                    self.expr(cx, frame, part);
                }
            }
            ExprKind::Concat(parts) => {
                for &part in parts {
                    self.expr(cx, frame, part);
                }
            }
            _ => {}
        }
    }

    fn comparison(&mut self, cx: Cx, frame: &Frame, lhs: ExprId, rhs: ExprId) {
        let (Some(left), Some(right)) = (self.range(cx, lhs), self.range(cx, rhs)) else {
            return;
        };
        // The operands agree if some width is in both ranges.
        let (source, target, narrow, wide) = match () {
            _ if left.room < right.least => (lhs, rhs, left.own, right.least),
            _ if right.room < left.least => (rhs, lhs, right.own, left.least),
            _ => return,
        };
        let hir = &self.elaboration.design()[frame.file];
        self.report(
            frame,
            Width {
                file: frame.file,
                kind: WidthKind::Compare,
                source: hir[source].span,
                target: hir[target].span,
                source_width: narrow,
                target_width: wide,
                path: frame.path.clone(),
            },
        );
    }

    /// Each connection of `instance` whose width differs from its port's,
    /// the port as the body it is built with sizes it.
    fn instance(&mut self, cx: Cx, frame: &Frame, symbol: SymbolId, instance: &Instance) {
        let elaboration = self.elaboration;
        let design = elaboration.design();
        let children = elaboration.children(frame.body);
        let Some((definition, body)) = children.iter().find_map(|child| match child.kind {
            ChildKind::Instance {
                symbol: it,
                definition,
                body,
                ..
            } if it == symbol => Some((definition, body)),
            _ => None,
        }) else {
            return;
        };
        let SymbolKind::Definition { ports, .. } = &design.symbol(definition).kind else {
            return;
        };
        let declared = &design[definition.file];
        let names = elaboration.names(frame.file);
        // Each port connected, and what connects it: an expression, or a
        // name resolved where the instance stands.
        let mut connected: Vec<(SymbolId, Connection)> = Vec::new();
        for (index, arg) in instance.connections.iter().enumerate() {
            let port = |name: &str| {
                ports
                    .iter()
                    .copied()
                    .find(|&it| &*declared[it].name.text == name)
            };
            match arg {
                Arg::Positional(Some(value)) => {
                    if let Some(&port) = ports.get(index) {
                        connected.push((port, Connection::Expr(*value)));
                    }
                }
                Arg::Named {
                    name,
                    value: Some(value),
                } => {
                    if let Some(port) = port(&name.text) {
                        connected.push((port, Connection::Expr(*value)));
                    }
                }
                Arg::Implicit(name) => {
                    if let (Some(port), Some(Resolution::Declared(at))) =
                        (port(&name.text), names.loose_at(name.span))
                    {
                        connected.push((port, Connection::Name(at, name.span)));
                    }
                }
                _ => {}
            }
        }
        for (port, found) in names.wildcard(symbol) {
            if let Resolution::Declared(at) = found {
                let at_span = instance
                    .connections
                    .iter()
                    .find_map(|arg| match arg {
                        Arg::Wildcard(span) => Some(*span),
                        _ => None,
                    })
                    .unwrap_or(declared[port.symbol].name.span);
                connected.push((port.symbol, Connection::Name(*at, at_span)));
            }
        }

        let defined: Box<str> = design.symbol(definition).name.text.clone();
        for (port, connection) in connected {
            let SymbolKind::Port(declaration) = &declared[port].kind else {
                continue;
            };
            let Some(port_width) = elaboration.type_of(body, port).width() else {
                continue;
            };
            let (range, at) = match connection {
                Connection::Expr(expr) => (self.range(cx, expr), design[frame.file][expr].span),
                Connection::Name(at, span) => {
                    let width = elaboration.symbol_type(cx, at).width();
                    (width.map(Range::of), span)
                }
            };
            let Some(range) = range else {
                continue;
            };
            // An input takes its connection's value, and an output gives
            // the port's to it.
            let (source_width, target_width, output) = match declaration.direction {
                Some(INPUT_KW) => match outside(range, port_width) {
                    Some(source_width) => (source_width, port_width, false),
                    None => continue,
                },
                Some(OUTPUT_KW) if range.own != port_width => (port_width, range.own, true),
                _ => continue,
            };
            self.report(
                frame,
                Width {
                    file: frame.file,
                    kind: WidthKind::Connection {
                        port: declared[port].name.text.clone(),
                        definition: defined.clone(),
                        output,
                    },
                    source: at,
                    target: at,
                    source_width,
                    target_width,
                    path: frame.path.clone(),
                },
            );
        }
    }
}

/// What connects a port.
enum Connection {
    Expr(ExprId),
    /// A name, `.name` or by `.*`, resolved where the instance stands, and
    /// where it is written.
    Name(SymbolRef, Span),
}

/// The widths an expression may be taken at without a word.
#[derive(Clone, Copy)]
struct Range {
    /// The bits its value needs: a constant's, up to its highest 1.
    least: u32,
    /// Its width, to say it is extended from.
    own: u32,
    /// The bits its operators can carry into: `a + b` one more than its
    /// operands, `a * b` their sum. A constant written without a size takes
    /// any width.
    room: u32,
    /// The bits it adds to an operation's width: a constant only its
    /// value's, if sema knows it.
    grows: u32,
}

impl Range {
    /// An expression of `width` bits, all it may be taken at.
    fn of(width: u32) -> Range {
        Range {
            least: width,
            own: width,
            room: width,
            grows: width,
        }
    }

    /// A constant that needs `least` bits, and is `own` wide.
    fn constant(least: u32, own: u32) -> Range {
        let least = least.min(own);
        Range {
            least,
            own,
            room: u32::MAX,
            grows: least,
        }
    }
}

/// The width a source in `range` is taken to `target` from, if `target` is
/// outside it: its least, truncated, or its own, extended.
fn outside(range: Range, target: u32) -> Option<u32> {
    match () {
        _ if target < range.least => Some(range.least),
        _ if target > range.room => Some(range.own),
        _ => None,
    }
}

/// How many bits a constant needs: up to its highest 1, or for a negative
/// one its highest 0 and a sign bit.
fn needed(bits: &Bits) -> u32 {
    match bits.is_negative() {
        true => bits.not().with_signed(false).needed() + 1,
        false => bits.needed(),
    }
}
