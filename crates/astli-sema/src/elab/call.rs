//! Calls: the system functions a constant may call, and constant functions,
//! run statement by statement in a frame of their own.

use astli_syntax::SyntaxKind::{self, *};
use rustc_hash::FxHashMap;

use super::expr::{initial, truth};
use super::{Cx, Frame, Queries};
use crate::design::SymbolRef;
use crate::hir::*;
use crate::resolve::Resolution;
use crate::types::{Packed, Ty};
use crate::value::{Bit, Bits, Value};

/// How deep constant functions may call each other, recursion included.
const DEPTH: u32 = 64;

/// How many statements the outermost constant function call may take, those
/// of the functions it calls included. Enough to loop over every bit of a
/// wide vector many times over, and soon reached by a loop that never ends.
const STEPS: u32 = 1_000_000;

/// How a statement ends.
enum Flow {
    Next,
    Break,
    Continue,
    /// `return`, with its value converted to the function's type.
    Return(Option<Value>),
    /// What a constant function may not do, or sema cannot run: the call is
    /// unknown.
    Stop,
}

impl Queries {
    pub(crate) fn call_type(&self, cx: Cx, callee: ExprId, args: &[Arg]) -> Ty {
        let hir = &self.design[cx.file];
        match &hir[callee].kind {
            ExprKind::System(name) => self.system_type(cx, name, args),
            ExprKind::Name(_) | ExprKind::Scoped { .. } => match self.function(cx, callee) {
                Some(at) => self.type_in(cx, at),
                None => Ty::Unknown,
            },
            ExprKind::Member { base, name } if &*name.text == "size" => {
                match self.self_type(cx, *base) {
                    Ty::Dynamic(_) => Ty::INT,
                    _ => Ty::Unknown,
                }
            }
            _ => Ty::Unknown,
        }
    }

    pub(crate) fn call(&self, cx: Cx, callee: ExprId, args: &[Arg]) -> Value {
        let hir = &self.design[cx.file];
        match &hir[callee].kind {
            ExprKind::System(name) => self.system(cx, name, args),
            ExprKind::Name(_) | ExprKind::Scoped { .. } => match self.function(cx, callee) {
                Some(at) => self.run(cx, at, args),
                None => Value::Unknown,
            },
            // A dynamic array's or a queue's `size()`.
            ExprKind::Member { base, name } if &*name.text == "size" => {
                match self.dynamic_size(cx, *base) {
                    Some(size) => Value::Bits(Bits::from_u64(32, true, size as u64)),
                    None => Value::Unknown,
                }
            }
            _ => Value::Unknown,
        }
    }

    /// How many elements `expr` holds, if it is a dynamic array or a queue.
    fn dynamic_size(&self, cx: Cx, expr: ExprId) -> Option<usize> {
        match (self.self_type(cx, expr), self.eval_self(cx, expr)) {
            (Ty::Dynamic(_), Value::Array(values)) => Some(values.len()),
            _ => None,
        }
    }

    /// The function `callee` names.
    fn function(&self, cx: Cx, callee: ExprId) -> Option<SymbolRef> {
        let Some(Resolution::Declared(at)) = self.resolution(cx, callee) else {
            return None;
        };
        match self.design.symbol(at).kind {
            SymbolKind::Subroutine {
                keyword: FUNCTION_KW,
                ..
            } => Some(at),
            _ => None,
        }
    }

    // ------------------------------------------------------ system functions

    fn system_type(&self, cx: Cx, name: &str, args: &[Arg]) -> Ty {
        match name {
            "$clog2" | "$bits" | "$countones" | "$size" | "$left" | "$right" | "$low" | "$high"
            | "$increment" => Ty::INT,
            "$rtoi" => Ty::INTEGER,
            "$onehot" | "$onehot0" | "$isunknown" => Ty::BIT,
            "$signed" | "$unsigned" => match positional(args, 0).map(|it| self.self_type(cx, it)) {
                Some(Ty::Packed(own)) => Ty::Packed(Packed {
                    signed: name == "$signed",
                    ..own
                }),
                _ => Ty::Unknown,
            },
            "$itor" | "$bitstoreal" | "$ln" | "$log10" | "$exp" | "$sqrt" | "$pow" | "$floor"
            | "$ceil" => Ty::Real,
            "$realtobits" => Ty::Packed(Packed::vector(64, false, false)),
            _ => Ty::Unknown,
        }
    }

