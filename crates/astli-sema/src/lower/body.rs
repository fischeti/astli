//! Lowering statements, timing controls and expressions.

use astli_syntax::SyntaxKind::*;
use astli_syntax::ast::{self, AstNode};
use astli_syntax::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

use super::{Lower, block_label};
use crate::hir::*;

impl Lower<'_> {
    /// Lowers `item` standing in `scope` among statements. A declaration is
    /// declared in `scope` instead, and gives `None`.
    pub(super) fn body(&mut self, scope: ScopeId, item: ast::Item) -> Option<StmtId> {
        if self.declaration(scope, &item) {
            return None;
        }
        let span = self.span(item.syntax());
        let kind = match &item {
            ast::Item::Block(block) => return Some(self.block(scope, block, None)),
            ast::Item::LabeledStmt(stmt) => match (stmt.label(), stmt.item()) {
                (Some(label), Some(ast::Item::Block(block))) => {
                    return Some(self.block(scope, &block, Some(label)));
                }
                (label, Some(inner)) => {
                    let inner = self.body(scope, inner);
                    if let (Some(label), Some(inner)) = (label, inner) {
                        let name = self.name(&label);
                        self.declare(scope, name, SymbolKind::Label(inner));
                    }
                    return inner;
                }
                (_, None) => StmtKind::Opaque(self.opaque(stmt.syntax(), None)),
            },
            ast::Item::ExprStmt(stmt) => match (stmt.assignment(), stmt.expr()) {
                (Some(assignment), _) => StmtKind::Expr(self.assignment(scope, &assignment, None)),
                (None, Some(expr)) => StmtKind::Expr(self.expr_of(scope, expr)),
                (None, None) => StmtKind::Empty,
            },
            ast::Item::IfStmt(stmt) => StmtKind::If {
                condition: self.condition(scope, stmt.condition(), stmt.syntax()),
                then_branch: stmt.then_branch().and_then(|item| self.body(scope, item)),
                else_branch: stmt.else_branch().and_then(|item| self.body(scope, item)),
            },
            ast::Item::CaseStmt(stmt) => {
                let keyword = stmt.keyword().map_or(CASE_KW, |keyword| keyword.kind());
                let selector = stmt
                    .paren_expr()
                    .map(|selector| self.paren(scope, &selector));
                let mut items = Vec::new();
                for item in stmt.case_items() {
                    let labels = self.set(scope, item.syntax());
                    let body = item.item().and_then(|body| self.body(scope, body));
                    items.push(CaseItem { labels, body });
                }
                for verbatim in stmt.verbatims() {
                    let label = self.opaque_expr(verbatim.syntax());
                    items.push(CaseItem {
                        labels: vec![label],
                        body: None,
                    });
                }
                StmtKind::Case {
                    keyword,
                    inside: stmt.inside_token().is_some(),
                    selector,
                    items,
                }
            }
            ast::Item::ForStmt(stmt) => match stmt.header() {
                Some(header) => {
                    let header = self.for_header(scope, &header);
                    let body = stmt.body().and_then(|body| self.body(header.scope, body));
                    StmtKind::For { header, body }
                }
                None => StmtKind::Opaque(self.opaque(stmt.syntax(), None)),
            },
            ast::Item::ForeachStmt(stmt) => {
                match stmt.header().and_then(|it| Some((it.array()?, it))) {
                    Some((array, header)) => {
                        let loop_scope = self.scope(ScopeKind::Loop, Some(scope), None);
                        let array = self.expr_of(scope, array);
                        let variables = (header.syntax().children_with_tokens())
                            .filter_map(SyntaxElement::into_token)
                            .filter(|token| matches!(token.kind(), IDENT | ESCAPED_IDENT));
                        for variable in variables {
                            let name = self.name(&variable);
                            let data = Data {
                                keyword: None,
                                ty: Type::implicit(),
                                delay: None,
                                init: None,
                            };
                            self.declare(loop_scope, name, SymbolKind::Variable(data));
                        }
                        let body = stmt.body().and_then(|body| self.body(loop_scope, body));
                        StmtKind::Foreach {
                            scope: loop_scope,
                            array,
                            body,
                        }
                    }
                    None => StmtKind::Opaque(self.opaque(stmt.syntax(), None)),
                }
            }
            ast::Item::WhileStmt(stmt) => StmtKind::Loop {
                keyword: WHILE_KW,
                condition: Some(self.condition(scope, stmt.condition(), stmt.syntax())),
                body: stmt.body().and_then(|body| self.body(scope, body)),
            },
            ast::Item::DoWhileStmt(stmt) => StmtKind::Loop {
                keyword: DO_KW,
                condition: Some(self.condition(scope, stmt.condition(), stmt.syntax())),
                body: stmt.body().and_then(|body| self.body(scope, body)),
            },
            ast::Item::RepeatStmt(stmt) => StmtKind::Loop {
                keyword: REPEAT_KW,
                condition: Some(self.condition(scope, stmt.count(), stmt.syntax())),
                body: stmt.body().and_then(|body| self.body(scope, body)),
            },
            ast::Item::ForeverStmt(stmt) => StmtKind::Loop {
                keyword: FOREVER_KW,
                condition: None,
                body: stmt.body().and_then(|body| self.body(scope, body)),
            },
            ast::Item::ReturnStmt(stmt) => {
                StmtKind::Return(stmt.expr().map(|value| self.expr_of(scope, value)))
            }
            ast::Item::BreakStmt(_) => StmtKind::Break,
            ast::Item::ContinueStmt(_) => StmtKind::Continue,
            ast::Item::DisableStmt(stmt) => match (stmt.fork_token(), stmt.expr()) {
                (None, Some(target)) => StmtKind::Disable(Some(self.expr_of(scope, target))),
                _ => StmtKind::Disable(None),
            },
            ast::Item::WaitStmt(stmt) => match stmt.fork_token() {
                Some(_) => StmtKind::Wait {
                    condition: None,
                    body: None,
                },
                None => StmtKind::Wait {
                    condition: Some(self.condition(scope, stmt.paren_expr(), stmt.syntax())),
                    body: stmt.item().and_then(|body| self.body(scope, body)),
                },
            },
            ast::Item::EventTrigger(stmt) => match stmt.expr() {
                Some(event) => StmtKind::Trigger(self.expr_of(scope, event)),
                None => StmtKind::Opaque(self.opaque(stmt.syntax(), None)),
            },
            ast::Item::TimingStmt(stmt) => {
                let timing = self.timing(scope, stmt.delay_control(), stmt.event_control(), None);
                match timing {
                    Some(timing) => StmtKind::Timed {
                        timing,
                        body: stmt.item().and_then(|body| self.body(scope, body)),
                    },
                    None => StmtKind::Opaque(self.opaque(stmt.syntax(), None)),
                }
            }
            ast::Item::ProceduralAssign(stmt) => {
                let expr = match (stmt.assignment(), stmt.expr()) {
                    (Some(assignment), _) => Some(self.assignment(scope, &assignment, None)),
                    (None, Some(expr)) => Some(self.expr_of(scope, expr)),
                    (None, None) => None,
                };
                match (stmt.keyword(), expr) {
                    (Some(keyword), Some(expr)) => StmtKind::ProceduralAssign {
                        keyword: keyword.kind(),
                        expr,
                    },
                    _ => StmtKind::Opaque(self.opaque(stmt.syntax(), None)),
                }
            }
            ast::Item::ConstraintBlock(block) => return Some(self.constraints(scope, block)),
            // `soft` changes how hard a constraint holds, not what it reads;
            // `unique {…}` takes a list, which is not lowered.
            ast::Item::ConstraintExpr(constraint) => {
                match (constraint.expr(), constraint.range_list()) {
                    (Some(expr), _) => StmtKind::Expr(self.expr_of(scope, expr)),
                    (None, None) => StmtKind::Empty,
                    (None, Some(_)) => StmtKind::Opaque(self.opaque(constraint.syntax(), None)),
                }
            }
            ast::Item::Implication(implication) => match implication.condition() {
                Some(condition) => StmtKind::If {
                    condition: self.expr_of(scope, condition),
                    then_branch: implication.item().and_then(|item| self.body(scope, item)),
                    else_branch: None,
                },
                None => StmtKind::Opaque(self.opaque(implication.syntax(), None)),
            },
            ast::Item::ImmediateAssertion(stmt) => StmtKind::Assertion {
                keyword: stmt.keyword().map_or(ASSERT_KW, |keyword| keyword.kind()),
                condition: self.condition(scope, stmt.condition(), stmt.syntax()),
                pass: stmt.pass().and_then(|item| self.body(scope, item)),
                fail: stmt.fail().and_then(|item| self.body(scope, item)),
            },
            ast::Item::ConcurrentAssertion(stmt) => match self.concurrent(scope, stmt) {
                Some(kind) => kind,
                None => StmtKind::Opaque(self.opaque(stmt.syntax(), None)),
            },
            item => StmtKind::Opaque(self.opaque(item.syntax(), None)),
        };
        Some(self.stmt(kind, span))
    }

    /// A procedural block, its name declared in `scope`: `label`, or the one
    /// after its `begin` or `fork`.
    fn block(&mut self, scope: ScopeId, block: &ast::Block, label: Option<SyntaxToken>) -> StmtId {
        let span = self.span(block.syntax());
        let inner = self.scope(ScopeKind::Block, Some(scope), None);
        if let Some(label) = label.or_else(|| block_label(block)) {
            let name = self.name(&label);
            let symbol = self.declare(scope, name, SymbolKind::Block(inner));
            self.hir.scopes[inner.index()].owner = Some(symbol);
        }
        let stmts = block
            .items()
            .filter_map(|item| self.body(inner, item))
            .collect();
        let join = match block.open() {
            Some(open) if open.kind() == FORK_KW => block.close().map(|close| close.kind()),
            _ => None,
        };
        let kind = StmtKind::Block {
            scope: inner,
            join,
            stmts,
        };
        self.stmt(kind, span)
    }

    /// `{…}`, a block of constraints.
    pub(super) fn constraints(&mut self, scope: ScopeId, block: &ast::ConstraintBlock) -> StmtId {
        let span = self.span(block.syntax());
        let inner = self.scope(ScopeKind::Block, Some(scope), None);
        let stmts = block
            .items()
            .filter_map(|item| self.body(inner, item))
            .collect();
        let kind = StmtKind::Block {
            scope: inner,
            join: None,
            stmts,
        };
        self.stmt(kind, span)
    }

    /// A `for` header, its declarations in a scope of its own.
    pub(super) fn for_header(&mut self, scope: ScopeId, header: &ast::ParenExpr) -> For {
        let loop_scope = self.scope(ScopeKind::Loop, Some(scope), None);
        let mut lowered = For {
            scope: loop_scope,
            init: Vec::new(),
            condition: None,
            step: Vec::new(),
        };
        let mut clause = 0;
        for element in header.syntax().children_with_tokens() {
            let node = match element {
                SyntaxElement::Token(token) => {
                    clause += usize::from(token.kind() == SEMICOLON);
                    continue;
                }
                SyntaxElement::Node(node) => node,
            };
            if let Some(decl) = ast::VarDecl::cast(node.clone()) {
                self.var_decl(loop_scope, &decl);
                continue;
            }
            let expr = if let Some(assignment) = ast::Assignment::cast(node.clone()) {
                self.assignment(loop_scope, &assignment, None)
            } else if let Some(expr) = ast::Expr::cast(node.clone()) {
                self.expr_of(loop_scope, expr)
            } else {
                self.opaque_expr(&node)
            };
            match clause {
                0 => lowered.init.push(expr),
                1 => lowered.condition = Some(expr),
                _ => lowered.step.push(expr),
            }
        }
        lowered
    }

    /// The values and ranges `node` lists, an `inside` set or a case item's
    /// labels: each expression among its children, and each `[lo:hi]` a
    /// [`Range`](ExprKind::Range). A range missing a bound is its bounds.
    fn set(&mut self, scope: ScopeId, node: &SyntaxNode) -> Vec<ExprId> {
        let mut set = Vec::new();
        // The `[` of a range open, its operator and the bounds so far.
        let mut range: Option<(SyntaxToken, SyntaxKind, Vec<ExprId>)> = None;
        for element in node.children_with_tokens() {
            match element {
                SyntaxElement::Token(token) => match (token.kind(), &mut range) {
                    (L_BRACK, _) => range = Some((token, COLON, Vec::new())),
                    (COLON | PLUS_SLASH_MINUS | PLUS_PERCENT_MINUS, Some((_, op, _))) => {
                        *op = token.kind();
                    }
                    (R_BRACK, Some(_)) => {
                        let (open, op, bounds) = range.take().unwrap();
                        match bounds[..] {
                            [lo, hi] => {
                                let span = self.token_span(&open).cover(self.token_span(&token));
                                let span = span.unwrap_or(self.token_span(&open));
                                set.push(self.expr(ExprKind::Range { op, lo, hi }, span));
                            }
                            _ => set.extend(bounds),
                        }
                    }
                    _ => {}
                },
                SyntaxElement::Node(node) => {
                    if let Some(expr) = ast::Expr::cast(node) {
                        let expr = self.expr_of(scope, expr);
                        match &mut range {
                            Some((_, _, bounds)) => bounds.push(expr),
                            None => set.push(expr),
                        }
                    }
                }
            }
        }
        set.extend(range.into_iter().flat_map(|(_, _, bounds)| bounds));
        set
    }

    /// `lhs op rhs`, the timing written in it, or else `timing`.
    pub(super) fn assignment(
        &mut self,
        scope: ScopeId,
        assignment: &ast::Assignment,
        timing: Option<Timing>,
    ) -> ExprId {
        let (Some(lhs), Some(op), Some(rhs)) =
            (assignment.lhs(), assignment.op(), assignment.rhs())
        else {
            return self.opaque_expr(assignment.syntax());
        };
        let span = self.span(assignment.syntax());
        let own = self.timing(
            scope,
            assignment.delay_control(),
            assignment.event_control(),
            assignment.repeat_control(),
        );
        let kind = ExprKind::Assign {
            op: op.kind(),
            lhs: self.expr_of(scope, lhs),
            rhs: self.expr_of(scope, rhs),
            timing: own.or(timing),
        };
        self.expr(kind, span)
    }

    /// A concurrent assertion, or `None` if it has no property.
    pub(super) fn concurrent(
        &mut self,
        scope: ScopeId,
        assertion: &ast::ConcurrentAssertion,
    ) -> Option<StmtKind> {
        let (keyword, spec) = (assertion.keyword()?, assertion.property_spec()?);
        Some(StmtKind::ConcurrentAssertion {
            keyword: keyword.kind(),
            spec: self.property_spec(scope, &spec),
            pass: assertion.pass().and_then(|item| self.body(scope, item)),
            fail: assertion.fail().and_then(|item| self.body(scope, item)),
        })
    }

    // ------------------------------------------------------------ properties

    pub(super) fn property_spec(
        &mut self,
        scope: ScopeId,
        spec: &ast::PropertySpec,
    ) -> PropertySpec {
        PropertySpec {
            clock: spec.event_control().map(|clock| self.event(scope, &clock)),
            disable: spec
                .disable()
                .map(|condition| self.paren(scope, &condition)),
            property: self.property_of(scope, spec.property(), spec.syntax()),
        }
    }

    /// `property`, or an empty opaque expression over `around` where the
    /// tree has none.
    fn property_of(
        &mut self,
        scope: ScopeId,
        property: Option<ast::PropExpr>,
        around: &SyntaxNode,
    ) -> ExprId {
        match property {
            Some(property) => self.property(scope, property),
            None => {
                let span = self.span(around);
                self.expr(ExprKind::Opaque(Opaque::default()), span)
            }
        }
    }

    pub(super) fn property(&mut self, scope: ScopeId, property: ast::PropExpr) -> ExprId {
        let span = self.span(property.syntax());
        let kind = match &property {
            ast::PropExpr::Expr(expr) => return self.expr_of(scope, expr.clone()),
            ast::PropExpr::PropertyBinExpr(binary) => match binary.op() {
                Some(op) => Property::Binary {
                    op: op.kind(),
                    lhs: self.property_of(scope, binary.lhs(), binary.syntax()),
                    rhs: self.property_of(scope, binary.rhs(), binary.syntax()),
                },
                None => return self.opaque_expr(binary.syntax()),
            },
            ast::PropExpr::PropertyUnaryExpr(unary) => match unary.op() {
                Some(op) => Property::Unary {
                    op: op.kind(),
                    bounds: self.bounds(scope, unary.bounds()),
                    condition: unary.condition().map(|it| self.paren(scope, &it)),
                    operand: self.property_of(scope, unary.operand(), unary.syntax()),
                },
                None => return self.opaque_expr(unary.syntax()),
            },
            ast::PropExpr::PropertyParen(paren) => {
                let sequence = self.property_of(scope, paren.sequence(), paren.syntax());
                let mut items = Vec::new();
                for item in paren.items() {
                    let item = if let Some(assignment) = ast::Assignment::cast(item.clone()) {
                        self.assignment(scope, &assignment, None)
                    } else if let Some(expr) = ast::Expr::cast(item.clone()) {
                        self.expr_of(scope, expr)
                    } else {
                        self.opaque_expr(&item)
                    };
                    items.push(item);
                }
                // `(s)` is the sequence it holds.
                if items.is_empty() {
                    return sequence;
                }
                Property::Match { sequence, items }
            }
            ast::PropExpr::PropertyIf(choice) => Property::If {
                condition: self.condition(scope, choice.condition(), choice.syntax()),
                then_branch: self.property_of(scope, choice.then_branch(), choice.syntax()),
                else_branch: choice.else_branch().map(|it| self.property(scope, it)),
            },
            ast::PropExpr::ClockedProperty(clocked) => match clocked.event_control() {
                Some(clock) => Property::Clocked {
                    clock: self.event(scope, &clock),
                    property: self.property_of(scope, clocked.prop_expr(), clocked.syntax()),
                },
                None => return self.opaque_expr(clocked.syntax()),
            },
            ast::PropExpr::SequenceDelay(delay) => match delay.cycle_delay() {
                Some(cycles) => Property::Delay {
                    lhs: delay.lhs().map(|lhs| self.property(scope, lhs)),
                    op: (cycles.star_token().or(cycles.plus_token())).map(|op| op.kind()),
                    bounds: self.bounds(scope, cycles.bounds()),
                    rhs: self.property_of(scope, delay.rhs(), delay.syntax()),
                },
                None => return self.opaque_expr(delay.syntax()),
            },
            ast::PropExpr::Repetition(repetition) => match repetition.op() {
                Some(op) => Property::Repetition {
                    op: op.kind(),
                    operand: self.property_of(scope, repetition.operand(), repetition.syntax()),
                    bounds: self.bounds(scope, repetition.bounds()),
                },
                None => return self.opaque_expr(repetition.syntax()),
            },
        };
        self.expr(ExprKind::Property(kind), span)
    }

    /// A count, or a range's low and high.
    fn bounds(
        &mut self,
        scope: ScopeId,
        bounds: impl Iterator<Item = ast::Expr>,
    ) -> Option<Bounds> {
        let mut bounds = bounds.map(|bound| self.expr_of(scope, bound));
        let low = bounds.next()?;
        Some(Bounds {
            low,
            high: bounds.next(),
        })
    }

    // ---------------------------------------------------------------- timing

    fn timing(
        &mut self,
        scope: ScopeId,
        delay: Option<ast::DelayControl>,
        event: Option<ast::EventControl>,
        repeat: Option<ast::RepeatControl>,
    ) -> Option<Timing> {
        if let Some(delay) = delay {
            return Some(self.delay(scope, &delay));
        }
        if let Some(event) = event {
            return Some(self.event(scope, &event));
        }
        let repeat = repeat?;
        let count = self.condition(scope, repeat.count(), repeat.syntax());
        let events = match repeat
            .event_control()
            .map(|event| self.event(scope, &event))
        {
            Some(Timing::Events(events)) => events,
            _ => Vec::new(),
        };
        Some(Timing::Repeat(count, events))
    }

    /// `#d` or `##n`.
    pub(super) fn delay(&mut self, scope: ScopeId, delay: &ast::DelayControl) -> Timing {
        let op = delay.op().map_or(HASH, |op| op.kind());
        let value = match delay.expr() {
            Some(value) => self.expr_of(scope, value),
            None => self.opaque_expr(delay.syntax()),
        };
        Timing::Delay(op, value)
    }

    /// `@*`, `@ev`, `@(posedge a iff b or c)`.
    pub(super) fn event(&mut self, scope: ScopeId, event: &ast::EventControl) -> Timing {
        if event.star_token().is_some() {
            return Timing::Star;
        }
        let paren = match event.expr() {
            Some(ast::Expr::ParenExpr(paren)) => paren,
            Some(expr) => {
                let expr = self.expr_of(scope, expr);
                return Timing::Events(vec![Event {
                    edge: None,
                    expr,
                    iff: None,
                }]);
            }
            None => return Timing::Events(Vec::new()),
        };
        let mut events: Vec<Event> = Vec::new();
        let mut edge = None;
        let mut iff = false;
        for element in paren.syntax().children_with_tokens() {
            let node = match element {
                SyntaxElement::Token(token) => {
                    match token.kind() {
                        STAR => return Timing::Star,
                        POSEDGE_KW | NEGEDGE_KW | EDGE_KW => edge = Some(token.kind()),
                        IFF_KW => iff = true,
                        _ => {}
                    }
                    continue;
                }
                SyntaxElement::Node(node) => node,
            };
            let expr = match ast::TypeOrExpr::cast(node.clone()) {
                Some(expr) => self.type_or_expr(scope, expr),
                None => self.opaque_expr(&node),
            };
            match events.last_mut() {
                Some(last) if std::mem::take(&mut iff) => last.iff = Some(expr),
                _ => events.push(Event {
                    edge: edge.take(),
                    expr,
                    iff: None,
                }),
            }
        }
        Timing::Events(events)
    }

    // ----------------------------------------------------------- expressions

    pub(super) fn opaque_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let span = self.span(node);
        let opaque = self.opaque(node, None);
        self.expr(ExprKind::Opaque(opaque), span)
    }

    /// A parenthesised condition, or an empty opaque expression over
    /// `around` where the tree has none.
    pub(super) fn condition(
        &mut self,
        scope: ScopeId,
        condition: Option<ast::ParenExpr>,
        around: &SyntaxNode,
    ) -> ExprId {
        match condition {
            Some(condition) => self.paren(scope, &condition),
            None => {
                let span = self.span(around);
                self.expr(ExprKind::Opaque(Opaque::default()), span)
            }
        }
    }

    /// What `(…)` holds, or an empty opaque expression for `()`.
    pub(super) fn paren(&mut self, scope: ScopeId, paren: &ast::ParenExpr) -> ExprId {
        match self.paren_inner(scope, paren) {
            Some(inner) => inner,
            None => {
                let span = self.span(paren.syntax());
                self.expr(ExprKind::Opaque(Opaque::default()), span)
            }
        }
    }

    /// The one expression, type or assignment `(…)` holds, `None` when it
    /// holds nothing, or an opaque one over anything else: an event list, a
    /// `min:typ:max`.
    pub(super) fn paren_inner(&mut self, scope: ScopeId, paren: &ast::ParenExpr) -> Option<ExprId> {
        let plain = (paren.syntax().children_with_tokens())
            .filter_map(SyntaxElement::into_token)
            .all(|token| token.kind().is_trivia() || matches!(token.kind(), L_PAREN | R_PAREN));
        let nodes: Vec<SyntaxNode> = paren.syntax().children().collect();
        match nodes.as_slice() {
            [] if plain => None,
            [node] if plain => Some(if let Some(inner) = ast::TypeOrExpr::cast(node.clone()) {
                self.type_or_expr(scope, inner)
            } else if let Some(assignment) = ast::Assignment::cast(node.clone()) {
                self.assignment(scope, &assignment, None)
            } else {
                self.opaque_expr(node)
            }),
            _ => Some(self.opaque_expr(paren.syntax())),
        }
    }

    /// `call with (…)`: an array method's expression, over the one name the
    /// call passes or `item`; or a bin's, after a cover point. `call with
    /// {…}`: a `randomize`'s constraints. `None` for any other `with`: an
    /// object's `randomize`, whose constraints name its members first, which
    /// are not found through a handle; one restricted to the names it lists;
    /// a stream's slice.
    fn with_call(
        &mut self,
        scope: ScopeId,
        call: &ast::CallExpr,
        callee: ast::Expr,
        with: &ast::WithClause,
    ) -> Option<ExprKind> {
        let (None, None) = (with.l_brack_token(), call.arg_list()) else {
            return None;
        };
        let condition = match (with.condition(), with.constraint_block()) {
            (Some(condition), None) => condition,
            (None, Some(block)) => {
                // `c.randomize() with {…}` is a call of `c.randomize()`.
                let method = match &callee {
                    ast::Expr::CallExpr(inner) => inner.expr(),
                    _ => Some(callee.clone()),
                };
                if let Some(ast::Expr::FieldExpr(_)) = method {
                    return None;
                }
                let call = self.expr_of(scope, callee);
                let constraints = self.constraints(scope, &block);
                return Some(ExprKind::Constrained { call, constraints });
            }
            _ => return None,
        };
        // `q.find(x) with (…)` is a call of `q.find(x)`, whose argument
        // names the iterator.
        let named = match &callee {
            ast::Expr::CallExpr(method) => method.arg_list().and_then(|list| {
                let mut args = list.syntax().children();
                let arg = ast::Arg::cast(args.next()?)?;
                let (None, None) = (args.next(), arg.dot_token()) else {
                    return None;
                };
                let Some(ast::TypeOrExpr::Expr(ast::Expr::NameRef(name))) = arg.type_or_expr()
                else {
                    return None;
                };
                let name = name.name()?;
                matches!(name.kind(), IDENT | ESCAPED_IDENT).then(|| (method.expr(), name))
            }),
            _ => None,
        };
        let (base, name) = match named {
            Some((method, name)) => {
                let span = self.span(callee.syntax());
                let callee = match method {
                    Some(method) => self.expr_of(scope, method),
                    None => self.opaque_expr(callee.syntax()),
                };
                let base = self.expr(
                    ExprKind::Call {
                        callee,
                        args: Vec::new(),
                    },
                    span,
                );
                (base, self.name(&name))
            }
            None => (self.expr_of(scope, callee), self.item(with)),
        };
        let with = self.with(scope, name, &condition);
        Some(ExprKind::With { base, with })
    }

    /// The iterator a `with` names by default, written nowhere: placed at
    /// its `with`.
    pub(super) fn item(&self, with: &ast::WithClause) -> Name {
        let span = match with.with_token() {
            Some(token) => self.token_span(&token),
            None => self.span(with.syntax()),
        };
        Name {
            text: "item".into(),
            span,
        }
    }

    /// `with (…)`, its expression over `name`, which a scope of its own
    /// declares.
    pub(super) fn with(&mut self, scope: ScopeId, name: Name, condition: &ast::ParenExpr) -> With {
        let inner = self.scope(ScopeKind::Loop, Some(scope), None);
        self.declare(inner, name, SymbolKind::Iterator);
        let expr = self.paren(inner, condition);
        With { scope: inner, expr }
    }

    pub(super) fn type_or_expr(&mut self, scope: ScopeId, value: ast::TypeOrExpr) -> ExprId {
        match value {
            ast::TypeOrExpr::Expr(expr) => self.expr_of(scope, expr),
            ast::TypeOrExpr::DataType(ty) => {
                let span = self.span(ty.syntax());
                let ty = self.data_type(scope, Some(ty));
                self.expr(ExprKind::Type(Box::new(ty)), span)
            }
        }
    }

    pub(super) fn expr_of(&mut self, scope: ScopeId, expr: ast::Expr) -> ExprId {
        let span = self.span(expr.syntax());
        let kind = match &expr {
            ast::Expr::LiteralExpr(literal) if literal.macro_call().is_none() => {
                let text: String = (literal.syntax().descendants_with_tokens())
                    .filter_map(SyntaxElement::into_token)
                    .filter(|token| !token.kind().is_trivia())
                    .map(|token| token.text().to_string())
                    .collect();
                let time = (literal.syntax().children_with_tokens())
                    .any(|element| element.kind() == TIME_LITERAL);
                match time {
                    true => ExprKind::Time {
                        value: self.timescale.scale(&text),
                        text: text.into(),
                    },
                    false => ExprKind::Literal(text.into()),
                }
            }
            ast::Expr::NameRef(name) => match name.name() {
                Some(token) => match token.kind() {
                    IDENT | ESCAPED_IDENT => ExprKind::Name(self.name(&token).text),
                    SYSTEM_IDENT => ExprKind::System(token.text().into()),
                    keyword => ExprKind::Keyword(keyword),
                },
                None => return self.opaque_expr(name.syntax()),
            },
            ast::Expr::ParenExpr(paren) => return self.paren(scope, paren),
            // A tool takes the typical value unless told otherwise.
            ast::Expr::MinTypMaxExpr(values) => match values.typ() {
                Some(typ) => return self.expr_of(scope, typ),
                None => return self.opaque_expr(values.syntax()),
            },
            ast::Expr::UnaryExpr(unary) => match (unary.op(), unary.expr()) {
                (Some(op), Some(operand)) => ExprKind::Unary {
                    op: op.kind(),
                    operand: self.expr_of(scope, operand),
                },
                _ => return self.opaque_expr(unary.syntax()),
            },
            ast::Expr::PostfixExpr(postfix) => match (postfix.op(), postfix.expr()) {
                (Some(op), Some(operand)) => ExprKind::Postfix {
                    op: op.kind(),
                    operand: self.expr_of(scope, operand),
                },
                _ => return self.opaque_expr(postfix.syntax()),
            },
            ast::Expr::BinExpr(binary) => match (binary.lhs(), binary.op(), binary.rhs()) {
                (Some(lhs), Some(op), Some(rhs)) => ExprKind::Binary {
                    op: op.kind(),
                    lhs: self.expr_of(scope, lhs),
                    rhs: self.expr_of(scope, rhs),
                },
                _ => return self.opaque_expr(binary.syntax()),
            },
            ast::Expr::TernaryExpr(ternary) => {
                match (
                    ternary.condition(),
                    ternary.then_value(),
                    ternary.else_value(),
                ) {
                    (Some(condition), Some(then_value), Some(else_value)) => {
                        ExprKind::Conditional {
                            condition: self.expr_of(scope, condition),
                            then_value: self.expr_of(scope, then_value),
                            else_value: self.expr_of(scope, else_value),
                        }
                    }
                    _ => return self.opaque_expr(ternary.syntax()),
                }
            }
            ast::Expr::FieldExpr(field) => match (field.expr(), field.field()) {
                (Some(base), Some(name)) => ExprKind::Member {
                    base: self.expr_of(scope, base),
                    name: self.name(&name),
                },
                _ => return self.opaque_expr(field.syntax()),
            },
            ast::Expr::ScopeExpr(scoped) => match (scoped.expr(), scoped.member()) {
                (Some(base), Some(name)) => ExprKind::Scoped {
                    base: self.expr_of(scope, base),
                    name: self.name(&name),
                },
                _ => return self.opaque_expr(scoped.syntax()),
            },
            ast::Expr::IndexExpr(select) => match (select.base(), select.index()) {
                (Some(base), Some(index)) => {
                    let base = self.expr_of(scope, base);
                    let index = self.expr_of(scope, index);
                    let range = match (select.op(), select.end()) {
                        (Some(op), Some(end)) => Some((op.kind(), self.expr_of(scope, end))),
                        _ => None,
                    };
                    ExprKind::Select { base, index, range }
                }
                _ => return self.opaque_expr(select.syntax()),
            },
            ast::Expr::CallExpr(call) => match (call.expr(), call.with_clause()) {
                (Some(callee), None) => {
                    let callee = self.expr_of(scope, callee);
                    let args = match call.arg_list() {
                        Some(list) => self.args(scope, &list),
                        None => Vec::new(),
                    };
                    ExprKind::Call { callee, args }
                }
                (Some(callee), Some(with)) => match self.with_call(scope, call, callee, &with) {
                    Some(kind) => kind,
                    None => return self.opaque_expr(call.syntax()),
                },
                (None, _) => return self.opaque_expr(call.syntax()),
            },
            ast::Expr::CastExpr(cast) => match (cast.ty(), cast.operand()) {
                (Some(ty), Some(operand)) => ExprKind::Cast {
                    ty: self.expr_of(scope, ty),
                    operand: self.paren(scope, &operand),
                },
                _ => return self.opaque_expr(cast.syntax()),
            },
            ast::Expr::ConcatExpr(concat) => ExprKind::Concat(
                concat
                    .exprs()
                    .map(|expr| self.expr_of(scope, expr))
                    .collect(),
            ),
            ast::Expr::ReplicationExpr(replication) => {
                match (replication.count(), replication.concat()) {
                    (Some(count), Some(concat)) => ExprKind::Replication {
                        count: self.expr_of(scope, count),
                        concat: self.expr_of(scope, ast::Expr::ConcatExpr(concat)),
                    },
                    _ => return self.opaque_expr(replication.syntax()),
                }
            }
            ast::Expr::StreamExpr(stream) => match (stream.op(), stream.concat()) {
                (Some(op), Some(concat)) => ExprKind::Stream {
                    op: op.kind(),
                    slice: stream.slice().map(|slice| self.type_or_expr(scope, slice)),
                    concat: self.expr_of(scope, ast::Expr::ConcatExpr(concat)),
                },
                _ => return self.opaque_expr(stream.syntax()),
            },
            ast::Expr::AssignmentPattern(pattern) => {
                let ty = pattern.expr().map(|ty| self.expr_of(scope, ty));
                let mut items = Vec::new();
                for item in pattern.pattern_items() {
                    let key = item.key().map(|key| self.expr_of(scope, key));
                    let value = match item.value() {
                        Some(value) => self.expr_of(scope, value),
                        None => self.opaque_expr(item.syntax()),
                    };
                    items.push(PatternItem { key, value });
                }
                ExprKind::Pattern { ty, items }
            }
            // A list a macro writes is not known.
            ast::Expr::InsideExpr(inside) => match (inside.expr(), inside.range_list()) {
                (Some(tested), Some(list)) if list.l_brace_token().is_some() => {
                    let expr = self.expr_of(scope, tested);
                    let set = self.set(scope, list.syntax());
                    ExprKind::Inside { expr, set }
                }
                _ => return self.opaque_expr(inside.syntax()),
            },
            // A list a macro writes is not known.
            ast::Expr::DistExpr(dist) => match (dist.expr(), dist.range_list()) {
                (Some(tested), Some(list)) if list.l_brace_token().is_some() => {
                    let expr = self.expr_of(scope, tested);
                    let values: Vec<ast::Expr> = (list.exprs())
                        .chain(list.dist_items().flat_map(|item| item.exprs()))
                        .collect();
                    let set = values
                        .into_iter()
                        .map(|value| self.expr_of(scope, value))
                        .collect();
                    ExprKind::Dist { expr, set }
                }
                _ => return self.opaque_expr(dist.syntax()),
            },
            ast::Expr::BinsofExpr(binsof) => match binsof.expr() {
                Some(bins) => {
                    let bins = self.expr_of(scope, bins);
                    let intersect = match binsof.range_list() {
                        Some(list) if list.l_brace_token().is_some() => {
                            let set = list.exprs().collect::<Vec<_>>();
                            set.into_iter()
                                .map(|value| self.expr_of(scope, value))
                                .collect()
                        }
                        Some(list) => vec![self.opaque_expr(list.syntax())],
                        None => Vec::new(),
                    };
                    ExprKind::Binsof { bins, intersect }
                }
                None => return self.opaque_expr(binsof.syntax()),
            },
            ast::Expr::TaggedExpr(tagged) => match tagged.member() {
                Some(member) => ExprKind::Tagged {
                    member: self.name(&member),
                    value: tagged.value().map(|value| self.expr_of(scope, value)),
                },
                None => return self.opaque_expr(tagged.syntax()),
            },
            ast::Expr::TypeReference(reference) => {
                let ty =
                    self.data_type(scope, Some(ast::DataType::TypeReference(reference.clone())));
                ExprKind::Type(Box::new(ty))
            }
            _ => return self.opaque_expr(expr.syntax()),
        };
        self.expr(kind, span)
    }
}
