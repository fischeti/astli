//! Widths that differ where the standard extends or truncates without a
//! word: an assignment's sides, a port and its connection, a comparison's
//! operands, in what the tops build.

use astli_sema::{
    Arg, Bits, BodyId, Bound, Built, Conversion, ConversionKind, Elaboration, ExprId, ExprKind,
    FileId, Frame, Literal, Operator, Parameter, Resolution, ScopeKind, SymbolId, SymbolKind,
    SymbolRef, Ty, TypeKind, Value, Walked, Within,
};
use astli_syntax::SyntaxKind::*;
use astli_text::Span;
use rayon::prelude::*;
use rustc_hash::FxHashSet;

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
    /// An argument to the input `formal` of the subroutine `subroutine`.
    Argument {
        formal: Box<str>,
        subroutine: Box<str>,
    },
    /// An override of the parameter `parameter` of an instance of
    /// `definition`.
    Override {
        parameter: Box<str>,
        definition: Box<str>,
    },
}

impl Width {
    /// Whether the source loses bits, rather than gains them.
    pub fn truncates(&self) -> bool {
        self.source_width > self.target_width
    }
}

/// Each place in what `built` builds where two widths differ, once however
/// many bodies build it: an assignment, a port connection, a comparison. A
/// constant needs only the bits of its value and takes any wider width but
/// for a literal written with a size, a sum may take its carry, and what
/// sema cannot size is silent.
pub fn widths(elaboration: &Elaboration, built: &Built) -> Vec<Width> {
    // Each frame is sized on its own, so they are walked apart.
    let walked: Vec<Vec<Width>> = (built.frames().par_iter())
        .map(|frame| {
            let walked = elaboration.walk(frame.body);
            let mut walk = Walk {
                elaboration,
                walked: &walked,
                found: Vec::new(),
            };
            elaboration.conversions(frame, &walked, |conversion| {
                walk.conversion(frame, conversion)
            });
            walk.found
        })
        .collect();
    let mut seen = FxHashSet::default();
    let mut found = Vec::new();
    for width in walked.into_iter().flatten() {
        let kind = match width.kind {
            WidthKind::Assign => 0,
            WidthKind::Connection { .. } => 1,
            WidthKind::Compare => 2,
            WidthKind::Override { .. } => 3,
            WidthKind::Argument { .. } => 4,
        };
        if seen.insert((width.file, width.source, kind)) {
            found.push(width);
        }
    }
    found
}

struct Walk<'a> {
    elaboration: &'a Elaboration,
    walked: &'a Walked<'a>,
    found: Vec<Width>,
}