    fn system(&self, cx: Cx, name: &str, args: &[Arg]) -> Value {
        let int = |value: u64| Value::Bits(Bits::from_u64(32, true, value));
        let first = positional(args, 0);
        let value = || first.map_or(Value::Unknown, |arg| self.eval_self(cx, arg));
        let real = || match value() {
            Value::Real(real) => Some(real),
            Value::Bits(bits) => Some(bits.to_f64()),
            _ => None,
        };
        match name {
            "$clog2" => match value() {
                Value::Bits(bits) if bits.has_unknown() => {
                    Value::Bits(Bits::filled(32, true, Bit::X))
                }
                Value::Bits(bits) => {
                    let bits = bits.with_signed(false);
                    let one = Bits::from_u64(bits.width(), false, 1);
                    match bits.truth() == Bit::Zero || bits == one {
                        true => int(0),
                        false => int(u64::from(bits.sub(&one).needed())),
                    }
                }
                Value::Real(real) => {
                    int(Bits::from_f64(64, false, real).to_u64().map_or(0, |value| {
                        if value <= 1 {
                            0
                        } else {
                            u64::from(64 - (value - 1).leading_zeros())
                        }
                    }))
                }
                _ => Value::Unknown,
            },
            "$bits" => match first.and_then(|arg| self.type_or_expr(cx, arg).bits()) {
                Some(bits) if bits <= i32::MAX as u64 => int(bits),
                _ => Value::Unknown,
            },
            "$signed" | "$unsigned" => match value() {
                Value::Bits(bits) => Value::Bits(bits.with_signed(name == "$signed")),
                _ => Value::Unknown,
            },
            "$countones" => match value() {
                Value::Bits(bits) => int(bits.count_ones()),
                _ => Value::Unknown,
            },
            "$onehot" | "$onehot0" => match value() {
                Value::Bits(bits) if bits.has_unknown() => Value::Bits(Bits::bit_value(Bit::X)),
                Value::Bits(bits) => {
                    let ones = bits.count_ones();
                    let hot = ones == 1 || (name == "$onehot0" && ones == 0);
                    Value::Bits(Bits::bit_value(Bit::from(hot)))
                }
                _ => Value::Unknown,
            },
            "$isunknown" => match value() {
                Value::Bits(bits) => Value::Bits(Bits::bit_value(Bit::from(bits.has_unknown()))),
                _ => Value::Unknown,
            },
            "$size" | "$left" | "$right" | "$low" | "$high" | "$increment" => {
                // Of the dimension the second argument names, the first by
                // default.
                let dimension = match positional(args, 1) {
                    Some(dimension) => self
                        .integer(cx, dimension)
                        .and_then(|n| u32::try_from(n).ok()),
                    None => Some(1),
                };
                // A dynamic array or a queue counts up from 0, however
                // many elements it holds.
                let size = first.and_then(|arg| self.dynamic_size(cx, arg));
                if let (Some(1), Some(size)) = (dimension, size) {
                    let value = match name {
                        "$size" => size as i64,
                        "$left" | "$low" => 0,
                        "$right" | "$high" => size as i64 - 1,
                        _ => -1,
                    };
                    return Value::Bits(Bits::from_i64(32, true, value));
                }
                let range = (first.zip(dimension))
                    .and_then(|(arg, n)| self.type_or_expr(cx, arg).dimension(n));
                let Some((left, right)) = range else {
                    return Value::Unknown;
                };
                let value = match name {
                    "$size" => left.abs_diff(right) as i64 + 1,
                    "$left" => left,
                    "$right" => right,
                    "$low" => left.min(right),
                    "$high" => left.max(right),
                    _ => {
                        if left >= right {
                            1
                        } else {
                            -1
                        }
                    }
                };
                Value::Bits(Bits::from_i64(32, true, value))
            }
            "$rtoi" => match real() {
                Some(real) => Value::Bits(Bits::from_f64(32, true, real.trunc())),
                None => Value::Unknown,
            },
            "$itor" => real().map_or(Value::Unknown, Value::Real),
            "$realtobits" => match real() {
                Some(real) => Value::Bits(Bits::from_u64(64, false, real.to_bits())),
                None => Value::Unknown,
            },
            "$bitstoreal" => match value() {
                Value::Bits(bits) => match bits.resize(64).to_u64() {
                    Some(raw) => Value::Real(f64::from_bits(raw)),
                    None => Value::Unknown,
                },
                _ => Value::Unknown,
            },
            "$ln" | "$log10" | "$exp" | "$sqrt" | "$floor" | "$ceil" => match real() {
                Some(real) => Value::Real(match name {
                    "$ln" => real.ln(),
                    "$log10" => real.log10(),
                    "$exp" => real.exp(),
                    "$sqrt" => real.sqrt(),
                    "$floor" => real.floor(),
                    _ => real.ceil(),
                }),
                None => Value::Unknown,
            },
            "$pow" => {
                let exponent = positional(args, 1).map(|arg| self.eval_self(cx, arg));
                let exponent = match exponent {
                    Some(Value::Real(real)) => real,
                    Some(Value::Bits(bits)) => bits.to_f64(),
                    _ => return Value::Unknown,
                };
                real().map_or(Value::Unknown, |base| Value::Real(base.powf(exponent)))
            }
            _ => Value::Unknown,
        }
    }

    // ---------------------------------------------------- constant functions

    /// Runs the function at `at`, its arguments `args` evaluated in `cx`.
    fn run(&self, cx: Cx, at: SymbolRef, args: &[Arg]) -> Value {
        let hir = &self.design[at.file];
        let SymbolKind::Subroutine {
            scope, ports, body, ..
        } = &hir[at.symbol].kind
        else {
            return Value::Unknown;
        };
        let Some(owner) = self.owner(cx, at) else {
            return Value::Unknown;
        };
        if self.depth.get() >= DEPTH {
            return Value::Unknown;
        }
        let frame = Frame {
            function: at.symbol,
            locals: Default::default(),
        };
        let callee = Cx {
            body: owner,
            file: at.file,
            frame: Some(&frame),
        };

        let mut bound = FxHashMap::default();
        for (index, &port) in ports.iter().enumerate() {
            let SymbolKind::Port(declared) = &hir[port].kind else {
                return Value::Unknown;
            };
            // An argument a constant function writes back is not constant.
            if declared.direction.is_some_and(|it| it != INPUT_KW) {
                return Value::Unknown;
            }
            let name = &hir[port].name.text;
            let given = args.iter().enumerate().find_map(|(at, arg)| match arg {
                Arg::Positional(value) if at == index => Some(*value),
                Arg::Named { name: it, value } if it.text == *name => Some(*value),
                _ => None,
            });
            let ty = self.type_of(owner, port);
            let value = match given.flatten() {
                Some(value) => self.eval_assign(cx, value, &ty),
                None => match declared.default {
                    Some(default) => self.eval_assign(callee, default, &ty),
                    None => return Value::Unknown,
                },
            };
            bound.insert(port, value);
        }
        let returns = self.type_of(owner, at.symbol);
        bound.insert(at.symbol, initial(&returns));
        *frame.locals.borrow_mut() = bound;
        self.declare(callee, *scope);

        if self.depth.get() == 0 {
            self.steps.set(STEPS);
        }
        self.depth.set(self.depth.get() + 1);
        let flow = self.block(callee, body);
        self.depth.set(self.depth.get() - 1);
        let value = match flow {
            Flow::Return(Some(value)) => value,
            Flow::Next | Flow::Return(None) => frame.locals.borrow()[&at.symbol].clone(),
            Flow::Break | Flow::Continue | Flow::Stop => Value::Unknown,
        };
        self.convert(value, &returns)
    }