impl Walk<'_> {
    /// The widths of `conversion` in `frame`, if they differ. What a
    /// subroutine or a class holds is not sized, nor is a return, nor what a
    /// branch a constant condition rules out holds, as the widths it is
    /// written for are other parameters'.
    fn conversion(&mut self, frame: &Frame, conversion: Conversion) {
        if conversion.within != Within::Body || conversion.unevaluated {
            return;
        }
        let hir = &self.elaboration.design()[frame.file];
        match conversion.kind {
            ConversionKind::Assign { op, lhs, rhs } => {
                let target = self.walked.expr_type(lhs);
                match op {
                    EQ | LT_EQ => self.assignment(frame, hir[lhs].span, &target, rhs, true),
                    // A shift's amount is no operand of its width.
                    LT_LT_EQ | LT_LT_LT_EQ | GT_GT_EQ | GT_GT_GT_EQ => {}
                    // `a += b` computes at the wider of the two, so only a
                    // `b` wider than `a` loses bits, and nothing extends.
                    _ => self.assignment(frame, hir[lhs].span, &target, rhs, false),
                }
            }
            ConversionKind::Init { symbol, value } => {
                let at = SymbolRef {
                    file: frame.file,
                    symbol,
                };
                let target = self.elaboration.symbol_type(frame.body, at);
                self.assignment(frame, hir[symbol].name.span, &target, value, true);
            }
            ConversionKind::Default { parameter, value } => {
                let at = SymbolRef {
                    file: frame.file,
                    symbol: parameter,
                };
                let target = self.elaboration.symbol_type(frame.body, at);
                self.assignment(frame, hir[parameter].name.span, &target, value, true);
            }
            ConversionKind::Override {
                definition,
                body,
                parameter,
                value,
                ..
            } => {
                let design = self.elaboration.design();
                let kind = WidthKind::Override {
                    parameter: design[definition.file][parameter].name.text.clone(),
                    definition: design.symbol(definition).name.text.clone(),
                };
                let target = self.elaboration.type_of(body, parameter);
                self.given(frame, kind, hir[value].span, &target, value, true);
            }
            ConversionKind::Argument {
                subroutine,
                formal,
                value,
            } => {
                let design = self.elaboration.design();
                let kind = WidthKind::Argument {
                    formal: design[subroutine.file][formal].name.text.clone(),
                    subroutine: design.symbol(subroutine).name.text.clone(),
                };
                let at = SymbolRef {
                    file: subroutine.file,
                    symbol: formal,
                };
                let target = self.elaboration.symbol_type(frame.body, at);
                self.given(frame, kind, hir[value].span, &target, value, true);
            }
            ConversionKind::Connection {
                definition,
                body,
                port,
                output,
                connection,
                ..
            } => self.connection(frame, definition, body, port, output, connection),
            ConversionKind::Compare { lhs, rhs, .. } => self.comparison(frame, lhs, rhs),
            _ => {}
        }
    }

    fn report(&mut self, width: Width) {
        self.found.push(width);
    }

    /// `target = value`, the target of type `target`; truncated alone
    /// unless `extends`.
    fn assignment(&mut self, frame: &Frame, at: Span, target: &Ty, value: ExprId, extends: bool) {
        self.given(frame, WidthKind::Assign, at, target, value, extends);
    }

    /// `value` given to what `at` writes, of type `target`, as `kind`;
    /// truncated alone unless `extends`.
    fn given(
        &mut self,
        frame: &Frame,
        kind: WidthKind,
        at: Span,
        target: &Ty,
        value: ExprId,
        extends: bool,
    ) {
        let (Some(target_width), Some(range)) = (target.width(), self.range(frame, value)) else {
            return;
        };
        let Some(source_width) = outside(range, target_width) else {
            return;
        };
        if !extends && source_width <= target_width {
            return;
        }
        let hir = &self.elaboration.design()[frame.file];
        self.report(Width {
            file: frame.file,
            kind,
            source: hir[value].span,
            target: at,
            source_width,
            target_width,
            path: frame.path.clone(),
        });
    }

    /// The widths `expr` may be taken at without a word. `None` if it is
    /// not packed.
    fn range(&self, frame: &Frame, expr: ExprId) -> Option<Range> {
        let elaboration = self.elaboration;
        let hir = &elaboration.design()[frame.file];
        let own = self.walked.expr_type(expr).width()?;
        let ExprKind::Literal { value, size, .. } = &hir[expr].kind else {
            return Some(match self.walked.eval(expr) {
                // A time counts time units, and what it scales is meant to
                // reach all of its 64 bits.
                Value::Bits(bits) if !bits.has_unknown() && self.time(frame, expr) => Range {
                    grows: own,
                    ..Range::constant(needed(&bits), own)
                },
                Value::Bits(bits) if !bits.has_unknown() => Range::constant(needed(&bits), own),
                // A constant sema cannot evaluate may need any of its bits.
                Value::Unknown if self.constant(frame, expr) => Range {
                    least: 1,
                    own,
                    room: u32::MAX,
                    grows: u32::MAX,
                },
                _ => self.operation(frame, expr, own),
            });
        };
        Some(match &**value {
            Literal::Fill(_) => Range::constant(1, own),
            // The width written is the one it has.
            Literal::Bits(bits) if size.is_some() => Range {
                room: own,
                ..Range::constant(needed(bits), own)
            },
            Literal::Bits(bits) => Range::constant(needed(bits), own),
            _ => Range::constant(own, own),
        })
    }

    /// The range of `expr`, not a constant, of width `own` by itself.
    /// Constants in it count only the bits of their values, as they would
    /// by themselves: `a + 1` is as wide as `a`.
    fn operation(&self, frame: &Frame, expr: ExprId, own: u32) -> Range {
        let hir = &self.elaboration.design()[frame.file];
        let range = |expr: ExprId| self.range(frame, expr).unwrap_or(Range::of(own));
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
                // It is as wide as its wider arm. An arm written with a size
                // has that size, however few bits its value needs; one
                // without, a fill such as `'0` among them, takes the other's
                // and adds only its value's bits.
                let sized = |arm: Range| match arm.room {
                    u32::MAX => (arm.grows, arm.grows),
                    room => (arm.own, room),
                };
                let ((then_own, then_room), (else_own, else_room)) =
                    (sized(then_value), sized(else_value));
                let own = then_own.max(else_own).max(least);
                let room = then_room.max(else_room).max(own);
                return Range {
                    least,
                    own,
                    room,
                    grows: room,
                };
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

    /// Whether `expr` names a parameter declared `time`.
    fn time(&self, frame: &Frame, expr: ExprId) -> bool {
        let elaboration = self.elaboration;
        let Some(Resolution::Declared(at)) = elaboration.names(frame.file).expr(expr) else {
            return false;
        };
        matches!(
            &elaboration.design().symbol(at).kind,
            SymbolKind::Parameter(Parameter { ty, .. })
                if matches!(ty.kind, TypeKind::Builtin { keyword: TIME_KW, .. })
        )
    }

    /// Whether `expr` is a constant: of literals, parameters, enum members
    /// and genvars, and names sema cannot see, which may be any of those. A
    /// loop's variable counts, as a loop with constant bounds is unrolled
    /// into one constant per iteration.
    fn constant(&self, frame: &Frame, expr: ExprId) -> bool {
        let elaboration = self.elaboration;
        let hir = &elaboration.design()[frame.file];
        let all = |exprs: &[ExprId]| exprs.iter().all(|&expr| self.constant(frame, expr));
        match &hir[expr].kind {
            ExprKind::Literal { .. } | ExprKind::Time { .. } => true,
            ExprKind::Name(_) | ExprKind::Scoped { .. } => {
                match elaboration.names(frame.file).expr(expr) {
                    Some(Resolution::Declared(at)) => match elaboration.design().symbol(at).kind {
                        SymbolKind::Parameter(Parameter { is_type: false, .. })
                        | SymbolKind::EnumMember { .. }
                        | SymbolKind::Genvar(_) => true,
                        SymbolKind::Variable(_) => {
                            let home = elaboration.design().symbol(at).scope;
                            elaboration.design()[at.file][home].kind == ScopeKind::Loop
                        }
                        _ => false,
                    },
                    Some(Resolution::Unknown) => true,
                    _ => false,
                }
            }
            ExprKind::Unary { operand, .. } | ExprKind::Cast { operand, .. } => {
                self.constant(frame, *operand)
            }
            ExprKind::Binary { lhs, rhs, .. } => all(&[*lhs, *rhs]),
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
                ..
            } => all(&[*condition, *then_value, *else_value]),
            ExprKind::Concat(parts) => all(parts),
            ExprKind::Replication { count, concat } => all(&[*count, *concat]),
            ExprKind::Select { base, index, range } => {
                all(&[*base, *index]) && range.is_none_or(|(_, end)| self.constant(frame, end))
            }
            ExprKind::Member { base, .. } => self.constant(frame, *base),
            ExprKind::Call { callee, args } => {
                matches!(hir[*callee].kind, ExprKind::System(_))
                    && args.iter().all(|arg| match arg {
                        Arg::Positional(Some(arg)) => self.constant(frame, *arg),
                        _ => false,
                    })
            }
            _ => false,
        }
    }

    fn comparison(&mut self, frame: &Frame, lhs: ExprId, rhs: ExprId) {
        let (Some(left), Some(right)) = (self.range(frame, lhs), self.range(frame, rhs)) else {
            return;
        };
        // The operands agree if some width is in both ranges.
        let (source, target, narrow, wide) = match () {
            _ if left.room < right.least => (lhs, rhs, left.own, right.least),
            _ if right.room < left.least => (rhs, lhs, right.own, left.least),
            _ => return,
        };
        let hir = &self.elaboration.design()[frame.file];
        self.report(Width {
            file: frame.file,
            kind: WidthKind::Compare,
            source: hir[source].span,
            target: hir[target].span,
            source_width: narrow,
            target_width: wide,
            path: frame.path.clone(),
        });
    }

    /// A connection whose width differs from its port's, the port as the
    /// body it is built with sizes it.
    fn connection(
        &mut self,
        frame: &Frame,
        definition: SymbolRef,
        body: BodyId,
        port: SymbolId,
        output: bool,
        connection: Bound,
    ) {
        let elaboration = self.elaboration;
        let design = elaboration.design();
        let Some(port_width) = elaboration.type_of(body, port).width() else {
            return;
        };
        // A name, `.name` or by `.*`, is resolved where the instance
        // stands.
        let (range, at) = match connection {
            Bound::Expr(expr) => (self.range(frame, expr), design[frame.file][expr].span),
            Bound::Name(Resolution::Declared(found), span) => {
                let width = elaboration.symbol_type(frame.body, found).width();
                (width.map(Range::of), span)
            }
            Bound::Name(..) => return,
        };
        let Some(range) = range else {
            return;
        };
        // An input takes its connection's value, and an output gives the
        // port's to it.
        let (source_width, target_width) = match output {
            false => match outside(range, port_width) {
                Some(source_width) => (source_width, port_width),
                None => return,
            },
            true if range.own != port_width => (port_width, range.own),
            true => return,
        };
        self.report(Width {
            file: frame.file,
            kind: WidthKind::Connection {
                port: design[definition.file][port].name.text.clone(),
                definition: design.symbol(definition).name.text.clone(),
                output,
            },
            source: at,
            target: at,
            source_width,
            target_width,
            path: frame.path.clone(),
        });
    }
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