    /// Gives each variable `scope` declares its initial value.
    fn declare(&self, cx: Cx, scope: ScopeId) {
        let hir = &self.design[cx.file];
        let Some(frame) = cx.frame else {
            return;
        };
        for member in &hir[scope].members {
            let Member::Declare(symbol) = member else {
                continue;
            };
            let SymbolKind::Variable(data) = &hir[*symbol].kind else {
                continue;
            };
            let at = SymbolRef {
                file: cx.file,
                symbol: *symbol,
            };
            let ty = self.type_in(cx, at);
            let value = match data.init {
                Some(init) => self.eval_assign(cx, init, &ty),
                None => initial(&ty),
            };
            frame.locals.borrow_mut().insert(*symbol, value);
        }
    }

    /// Takes a step of the budget; `false` once it is spent.
    fn tick(&self) -> bool {
        let left = self.steps.get();
        self.steps.set(left.saturating_sub(1));
        left > 0
    }

    fn block(&self, cx: Cx, stmts: &[StmtId]) -> Flow {
        for &stmt in stmts {
            match self.exec(cx, stmt) {
                Flow::Next => {}
                flow => return flow,
            }
        }
        Flow::Next
    }

    fn exec_opt(&self, cx: Cx, stmt: Option<StmtId>) -> Flow {
        match stmt {
            Some(stmt) => self.exec(cx, stmt),
            None => Flow::Next,
        }
    }

    fn exec(&self, cx: Cx, stmt: StmtId) -> Flow {
        if !self.tick() {
            return Flow::Stop;
        }
        let hir = &self.design[cx.file];
        match &hir[stmt].kind {
            StmtKind::Empty => Flow::Next,
            StmtKind::Expr(expr) => match self.effect(cx, *expr) {
                true => Flow::Next,
                false => Flow::Stop,
            },
            StmtKind::Block {
                scope,
                join: None,
                stmts,
            } => {
                self.declare(cx, *scope);
                self.block(cx, stmts)
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => match truth(&self.eval_self(cx, *condition)) {
                Some(Bit::One) => self.exec_opt(cx, *then_branch),
                // An unknown condition takes the `else`.
                Some(_) => self.exec_opt(cx, *else_branch),
                None => Flow::Stop,
            },
            StmtKind::Case {
                keyword,
                inside,
                selector: Some(selector),
                items,
            } => self.case(cx, *keyword, *inside, *selector, items),
            StmtKind::For { header, body } => {
                self.declare(cx, header.scope);
                for &init in &header.init {
                    if !self.effect(cx, init) {
                        return Flow::Stop;
                    }
                }
                loop {
                    if let Some(condition) = header.condition {
                        match truth(&self.eval_self(cx, condition)) {
                            Some(Bit::One) => {}
                            Some(_) => return Flow::Next,
                            None => return Flow::Stop,
                        }
                    }
                    match self.iteration(cx, *body) {
                        Flow::Next | Flow::Continue => {}
                        Flow::Break => return Flow::Next,
                        flow => return flow,
                    }
                    for &step in &header.step {
                        if !self.effect(cx, step) {
                            return Flow::Stop;
                        }
                    }
                }
            }
            StmtKind::Loop {
                keyword,
                condition,
                body,
            } => self.repeat(cx, *keyword, *condition, *body),
            StmtKind::Return(value) => {
                let Some(frame) = cx.frame else {
                    return Flow::Stop;
                };
                let at = SymbolRef {
                    file: cx.file,
                    symbol: frame.function,
                };
                let returns = self.type_in(cx, at);
                Flow::Return(value.map(|value| self.eval_assign(cx, value, &returns)))
            }
            StmtKind::Break => Flow::Break,
            StmtKind::Continue => Flow::Continue,
            _ => Flow::Stop,
        }
    }

    /// One pass through a loop's body, which takes a step even if empty.
    fn iteration(&self, cx: Cx, body: Option<StmtId>) -> Flow {
        if !self.tick() {
            return Flow::Stop;
        }
        self.exec_opt(cx, body)
    }

    /// `while`, `do … while`, `repeat` or `forever`.
    fn repeat(
        &self,
        cx: Cx,
        keyword: SyntaxKind,
        condition: Option<ExprId>,
        body: Option<StmtId>,
    ) -> Flow {
        let mut count = match (keyword, condition) {
            (REPEAT_KW, Some(count)) => match self.eval_self(cx, count) {
                Value::Bits(bits) if bits.has_unknown() => Some(0),
                Value::Bits(bits) => match bits.is_negative() {
                    true => Some(0),
                    false => Some(bits.to_u64().unwrap_or(u64::MAX)),
                },
                _ => return Flow::Stop,
            },
            _ => None,
        };
        let holds = |cx: Cx| match condition {
            Some(condition) => truth(&self.eval_self(cx, condition)),
            None => Some(Bit::One),
        };
        let mut first = keyword == DO_KW;
        loop {
            match &mut count {
                Some(0) => return Flow::Next,
                Some(count) => *count -= 1,
                None if first => first = false,
                None => match holds(cx) {
                    Some(Bit::One) => {}
                    Some(_) => return Flow::Next,
                    None => return Flow::Stop,
                },
            }
            match self.iteration(cx, body) {
                Flow::Next | Flow::Continue => {}
                Flow::Break => return Flow::Next,
                flow => return flow,
            }
        }
    }

    fn case(
        &self,
        cx: Cx,
        keyword: SyntaxKind,
        inside: bool,
        selector: ExprId,
        items: &[CaseItem],
    ) -> Flow {
        if keyword == RANDCASE_KW {
            return Flow::Stop;
        }
        // The selector and every label are sized to the widest of them.
        let labels: Vec<ExprId> = items
            .iter()
            .flat_map(|item| &item.labels)
            .copied()
            .collect();
        let ty = self.set_type(cx, selector, &labels);
        let selector = self.eval_at(cx, selector, &ty);
        let mut default = None;
        for item in items {
            if item.labels.is_empty() {
                default = Some(item.body);
                continue;
            }
            for &label in &item.labels {
                if inside {
                    match self.in_item(cx, &selector, label, &ty) {
                        Some(Bit::One) => return self.exec_opt(cx, item.body),
                        Some(_) => continue,
                        None => return Flow::Stop,
                    }
                }
                let matched = match (&selector, self.eval_at(cx, label, &ty)) {
                    (Value::Bits(selector), Value::Bits(label)) => match keyword {
                        CASEZ_KW => selector.casez_eq(&label),
                        CASEX_KW => selector.casex_eq(&label),
                        _ => selector.case_eq(&label),
                    },
                    (Value::Real(selector), Value::Real(label)) => *selector == label,
                    (Value::String(selector), Value::String(label)) => *selector == label,
                    _ => return Flow::Stop,
                };
                if matched {
                    return self.exec_opt(cx, item.body);
                }
            }
        }
        match default {
            Some(body) => self.exec_opt(cx, body),
            None => Flow::Next,
        }
    }

    /// Runs `expr`, a statement, for its effect; `false` if a constant
    /// function may not.
    fn effect(&self, cx: Cx, expr: ExprId) -> bool {
        let hir = &self.design[cx.file];
        match &hir[expr].kind {
            ExprKind::Assign { op, lhs, rhs, .. } => self.assign(cx, *op, *lhs, *rhs).is_some(),
            ExprKind::Unary {
                op: PLUS_PLUS | MINUS_MINUS,
                operand,
            }
            | ExprKind::Postfix { operand, .. } => {
                self.place(cx, *operand).is_some() && {
                    self.eval_self(cx, expr);
                    true
                }
            }
            ExprKind::Call { callee, args } => match &hir[*callee].kind {
                // Messages change nothing; a fatal one ends elaboration.
                ExprKind::System(name) => {
                    matches!(&**name, "$display" | "$write" | "$info" | "$warning")
                }
                _ => {
                    self.call(cx, *callee, args);
                    true
                }
            },
            _ => false,
        }
    }
}

/// The `index`th positional argument.
fn positional(args: &[Arg], index: usize) -> Option<ExprId> {
    match args.get(index)? {
        Arg::Positional(value) => *value,
        _ => None,
    }
}
