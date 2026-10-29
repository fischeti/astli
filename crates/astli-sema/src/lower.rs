//! Lowering a tree to its [`Hir`]: the declarations here, statements and
//! expressions in `body`.

mod body;

use astli_parse::Parsed;
use astli_syntax::SyntaxKind::{self, *};
use astli_syntax::ast::{self, AstNode};
use astli_syntax::{SyntaxElement, SyntaxNode, SyntaxToken};
use astli_text::Span;
use rowan::TextSize;

use crate::hir::*;
use crate::value::{Literal, literal};

/// Lowers `parsed`, a tree in either mode, to its HIR.
///
/// An expanded tree is what the HIR is for. A raw tree lowers too, its macro
/// calls and conditionals opaque.
pub fn lower(parsed: &Parsed) -> Hir {
    let mut hir = Hir::default();
    let fallback = parsed.root.descendants_with_tokens().find_map(|element| {
        let token = element.into_token()?;
        parsed.span(&token)
    });
    // A tree without a placed token holds no construct to lower.
    let Some(fallback) = fallback else {
        hir.scopes.push(Scope {
            kind: ScopeKind::File,
            parent: None,
            owner: None,
            members: Vec::new(),
        });
        return hir;
    };
    let mut lower = Lower {
        parsed,
        hir,
        fallback,
        ports: Vec::new(),
        overridable: None,
        directives: directives(&parsed.root),
        timescale: Timescale::DEFAULT,
        file_timescale: Timescale::DEFAULT,
    };
    let root = lower.scope(ScopeKind::File, None, None);
    let mut definitions = Vec::new();
    if let Some(file) = ast::SourceFile::cast(parsed.root.clone()) {
        lower.file_timescale = Timescale::DEFAULT.declared(&parsed.root);
        lower.timescale = lower.file_timescale;
        for item in file.items() {
            if matches!(
                item,
                ast::Item::ModuleDecl(_)
                    | ast::Item::InterfaceDecl(_)
                    | ast::Item::ProgramDecl(_)
                    | ast::Item::PackageDecl(_)
            ) {
                definitions.push(item.syntax().text_range());
            }
            lower.member(root, item);
        }
    }
    // A definition holds what it misses itself.
    let includes = lower.directives.includes.iter();
    if includes
        .clone()
        .any(|at| !definitions.iter().any(|range| range.contains(*at)))
    {
        lower.push(root, Member::Opaque(Opaque::default()));
    }
    lower.hir
}

pub(crate) struct Lower<'a> {
    parsed: &'a Parsed,
    hir: Hir,
    /// The span of a node none of whose tokens has one, which no parsed
    /// construct is.
    fallback: Span,
    /// The ports of the definition or subroutine being lowered, which a
    /// declaration in its body completes.
    ports: Vec<SymbolId>,
    /// The scope whose `parameter`s can be overridden: the body of a design
    /// element without a parameter port list.
    overridable: Option<ScopeId>,
    directives: Directives,
    /// The time unit and precision of the design element being lowered, or
    /// of the file's top level.
    timescale: Timescale,
    /// What the file's top level declares, which a design element after no
    /// `` `timescale `` takes.
    file_timescale: Timescale,
}

impl Lower<'_> {
    // ---------------------------------------------------------------- arenas

    fn scope(
        &mut self,
        kind: ScopeKind,
        parent: Option<ScopeId>,
        owner: Option<SymbolId>,
    ) -> ScopeId {
        self.hir.scopes.push(Scope {
            kind,
            parent,
            owner,
            members: Vec::new(),
        });
        ScopeId(self.hir.scopes.len() as u32 - 1)
    }

    fn push(&mut self, scope: ScopeId, member: Member) {
        self.hir.scopes[scope.index()].members.push(member);
    }

    /// Declares `name` in `scope`.
    fn declare(&mut self, scope: ScopeId, name: Name, kind: SymbolKind) -> SymbolId {
        let id = self.symbol(name, kind);
        self.push(scope, Member::Declare(id));
        id
    }

    /// A symbol no scope declares, as a bound instance is.
    fn symbol(&mut self, name: Name, kind: SymbolKind) -> SymbolId {
        self.hir.symbols.push(Symbol { name, kind });
        SymbolId(self.hir.symbols.len() as u32 - 1)
    }

    fn expr(&mut self, kind: ExprKind, span: Span) -> ExprId {
        self.hir.exprs.push(Expr { kind, span });
        ExprId(self.hir.exprs.len() as u32 - 1)
    }

    fn stmt(&mut self, kind: StmtKind, span: Span) -> StmtId {
        self.hir.stmts.push(Stmt { kind, span });
        StmtId(self.hir.stmts.len() as u32 - 1)
    }

    // ----------------------------------------------------------------- spans

    fn token_span(&self, token: &SyntaxToken) -> Span {
        self.parsed.span(token).unwrap_or(self.fallback)
    }

    /// From `node`'s first token to its last, or its first alone when a
    /// macro placed them apart.
    fn span(&self, node: &SyntaxNode) -> Span {
        let mut spans = (node.descendants_with_tokens())
            .filter_map(|element| element.into_token())
            .filter(|token| !token.kind().is_trivia())
            .filter_map(|token| self.parsed.span(&token));
        let Some(first) = spans.next() else {
            return self.fallback;
        };
        let last = spans.last().unwrap_or(first);
        first.cover(last).unwrap_or(first)
    }

    /// The time unit and precision of a design element written at `at` in
    /// `scope`, before what it declares itself: its enclosing element's, or
    /// the last `` `timescale `` before it, or the file's.
    fn timescale_at(&self, scope: ScopeId, at: TextSize) -> Timescale {
        if scope != self.hir.root() {
            return self.timescale;
        }
        let directives = self.directives.timescales.iter();
        let before = directives.take_while(|(offset, _)| *offset < at);
        match before.last() {
            Some((_, Some(timescale))) => *timescale,
            Some((_, None)) | None => self.file_timescale,
        }
    }

    /// Whether the directives before `at` let a net be declared implicitly,
    /// as they do when none is written.
    fn implicit_nets(&self, at: TextSize) -> bool {
        let nettypes = self.directives.nettypes.iter();
        let before = nettypes.take_while(|(offset, _)| *offset < at);
        before.last().is_none_or(|(_, allows)| *allows)
    }

    fn name(&self, token: &SyntaxToken) -> Name {
        let text = token.text();
        let text = match token.kind() {
            ESCAPED_IDENT => text.strip_prefix('\\').unwrap_or(text).trim_end(),
            _ => text,
        };
        Name {
            text: text.into(),
            span: self.token_span(token),
        }
    }

    /// Every name spelled in `node` but `except`, the one it declares.
    fn opaque(&self, node: &SyntaxNode, except: Option<&SyntaxToken>) -> Opaque {
        let names = (node.descendants_with_tokens())
            .filter_map(|element| element.into_token())
            .filter(|token| matches!(token.kind(), IDENT | ESCAPED_IDENT))
            .filter(|token| Some(token) != except)
            .map(|token| self.name(&token))
            .collect();
        Opaque { names }
    }

    fn opaque_member(&mut self, scope: ScopeId, node: &SyntaxNode) {
        let opaque = self.opaque(node, None);
        self.push(scope, Member::Opaque(opaque));
    }

    /// Declares what `node` names, its body opaque.
    fn other(&mut self, scope: ScopeId, node: &SyntaxNode, name: Option<SyntaxToken>) {
        match name {
            Some(name) => {
                let body = self.opaque(node, Some(&name));
                let name = self.name(&name);
                self.declare(scope, name, SymbolKind::Other(body));
            }
            None => self.opaque_member(scope, node),
        }
    }

    // ----------------------------------------------------------------- items

    /// Lowers `item` standing in `scope`, a scope of items rather than
    /// statements: a file, a design element, a generate block.
    fn member(&mut self, scope: ScopeId, item: ast::Item) {
        if self.declaration(scope, &item) {
            return;
        }
        match item {
            ast::Item::ContinuousAssign(assign) => {
                let timing = assign
                    .delay_control()
                    .map(|delay| self.delay(scope, &delay));
                let assignments = assign
                    .assignments()
                    .map(|assignment| self.assignment(scope, &assignment, timing.clone()))
                    .collect();
                self.push(scope, Member::Assign(assignments));
            }
            ast::Item::Instantiation(instantiation) => {
                let Some(instances) = self.instantiation(scope, &instantiation) else {
                    return self.opaque_member(scope, instantiation.syntax());
                };
                for instance in instances {
                    self.push(scope, Member::Declare(instance));
                }
            }
            ast::Item::BindDirective(bind) => self.bind(scope, &bind),
            ast::Item::GateInstantiation(gate) => {
                let Some(keyword) = gate.keyword() else {
                    return self.opaque_member(scope, gate.syntax());
                };
                let definition = self.name(&keyword);
                let delay = gate.delay_control().map(|delay| self.delay(scope, &delay));
                let instance = Instance {
                    definition,
                    gate: Some(keyword.kind()),
                    parameters: Vec::new(),
                    delay,
                    dims: Vec::new(),
                    connections: Vec::new(),
                };
                for instance in self.instances(scope, instance, gate.instances()) {
                    self.push(scope, Member::Declare(instance));
                }
            }
            ast::Item::ProceduralBlock(block) => {
                let Some(keyword) = block.keyword() else {
                    return self.opaque_member(scope, block.syntax());
                };
                let span = self.span(block.syntax());
                let body = match block.body() {
                    Some(body) => self.body(scope, body),
                    None => None,
                };
                let body = body.unwrap_or_else(|| self.stmt(StmtKind::Empty, span));
                let span = self.token_span(&keyword);
                let process = Process {
                    kind: keyword.kind(),
                    body,
                    span,
                };
                self.push(scope, Member::Process(process));
            }
            ast::Item::GenerateRegion(region) => {
                for item in region.items() {
                    self.member(scope, item);
                }
            }
            ast::Item::IfStmt(stmt) => {
                let mut arms = Vec::new();
                self.generate_if(scope, &stmt, &mut arms);
                self.push(scope, Member::GenerateIf(arms));
            }
            ast::Item::CaseStmt(stmt) => {
                let Some(selector) = stmt.paren_expr() else {
                    return self.opaque_member(scope, stmt.syntax());
                };
                let selector = self.paren(scope, &selector);
                let mut arms = Vec::new();
                for item in stmt.case_items() {
                    let conditions = item.exprs().map(|expr| self.expr_of(scope, expr)).collect();
                    let body = self.generate_body(scope, item.item());
                    arms.push(GenerateArm { conditions, body });
                }
                // What the grammar left unparsed among the arms cannot be
                // placed in one, so it stands beside the `case`.
                for verbatim in stmt.verbatims() {
                    self.opaque_member(scope, verbatim.syntax());
                }
                self.push(scope, Member::GenerateCase { selector, arms });
            }
            ast::Item::ForStmt(stmt) => {
                let Some(header) = stmt.header() else {
                    return self.opaque_member(scope, stmt.syntax());
                };
                let header = self.for_header(scope, &header);
                let body = self.generate_body(header.scope, stmt.body());
                self.push(scope, Member::GenerateFor { header, body });
            }
            ast::Item::Block(block) => {
                let body = self.generate_block(scope, &block, None);
                self.push(scope, Member::Generate(body));
            }
            ast::Item::LabeledStmt(stmt) => match (stmt.label(), stmt.item()) {
                (Some(label), Some(ast::Item::Block(block))) => {
                    let body = self.generate_block(scope, &block, Some(label));
                    self.push(scope, Member::Generate(body));
                }
                (Some(label), Some(ast::Item::ImmediateAssertion(assertion))) => {
                    let stmt = self.deferred(scope, &assertion);
                    let name = self.name(&label);
                    self.declare(scope, name, SymbolKind::Label(stmt));
                }
                (Some(label), Some(ast::Item::ConcurrentAssertion(assertion))) => {
                    match self.assertion(scope, &assertion) {
                        Some(stmt) => {
                            let name = self.name(&label);
                            self.declare(scope, name, SymbolKind::Label(stmt));
                        }
                        None => self.other(scope, stmt.syntax(), Some(label)),
                    }
                }
                (label, _) => self.other(scope, stmt.syntax(), label),
            },
            ast::Item::ConcurrentAssertion(assertion) => {
                if self.assertion(scope, &assertion).is_none() {
                    self.opaque_member(scope, assertion.syntax());
                }
            }
            ast::Item::ImmediateAssertion(assertion) => {
                self.deferred(scope, &assertion);
            }
            ast::Item::DefaultDisable(disable) => match disable.expr() {
                Some(condition) => {
                    let condition = self.expr_of(scope, condition);
                    self.push(scope, Member::DefaultDisable(condition));
                }
                None => self.opaque_member(scope, disable.syntax()),
            },
            // A lone `;`.
            ast::Item::ExprStmt(stmt) if stmt.syntax().children().next().is_none() => {}
            item => self.opaque_member(scope, item.syntax()),
        }
    }

    /// A concurrent assertion standing among items, as the process it runs
    /// as; its statement, or `None` if it has no property.
    fn assertion(
        &mut self,
        scope: ScopeId,
        assertion: &ast::ConcurrentAssertion,
    ) -> Option<StmtId> {
        let keyword = assertion.keyword()?;
        let kind = self.concurrent(scope, assertion)?;
        let span = self.span(assertion.syntax());
        let body = self.stmt(kind, span);
        let process = Process {
            kind: keyword.kind(),
            body,
            span: self.token_span(&keyword),
        };
        self.push(scope, Member::Process(process));
        Some(body)
    }

    /// A deferred immediate assertion standing among items, as the process
    /// it runs as; its statement.
    fn deferred(&mut self, scope: ScopeId, assertion: &ast::ImmediateAssertion) -> StmtId {
        let keyword = assertion
            .keyword()
            .map_or(ASSERT_KW, |keyword| keyword.kind());
        let item = ast::Item::ImmediateAssertion(assertion.clone());
        let body = (self.body(scope, item)).expect("an assertion declares nothing");
        let span = match assertion.keyword() {
            Some(keyword) => self.token_span(&keyword),
            None => self.span(assertion.syntax()),
        };
        let process = Process {
            kind: keyword,
            body,
            span,
        };
        self.push(scope, Member::Process(process));
        body
    }

    /// Lowers `item` if it declares something, as it may in a scope of items
    /// or of statements; whether it did.
    fn declaration(&mut self, scope: ScopeId, item: &ast::Item) -> bool {
        match item {
            ast::Item::ModuleDecl(decl) => self.definition(
                scope,
                decl.syntax(),
                decl.keyword(),
                decl.name(),
                decl.param_port_list(),
                decl.port_list(),
                decl.items(),
            ),
            ast::Item::InterfaceDecl(decl) => self.definition(
                scope,
                decl.syntax(),
                decl.interface_token(),
                decl.name(),
                decl.param_port_list(),
                decl.port_list(),
                decl.items(),
            ),
            ast::Item::ProgramDecl(decl) => self.definition(
                scope,
                decl.syntax(),
                decl.program_token(),
                decl.name(),
                decl.param_port_list(),
                decl.port_list(),
                decl.items(),
            ),
            // Instantiated as a module is, with formals for ports.
            ast::Item::CheckerDecl(decl) => self.definition(
                scope,
                decl.syntax(),
                decl.checker_token(),
                decl.name(),
                None,
                decl.port_list(),
                decl.items(),
            ),
            // Instantiated as a module is, its output first.
            ast::Item::UdpDecl(decl) => self.definition(
                scope,
                decl.syntax(),
                decl.primitive_token(),
                decl.name(),
                None,
                decl.port_list(),
                decl.items(),
            ),
            ast::Item::PackageDecl(decl) => self.definition(
                scope,
                decl.syntax(),
                decl.package_token(),
                decl.name(),
                None,
                None,
                decl.items(),
            ),
            ast::Item::ClassDecl(decl) => self.class(scope, decl),
            ast::Item::ConstraintDecl(decl) => self.constraint(scope, decl),
            ast::Item::FunctionDecl(decl) => {
                let returns =
                    Some(self.data_type(scope, decl.type_ref().map(ast::DataType::TypeRef)));
                self.subroutine(
                    scope,
                    decl.syntax(),
                    decl.function_token(),
                    returns,
                    decl.port_list(),
                    decl.items(),
                );
            }
            ast::Item::TaskDecl(decl) => {
                self.subroutine(
                    scope,
                    decl.syntax(),
                    decl.task_token(),
                    None,
                    decl.port_list(),
                    decl.items(),
                );
            }
            ast::Item::VarDecl(decl) => self.var_decl(scope, decl),
            ast::Item::ParamDecl(decl) => {
                self.param_decl(scope, decl, false, None);
            }
            ast::Item::Typedef(typedef) if typedef.data_type().is_none() => {
                match typedef.declarator().and_then(|it| it.name()) {
                    Some(name) => {
                        let name = self.name(&name);
                        self.declare(scope, name, SymbolKind::Forward);
                    }
                    None => self.opaque_member(scope, typedef.syntax()),
                }
            }
            ast::Item::Typedef(typedef) => {
                let ty = self.data_type(scope, typedef.data_type());
                let declarator = typedef.declarator();
                match declarator.as_ref().and_then(|it| it.name()) {
                    Some(name) => {
                        let ty = self.with_unpacked(scope, ty, declarator.as_ref());
                        let name = self.name(&name);
                        self.declare(scope, name, SymbolKind::Typedef(ty));
                    }
                    // A forward typedef names a type declared later.
                    None => self.opaque_member(scope, typedef.syntax()),
                }
            }
            ast::Item::ImportDecl(decl) => self.import(scope, decl),
            ast::Item::PortDecl(decl) => self.port_decl(scope, decl),
            ast::Item::ModportDecl(decl) => {
                for modport in decl.modports() {
                    self.modport(scope, &modport);
                }
            }
            ast::Item::NettypeDecl(decl) => {
                let name = decl.declarator().and_then(|it| it.name());
                self.other(scope, decl.syntax(), name);
            }
            ast::Item::LetDecl(decl) => self.let_decl(scope, decl),
            ast::Item::PropertyDecl(decl) => self.property_decl(
                scope,
                decl.syntax(),
                decl.property_token(),
                decl.name(),
                decl.port_list(),
                decl.var_decls(),
                decl.property_spec(),
            ),
            ast::Item::SequenceDecl(decl) => self.property_decl(
                scope,
                decl.syntax(),
                decl.sequence_token(),
                decl.name(),
                decl.port_list(),
                decl.var_decls(),
                decl.property_spec(),
            ),
            ast::Item::CovergroupDecl(decl) => self.covergroup(scope, decl),
            ast::Item::ClockingDecl(decl) => self.clocking(scope, decl),
            ast::Item::ConfigDecl(decl) => self.other(scope, decl.syntax(), decl.name()),
            // Time units and a UDP's table name nothing and read no name.
            ast::Item::TimeunitDecl(_) | ast::Item::UdpTable(_) => {}
            _ => return false,
        }
        true
    }

    /// A module, interface, program or package.
    #[expect(clippy::too_many_arguments, reason = "the parts four views share")]
    fn definition(
        &mut self,
        scope: ScopeId,
        node: &SyntaxNode,
        keyword: Option<SyntaxToken>,
        name: Option<SyntaxToken>,
        params: Option<ast::ParamPortList>,
        ports: Option<ast::PortList>,
        items: ast::AstChildren<ast::Item>,
    ) {
        let (Some(keyword), Some(name)) = (keyword, name) else {
            return self.opaque_member(scope, node);
        };
        let name = self.name(&name);
        let body = self.scope(ScopeKind::Definition, Some(scope), None);
        let kind = SymbolKind::Definition {
            keyword: keyword.kind(),
            scope: body,
            ports: Vec::new(),
            ports_known: true,
            parameters: Vec::new(),
            parameters_known: true,
            // The directive before the keyword may be the node's own leading
            // trivia.
            implicit_nets: self.implicit_nets(keyword.text_range().start()),
        };
        let symbol = self.declare(scope, name, kind);
        self.hir.scopes[body.index()].owner = Some(symbol);
        let outer_timescale = self.timescale;
        let at = keyword.text_range().start();
        self.timescale = self.timescale_at(scope, at).declared(node);
        // What an include not followed would have declared is unknown.
        let range = node.text_range();
        if self
            .directives
            .includes
            .iter()
            .any(|at| range.contains(*at))
        {
            self.push(body, Member::Opaque(Opaque::default()));
        }

        let outer_ports = std::mem::take(&mut self.ports);
        let outer_overridable = self.overridable;
        self.overridable = (params.is_none() && keyword.kind() != PACKAGE_KW).then_some(body);

        // The imports in the header come first, since the parameters and
        // ports may use what they import.
        let header_end = (node.children_with_tokens())
            .find(|element| element.kind() == SEMICOLON)
            .map_or(node.text_range().end(), |semicolon| {
                semicolon.text_range().start()
            });
        let (header, items): (Vec<_>, Vec<_>) =
            items.partition(|item| item.syntax().text_range().end() <= header_end);
        for item in header {
            self.member(body, item);
        }
        let listed = params.is_some();
        let (mut parameters, mut parameters_known) = match params {
            Some(list) => self.param_port_list(body, &list),
            None => (Vec::new(), true),
        };
        // A checker's formals are inputs unless they say otherwise.
        let direction = match keyword.kind() {
            CHECKER_KW => INPUT_KW,
            _ => INOUT_KW,
        };
        let ports_known = match ports {
            Some(list) => self.port_list(body, &list, direction),
            None => true,
        };
        for item in items {
            self.member(body, item);
        }
        // Without a parameter port list, what an instance overrides is the
        // body's `parameter`s, and an opaque member may be one.
        if !listed {
            let members = &self.hir[body].members;
            let overridable = |member: &Member| match member {
                Member::Declare(symbol) => match &self.hir[*symbol].kind {
                    SymbolKind::Parameter(parameter) => (!parameter.local).then_some(*symbol),
                    _ => None,
                },
                _ => None,
            };
            parameters = members.iter().filter_map(overridable).collect();
            parameters_known = !members.iter().any(|it| matches!(it, Member::Opaque(_)));
        }

        let ports = std::mem::replace(&mut self.ports, outer_ports);
        self.overridable = outer_overridable;
        self.timescale = outer_timescale;
        if let SymbolKind::Definition {
            ports: declared,
            ports_known: all_ports,
            parameters: overridable,
            parameters_known: all_parameters,
            ..
        } = &mut self.hir.symbols[symbol.index()].kind
        {
            *declared = ports;
            *all_ports = ports_known;
            *overridable = parameters;
            *all_parameters = parameters_known;
        }
    }

    /// The parameters a parameter port list declares in `body`, in order,
    /// and whether they are all of them.
    fn param_port_list(
        &mut self,
        body: ScopeId,
        list: &ast::ParamPortList,
    ) -> (Vec<SymbolId>, bool) {
        let mut parameters = Vec::new();
        let mut known = true;
        let mut previous = None;
        for child in list.syntax().children() {
            match ast::ParamDecl::cast(child.clone()) {
                Some(decl) => {
                    let style = self.param_decl(body, &decl, true, previous.take());
                    parameters.extend(style.symbols.iter().copied());
                    previous = Some(style);
                }
                None => {
                    self.opaque_member(body, &child);
                    known = false;
                }
            }
        }
        (parameters, known)
    }

    /// A class, its parameters and members in a scope of its own.
    fn class(&mut self, scope: ScopeId, decl: &ast::ClassDecl) {
        let Some(name) = decl.name() else {
            return self.opaque_member(scope, decl.syntax());
        };
        let name = self.name(&name);
        let body = self.scope(ScopeKind::Class, Some(scope), None);
        let outer_ports = std::mem::take(&mut self.ports);
        let outer_overridable = self.overridable.take();
        let parameters = match decl.param_port_list() {
            Some(list) => self.param_port_list(body, &list).0,
            None => Vec::new(),
        };
        // What it extends and implements may name its parameters.
        let extends = decl.extends().map(|base| {
            let ty = self.data_type(body, Some(ast::DataType::TypeRef(base)));
            let args = match decl.arg_list() {
                Some(list) => self.args(body, &list),
                None => Vec::new(),
            };
            (ty, args)
        });
        let implements = (decl.implements())
            .map(|it| self.data_type(body, Some(ast::DataType::TypeRef(it))))
            .collect();
        let kind = SymbolKind::Class(Class {
            scope: body,
            parameters,
            extends,
            implements,
        });
        let symbol = self.declare(scope, name, kind);
        self.hir.scopes[body.index()].owner = Some(symbol);
        for item in decl.items() {
            self.member(body, item);
        }
        self.ports = outer_ports;
        self.overridable = outer_overridable;
    }

    /// Where a method or a constraint named in `node` is declared, and its
    /// name: `scope`, or, for one defined outside its class, `C::name`,
    /// the scope of the class `C` that `scope` or one around it declares.
    /// `None` if that class is not found, or the name is not written.
    fn member_of_class(&self, scope: ScopeId, node: &SyntaxNode) -> Option<(ScopeId, SyntaxToken)> {
        let tokens: Vec<SyntaxToken> = (node.children_with_tokens())
            .filter_map(|element| element.into_token())
            .filter(|token| matches!(token.kind(), IDENT | ESCAPED_IDENT | NEW_KW | COLON_COLON))
            .collect();
        let Some(qualified) = tokens.iter().position(|token| token.kind() == COLON_COLON) else {
            return Some((scope, tokens.first()?.clone()));
        };
        let class = tokens.get(qualified.checked_sub(1)?)?;
        let name = tokens.get(qualified + 1)?;
        let class = self.name(class);
        let mut at = Some(scope);
        while let Some(scope) = at {
            for member in &self.hir[scope].members {
                if let Member::Declare(symbol) = member
                    && let SymbolKind::Class(found) = &self.hir[*symbol].kind
                    && self.hir[*symbol].name.text == class.text
                {
                    return Some((found.scope, name.clone()));
                }
            }
            at = self.hir[scope].parent;
        }
        None
    }

    /// A constraint, declared in its class.
    fn constraint(&mut self, scope: ScopeId, decl: &ast::ConstraintDecl) {
        let Some((scope, name)) = self.member_of_class(scope, decl.syntax()) else {
            return self.opaque_member(scope, decl.syntax());
        };
        if self.hir[scope].kind != ScopeKind::Class || name.kind() == NEW_KW {
            return self.opaque_member(scope, decl.syntax());
        }
        let name = self.name(&name);
        let body = (decl.constraint_block()).map(|block| self.constraints(scope, &block));
        self.declare(scope, name, SymbolKind::Constraint(body));
    }

    /// A function or a task.
    fn subroutine(
        &mut self,
        scope: ScopeId,
        node: &SyntaxNode,
        keyword: Option<SyntaxToken>,
        returns: Option<Type>,
        ports: Option<ast::PortList>,
        items: ast::AstChildren<ast::Item>,
    ) {
        let tokens = || (node.children_with_tokens()).filter_map(|element| element.into_token());
        // A method defined outside its class, `C::f`, is declared in the
        // class; only a class has a constructor.
        let (Some(keyword), Some((scope, name))) = (keyword, self.member_of_class(scope, node))
        else {
            return self.opaque_member(scope, node);
        };
        if name.kind() == NEW_KW && self.hir[scope].kind != ScopeKind::Class {
            return self.opaque_member(scope, node);
        }
        let name = self.name(&name);
        let prototype =
            tokens().any(|token| matches!(token.kind(), IMPORT_KW | EXTERN_KW | PURE_KW));
        let body = self.scope(ScopeKind::Subroutine, Some(scope), None);
        let kind = SymbolKind::Subroutine {
            keyword: keyword.kind(),
            scope: body,
            ports: Vec::new(),
            returns,
            body: Vec::new(),
            prototype,
        };
        let symbol = self.declare(scope, name, kind);
        self.hir.scopes[body.index()].owner = Some(symbol);

        let outer_ports = std::mem::take(&mut self.ports);
        let outer_overridable = self.overridable.take();
        if let Some(list) = ports {
            self.port_list(body, &list, INPUT_KW);
        }
        let stmts: Vec<StmtId> = items.filter_map(|item| self.body(body, item)).collect();
        let ports = std::mem::replace(&mut self.ports, outer_ports);
        self.overridable = outer_overridable;
        if let SymbolKind::Subroutine {
            ports: declared,
            body,
            ..
        } = &mut self.hir.symbols[symbol.index()].kind
        {
            *declared = ports;
            *body = stmts;
        }
    }

    /// A property or a sequence, its formals and local variables in a scope
    /// of its own.
    #[expect(clippy::too_many_arguments, reason = "the parts two views share")]
    fn property_decl(
        &mut self,
        scope: ScopeId,
        node: &SyntaxNode,
        keyword: Option<SyntaxToken>,
        name: Option<SyntaxToken>,
        ports: Option<ast::PortList>,
        locals: ast::AstChildren<ast::VarDecl>,
        spec: Option<ast::PropertySpec>,
    ) {
        let (Some(keyword), Some(name), Some(spec)) = (keyword, name, spec) else {
            return self.opaque_member(scope, node);
        };
        let name = self.name(&name);
        let body = self.scope(ScopeKind::Property, Some(scope), None);
        let ports = self.formals(body, ports);
        for decl in locals {
            self.var_decl(body, &decl);
        }
        let spec = self.property_spec(body, &spec);
        let kind = SymbolKind::Property {
            keyword: keyword.kind(),
            scope: body,
            ports,
            spec,
        };
        let symbol = self.declare(scope, name, kind);
        self.hir.scopes[body.index()].owner = Some(symbol);
    }

    /// A `let`, its formals in a scope of its own.
    fn let_decl(&mut self, scope: ScopeId, decl: &ast::LetDecl) {
        let (Some(name), Some(expr)) = (decl.name(), decl.expr()) else {
            return self.other(scope, decl.syntax(), decl.name());
        };
        let name = self.name(&name);
        let body = self.scope(ScopeKind::Property, Some(scope), None);
        let ports = self.formals(body, decl.port_list());
        let expr = self.expr_of(body, expr);
        let kind = SymbolKind::Let {
            scope: body,
            ports,
            expr,
        };
        let symbol = self.declare(scope, name, kind);
        self.hir.scopes[body.index()].owner = Some(symbol);
    }

    /// The formals of a property, a sequence, a `let` or a covergroup,
    /// declared in `body`, in order. One written with neither a direction
    /// nor a type is an input too.
    fn formals(
        &mut self,
        body: ScopeId,
        lists: impl IntoIterator<Item = ast::PortList>,
    ) -> Vec<SymbolId> {
        let outer_ports = std::mem::take(&mut self.ports);
        let outer_overridable = self.overridable.take();
        for list in lists {
            self.port_list(body, &list, INPUT_KW);
        }
        let ports = std::mem::replace(&mut self.ports, outer_ports);
        self.overridable = outer_overridable;
        for &port in &ports {
            if let SymbolKind::Port(port) = &mut self.hir.symbols[port.index()].kind {
                port.direction = port.direction.or(Some(INPUT_KW));
            }
        }
        ports
    }

    /// A covergroup: its formals and `sample`'s arguments in a scope of its
    /// own, and its cover points and crosses in one within that.
    fn covergroup(&mut self, scope: ScopeId, decl: &ast::CovergroupDecl) {
        // One that `extends` adds to the covergroup a base class declares,
        // whose cover points lowering does not find.
        let (Some(name), None) = (decl.name(), decl.extends_token()) else {
            return self.other(scope, decl.syntax(), decl.name());
        };
        let name = self.name(&name);
        let body = self.scope(ScopeKind::Covergroup, Some(scope), None);
        let points = self.scope(ScopeKind::Covergroup, Some(body), None);
        let ports = self.formals(body, decl.formals().into_iter().chain(decl.sample()));
        let event = decl.event_control().map(|event| self.event(body, &event));
        // `@@(begin f)` samples as a method starts or ends, which is not
        // lowered.
        if let (Some(_), Some(block)) = (decl.at_at_token(), decl.paren_expr()) {
            self.opaque_member(body, block.syntax());
        }
        let mut options = Vec::new();
        for item in decl.items() {
            match item {
                ast::Item::Coverpoint(point) => self.coverpoint(body, points, &point),
                ast::Item::Cross(cross) => self.cross(points, &cross),
                item => match self.option(body, &item) {
                    Some(option) => options.push(option),
                    None => self.member(body, item),
                },
            }
        }
        let kind = SymbolKind::Covergroup(Covergroup {
            scope: body,
            ports,
            event,
            points,
            options,
        });
        let symbol = self.declare(scope, name, kind);
        self.hir.scopes[body.index()].owner = Some(symbol);
    }

    /// A cover point, declared in `points`: what it samples and when, names
    /// in `scope`, the covergroup's.
    fn coverpoint(&mut self, scope: ScopeId, points: ScopeId, point: &ast::Coverpoint) {
        let ty = point.data_type().map(|ty| self.data_type(scope, Some(ty)));
        let expr = point.expr();
        let variable = match &expr {
            Some(ast::Expr::NameRef(name)) => name.name(),
            _ => None,
        };
        let name = match point.label().or(variable) {
            Some(name) if matches!(name.kind(), IDENT | ESCAPED_IDENT) => self.name(&name),
            _ => Name {
                text: "".into(),
                span: self.span(point.syntax()),
            },
        };
        let exprs = expr.map(|expr| self.expr_of(scope, expr));
        let iff = point.condition().map(|it| self.paren(scope, &it));
        let (bins, options) = self.bins(points, point.bins_block());
        let point = Coverpoint {
            keyword: COVERPOINT_KW,
            ty,
            exprs: exprs.into_iter().collect(),
            iff,
            bins,
            options,
        };
        self.declare(points, name, SymbolKind::Coverpoint(point));
    }

    /// A cross, declared in `points`, where what it crosses is found.
    fn cross(&mut self, points: ScopeId, cross: &ast::Cross) {
        let name = match cross.label() {
            Some(label) => self.name(&label),
            None => Name {
                text: "".into(),
                span: self.span(cross.syntax()),
            },
        };
        let exprs = (cross.points())
            .map(|point| self.expr_of(points, point))
            .collect();
        let iff = cross.condition().map(|it| self.paren(points, &it));
        let (bins, options) = self.bins(points, cross.bins_block());
        let cross = Coverpoint {
            keyword: CROSS_KW,
            ty: None,
            exprs,
            iff,
            bins,
            options,
        };
        self.declare(points, name, SymbolKind::Coverpoint(cross));
    }

    /// The bins and options of a cover point or a cross, names in `points`.
    /// Anything else it holds, as a cross's function, is declared there.
    fn bins(
        &mut self,
        points: ScopeId,
        block: Option<ast::BinsBlock>,
    ) -> (Vec<Bins>, Vec<CoverOption>) {
        let (mut bins, mut options) = (Vec::new(), Vec::new());
        for item in block.iter().flat_map(|block| block.items()) {
            let ast::Item::Bins(it) = item else {
                match self.option(points, &item) {
                    Some(option) => options.push(option),
                    None => self.member(points, item),
                }
                continue;
            };
            let (Some(keyword), Some(name)) = (it.keyword(), it.name()) else {
                self.opaque_member(points, it.syntax());
                continue;
            };
            let mut values: Vec<ExprId> = (it.count().into_iter())
                .map(|count| self.expr_of(points, count))
                .collect();
            match it.range_list() {
                Some(list) if list.l_brace_token().is_some() => {
                    values.extend(list.exprs().map(|value| self.expr_of(points, value)));
                }
                // A list a macro writes is not known.
                Some(list) => values.push(self.opaque_expr(list.syntax())),
                None => {}
            }
            for set in it.trans_sets() {
                values.extend(set.exprs().map(|value| self.expr_of(points, value)));
            }
            values.extend(
                (it.value().into_iter().chain(it.matches())).map(|v| self.expr_of(points, v)),
            );
            let with = it.with_clause().and_then(|with| {
                let condition = with.condition()?;
                let item = self.item(&with);
                Some(self.with(points, item, &condition))
            });
            let iff = it.condition().map(|it| self.paren(points, &it));
            bins.push(Bins {
                keyword: keyword.kind(),
                name: self.name(&name),
                values,
                with,
                iff,
            });
        }
        (bins, options)
    }

    /// `option.name = value` or `type_option.name = value`, if `item` is
    /// one.
    fn option(&mut self, scope: ScopeId, item: &ast::Item) -> Option<CoverOption> {
        let ast::Item::ExprStmt(stmt) = item else {
            return None;
        };
        let assignment = stmt.assignment()?;
        let ast::Expr::FieldExpr(field) = assignment.lhs()? else {
            return None;
        };
        let ast::Expr::NameRef(base) = field.expr()? else {
            return None;
        };
        let shared = match base.name()?.text() {
            "option" => false,
            "type_option" => true,
            _ => return None,
        };
        let name = self.name(&field.field()?);
        let value = self.expr_of(scope, assignment.rhs()?);
        Some(CoverOption {
            shared,
            name,
            value,
        })
    }

    /// A modport, each entry with the direction of the one before it unless
    /// it writes its own.
    fn modport(&mut self, scope: ScopeId, modport: &ast::Modport) {
        let (Some(name), Some(list)) = (modport.name(), modport.port_list()) else {
            return self.other(scope, modport.syntax(), modport.name());
        };
        let mut items = Vec::new();
        let mut direction = None;
        for child in list.syntax().children() {
            let Some(port) = ast::Port::cast(child.clone()) else {
                self.opaque_member(scope, &child);
                continue;
            };
            let tokens: Vec<SyntaxToken> = (port.syntax().children_with_tokens())
                .filter_map(SyntaxElement::into_token)
                .collect();
            let written = tokens.iter().map(|token| token.kind()).find(|kind| {
                matches!(kind, INPUT_KW | OUTPUT_KW | INOUT_KW | REF_KW | CLOCKING_KW)
            });
            direction = written.or(direction);
            let signal = port.declarator().and_then(|it| it.name());
            // `.name(expr)` names what it stands for.
            let named = tokens
                .iter()
                .find(|token| matches!(token.kind(), IDENT | ESCAPED_IDENT));
            let (Some(direction), Some(token)) = (direction, signal.as_ref().or(named)) else {
                self.opaque_member(scope, &child);
                continue;
            };
            let item = self.name(token);
            let value = match (&signal, port.arg_list()) {
                (Some(_), _) => self.expr(ExprKind::Name(item.text.clone()), item.span),
                (None, Some(list)) => match self.args(scope, &list).as_slice() {
                    [Arg::Positional(Some(value))] => *value,
                    _ => self.opaque_expr(list.syntax()),
                },
                (None, None) => self.opaque_expr(port.syntax()),
            };
            items.push(ModportItem {
                direction,
                name: item,
                value,
            });
        }
        let name = self.name(&name);
        self.declare(scope, name, SymbolKind::Modport(items));
    }

    /// A clocking block; or `default clocking cb;`, which names one.
    fn clocking(&mut self, scope: ScopeId, decl: &ast::ClockingDecl) {
        let Some(event) = decl.event_control() else {
            match decl.name() {
                Some(name) => {
                    let name = self.name(&name);
                    let at = self.expr(ExprKind::Name(name.text), name.span);
                    self.push(scope, Member::DefaultClocking(at));
                }
                None => self.opaque_member(scope, decl.syntax()),
            }
            return;
        };
        let event = self.event(scope, &event);
        let body = self.scope(ScopeKind::Clocking, Some(scope), None);
        let mut signals = Vec::new();
        let mut skews = Vec::new();
        for item in decl.items() {
            let ast::Item::ClockingItem(item) = item else {
                self.member(body, item);
                continue;
            };
            let written =
                |kind| (item.syntax().children_with_tokens()).any(|element| element.kind() == kind);
            let direction = match (written(INPUT_KW), written(OUTPUT_KW), written(INOUT_KW)) {
                (true, true, _) | (_, _, true) => Some(INOUT_KW),
                (true, false, false) => Some(INPUT_KW),
                (false, true, false) => Some(OUTPUT_KW),
                (false, false, false) => None,
            };
            skews.extend(item.delay_controls().map(|delay| self.delay(scope, &delay)));
            for declarator in item.declarators() {
                let (Some(direction), Some(signal)) = (direction, declarator.name()) else {
                    self.opaque_member(body, declarator.syntax());
                    continue;
                };
                let name = self.name(&signal);
                let signal = match declarator.init() {
                    Some(init) => self.type_or_expr(scope, init),
                    None => self.expr(ExprKind::Name(name.text.clone()), name.span),
                };
                signals.push(ClockingSignal {
                    direction,
                    name,
                    signal,
                });
            }
        }
        let qualifier = (decl.default_token().or(decl.global_token())).map(|it| it.kind());
        let clocking = Clocking {
            qualifier,
            event,
            scope: body,
            signals,
            skews,
        };
        // A default or global clocking block may be left unnamed.
        let name = match (decl.name(), decl.clocking_token()) {
            (Some(name), _) => self.name(&name),
            (None, keyword) => Name {
                text: "".into(),
                span: keyword.map_or(self.fallback, |it| self.token_span(&it)),
            },
        };
        let symbol = self.declare(scope, name, SymbolKind::Clocking(clocking));
        self.hir.scopes[body.index()].owner = Some(symbol);
    }

    /// Nets, variables and genvars.
    fn var_decl(&mut self, scope: ScopeId, decl: &ast::VarDecl) {
        let keyword = (decl.syntax().children_with_tokens())
            .filter_map(|element| element.into_token())
            .map(|token| token.kind())
            .find(|&kind| kind == GENVAR_KW || kind == VAR_KW || kind.is_net_type());
        let ty = self.data_type(scope, decl.data_type());
        let delay = decl.delay_control().map(|delay| self.delay(scope, &delay));
        for declarator in decl.declarators() {
            let Some(name) = declarator.name() else {
                self.opaque_member(scope, declarator.syntax());
                continue;
            };
            let name = self.name(&name);
            let init = declarator.init().map(|init| self.type_or_expr(scope, init));
            let ty = self.with_unpacked(scope, ty.clone(), Some(&declarator));
            // A non-ANSI port's type is declared apart from its direction.
            if let Some(port) = self.port_named(scope, &name.text) {
                if let SymbolKind::Port(port) = &mut self.hir.symbols[port.index()].kind {
                    port.keyword = keyword.or(port.keyword);
                    port.ty = ty;
                    port.default = init.or(port.default);
                }
                continue;
            }
            let kind = match keyword {
                Some(GENVAR_KW) => SymbolKind::Genvar(init),
                Some(kind) if kind.is_net_type() => SymbolKind::Net(Data {
                    keyword,
                    ty,
                    delay: delay.clone(),
                    init,
                }),
                _ => SymbolKind::Variable(Data {
                    keyword,
                    ty,
                    delay: delay.clone(),
                    init,
                }),
            };
            self.declare(scope, name, kind);
        }
    }

    /// The port called `name` of the definition or subroutine whose body
    /// `scope` is, if any.
    fn port_named(&self, scope: ScopeId, name: &str) -> Option<SymbolId> {
        if !matches!(
            self.hir[scope].kind,
            ScopeKind::Definition | ScopeKind::Subroutine
        ) {
            return None;
        }
        (self.ports.iter().copied()).find(|&port| &*self.hir[port].name.text == name)
    }

    /// Declares `decl`'s parameters. In a parameter port list, one that
    /// leaves out its keyword takes the one before it, and one that also
    /// leaves out its type continues it.
    fn param_decl(
        &mut self,
        scope: ScopeId,
        decl: &ast::ParamDecl,
        list: bool,
        previous: Option<ParamStyle>,
    ) -> ParamStyle {
        let keyword = decl.keyword().map(|token| token.kind());
        let local = match keyword {
            Some(LOCALPARAM_KW | SPECPARAM_KW) => true,
            // In a generate block, a package or a design element with a
            // parameter port list, `parameter` is local.
            Some(_) => !list && self.overridable != Some(scope),
            None => previous.as_ref().is_some_and(|previous| previous.local),
        };
        let written = decl.type_token().is_some() || decl.data_type().is_some();
        let (is_type, ty) = match (keyword, written, previous) {
            (None, false, Some(previous)) => (previous.is_type, previous.ty),
            _ => {
                let ty = self.data_type(scope, decl.data_type());
                (decl.type_token().is_some(), ty)
            }
        };
        let mut style = ParamStyle {
            local,
            is_type,
            ty,
            symbols: Vec::new(),
        };
        for declarator in decl.declarators() {
            let Some(name) = declarator.name() else {
                self.opaque_member(scope, declarator.syntax());
                continue;
            };
            let name = self.name(&name);
            let value = declarator.init().map(|init| self.type_or_expr(scope, init));
            let parameter = Parameter {
                local: style.local,
                is_type: style.is_type,
                ty: self.with_unpacked(scope, style.ty.clone(), Some(&declarator)),
                value,
            };
            let symbol = self.declare(scope, name, SymbolKind::Parameter(parameter));
            style.symbols.push(symbol);
        }
        style
    }

    /// `import a::b, c::*;`, and `export`.
    fn import(&mut self, scope: ScopeId, decl: &ast::ImportDecl) {
        let export = decl
            .keyword()
            .is_some_and(|keyword| keyword.kind() == EXPORT_KW);
        let tokens: Vec<SyntaxToken> = (decl.syntax().children_with_tokens())
            .filter_map(|element| element.into_token())
            .filter(|token| !token.kind().is_trivia())
            .skip(1)
            .collect();
        let name = |token: &SyntaxToken| match token.kind() {
            STAR => Some(None),
            IDENT | ESCAPED_IDENT => Some(Some(self.name(token))),
            _ => None,
        };
        let mut imports = Vec::new();
        for item in tokens.split(|token| matches!(token.kind(), COMMA | SEMICOLON)) {
            let import = match item {
                [] => continue,
                [package, colons, item] if colons.kind() == COLON_COLON => {
                    match (name(package), name(item)) {
                        (Some(written), Some(item)) => Import {
                            // `export *::*` names no package.
                            package: written.unwrap_or_else(|| Name {
                                text: "*".into(),
                                span: self.token_span(package),
                            }),
                            item,
                            export,
                        },
                        _ => return self.opaque_member(scope, decl.syntax()),
                    }
                }
                _ => return self.opaque_member(scope, decl.syntax()),
            };
            imports.push(import);
        }
        for import in imports {
            self.push(scope, Member::Import(import));
        }
    }

    /// A port list: a design element's, where a port with no direction is an
    /// `inout`, or a subroutine's, where it is an `input`.
    /// Whether every entry was a port it could lower.
    fn port_list(&mut self, scope: ScopeId, list: &ast::PortList, first: SyntaxKind) -> bool {
        let mut known = true;
        let mut previous: Option<(Option<SyntaxKind>, Option<SyntaxKind>, Type)> = None;
        for child in list.syntax().children() {
            let Some(port) = ast::Port::cast(child.clone()) else {
                self.opaque_member(scope, &child);
                known = false;
                continue;
            };
            let declarator = port.declarator();
            let Some(name) = declarator.as_ref().and_then(|it| it.name()) else {
                self.opaque_member(scope, &child);
                known = false;
                continue;
            };
            let mut direction = None;
            let mut keyword = None;
            let mut generic = None;
            let mut formal = None;
            let tokens =
                (port.syntax().children_with_tokens()).filter_map(|element| element.into_token());
            for token in tokens {
                match token.kind() {
                    INPUT_KW | OUTPUT_KW | INOUT_KW | REF_KW => direction = Some(token.kind()),
                    VAR_KW => keyword = Some(VAR_KW),
                    SEQUENCE_KW | PROPERTY_KW => formal = Some(token.kind()),
                    kind if kind.is_net_type() => keyword = Some(kind),
                    INTERFACE_KW => {
                        generic = Some(
                            self.expr(ExprKind::Keyword(INTERFACE_KW), self.token_span(&token)),
                        )
                    }
                    IDENT | ESCAPED_IDENT if generic.is_some() => {
                        let base = generic.take().expect("checked");
                        let name = self.name(&token);
                        let span = name.span;
                        generic = Some(self.expr(ExprKind::Member { base, name }, span));
                    }
                    _ => {}
                }
            }
            let written = port.data_type();
            let (direction, keyword, ty) = if let Some(interface) = generic {
                let ty = Type {
                    kind: TypeKind::Named(interface),
                    dims: Vec::new(),
                    packed: 0,
                };
                (None, None, ty)
            } else if let Some(formal) = formal {
                // A formal of a property or a sequence typed `sequence` or
                // `property`.
                let ty = Type {
                    kind: TypeKind::Builtin {
                        keyword: formal,
                        signing: None,
                    },
                    dims: Vec::new(),
                    packed: 0,
                };
                (direction.or(Some(first)), keyword, ty)
            } else if direction.is_none() && keyword.is_none() && written.is_none() {
                // A name alone continues the port before it, and a list of
                // names alone is a non-ANSI one.
                let (direction, keyword, ty) =
                    previous.clone().unwrap_or((None, None, Type::implicit()));
                (direction, keyword, ty)
            } else {
                let ty = self.data_type(scope, written);
                // Only an interface has a modport, `intf.mp`, and an
                // interface port has no direction.
                let modport = matches!(ty.kind, TypeKind::Named(path)
                    if matches!(self.hir[path].kind, ExprKind::Member { .. }));
                let inherited = previous.as_ref().and_then(|(direction, _, _)| *direction);
                let direction = match modport {
                    true => direction,
                    false => direction.or(inherited).or(Some(first)),
                };
                (direction, keyword, ty)
            };
            previous = Some((direction, keyword, ty.clone()));
            let ty = self.with_unpacked(scope, ty, declarator.as_ref());
            // A formal of a property, a sequence or a checker may default to
            // a sequence or a property.
            let default = match declarator.as_ref() {
                Some(declarator) => match (declarator.init(), declarator.actual()) {
                    (Some(init), _) => Some(self.type_or_expr(scope, init)),
                    (None, Some(actual)) => Some(self.property(scope, actual)),
                    (None, None) => None,
                },
                None => None,
            };
            let name = self.name(&name);
            let port = Port {
                direction,
                keyword,
                ty,
                default,
            };
            let symbol = self.declare(scope, name, SymbolKind::Port(port));
            self.ports.push(symbol);
        }
        known
    }

    /// `input a, b;` in a body: the direction of a non-ANSI port, or a
    /// subroutine's argument declared after its header.
    fn port_decl(&mut self, scope: ScopeId, decl: &ast::PortDecl) {
        let direction = decl.direction().map(|token| token.kind());
        let keyword = (decl.syntax().children_with_tokens())
            .filter_map(|element| element.into_token())
            .map(|token| token.kind())
            .find(|&kind| kind == VAR_KW || kind.is_net_type());
        let ty = self.data_type(scope, decl.data_type());
        for declarator in decl.declarators() {
            let Some(name) = declarator.name() else {
                self.opaque_member(scope, declarator.syntax());
                continue;
            };
            let name = self.name(&name);
            let ty = self.with_unpacked(scope, ty.clone(), Some(&declarator));
            let default = declarator.init().map(|init| self.type_or_expr(scope, init));
            if let Some(port) = self.port_named(scope, &name.text) {
                if let SymbolKind::Port(port) = &mut self.hir.symbols[port.index()].kind {
                    port.direction = direction;
                    port.keyword = keyword.or(port.keyword);
                    // `reg [3:0] q;` may come first, and its type stands.
                    let implicit = |ty: &Type| matches!(ty.kind, TypeKind::Implicit { .. });
                    if !implicit(&ty) || implicit(&port.ty) {
                        port.ty = ty;
                    }
                    port.default = default.or(port.default);
                }
                continue;
            }
            let port = Port {
                direction,
                keyword,
                ty,
                default,
            };
            let symbol = self.declare(scope, name, SymbolKind::Port(port));
            self.ports.push(symbol);
        }
    }

    /// The instances `instantiation` declares, declared in no scope yet; or
    /// `None` if it is left opaque, as a checker a package declares,
    /// `pkg::chk`, which is not looked up.
    fn instantiation(
        &mut self,
        scope: ScopeId,
        instantiation: &ast::Instantiation,
    ) -> Option<Vec<SymbolId>> {
        let scoped = (instantiation.type_ref()).is_some_and(|ty| {
            (ty.syntax().children_with_tokens()).any(|element| element.kind() == COLON_COLON)
        });
        if scoped {
            return None;
        }
        let definition = (instantiation.type_ref()).and_then(|ty| {
            (ty.syntax().children_with_tokens())
                .filter_map(|element| element.into_token())
                .find(|token| matches!(token.kind(), IDENT | ESCAPED_IDENT))
        })?;
        let definition = self.name(&definition);
        let parameters = match instantiation.arg_list() {
            Some(list) => self.args(scope, &list),
            None => Vec::new(),
        };
        let delay = (instantiation.delay_control()).map(|delay| self.delay(scope, &delay));
        let instance = Instance {
            definition,
            gate: None,
            parameters,
            delay,
            dims: Vec::new(),
            connections: Vec::new(),
        };
        Some(self.instances(scope, instance, instantiation.instances()))
    }

    /// Each of `instances` as `shared` is, with its own name, dimensions and
    /// connections, declared in no scope yet.
    fn instances(
        &mut self,
        scope: ScopeId,
        shared: Instance,
        instances: ast::AstChildren<ast::Instance>,
    ) -> Vec<SymbolId> {
        let mut symbols = Vec::new();
        for instance in instances {
            let name = match instance.name() {
                Some(name) => self.name(&name),
                None => Name {
                    text: "".into(),
                    span: self.span(instance.syntax()),
                },
            };
            let dims = instance
                .dimensions()
                .map(|dim| self.dim(scope, &dim))
                .collect();
            let connections = match instance.arg_list() {
                Some(list) => self.args(scope, &list),
                None => Vec::new(),
            };
            let instance = Instance {
                dims,
                connections,
                ..shared.clone()
            };
            symbols.push(self.symbol(name, SymbolKind::Instance(instance)));
        }
        symbols
    }

    /// `bind`, its target, and the instances it adds there. A target that is
    /// a name alone is a definition, though the standard lets it be an
    /// instance too: one written at the top, `bind u_top chk u_chk (...)`,
    /// is bound into every instance of a definition so named, if any.
    fn bind(&mut self, scope: ScopeId, bind: &ast::BindDirective) {
        let Some(instances) = (bind.instantiation()).and_then(|it| self.instantiation(scope, &it))
        else {
            return self.opaque_member(scope, bind.syntax());
        };
        let target = match bind.exprs().next() {
            Some(ast::Expr::NameRef(name)) => name.name().map(|name| self.name(&name)),
            _ => None,
        };
        let chosen = target.is_none() || bind.colon_token().is_some();
        let bind = Bind {
            target,
            chosen,
            instances,
        };
        self.push(scope, Member::Bind(bind));
    }

    /// The arguments of a call, a parameter override or a port connection.
    fn args(&mut self, scope: ScopeId, list: &ast::ArgList) -> Vec<Arg> {
        let mut args = Vec::new();
        for child in list.syntax().children() {
            let Some(arg) = ast::Arg::cast(child.clone()) else {
                let span = self.span(&child);
                let opaque = self.opaque(&child, None);
                args.push(Arg::Positional(Some(
                    self.expr(ExprKind::Opaque(opaque), span),
                )));
                continue;
            };
            let arg = match (arg.dot_token(), arg.name()) {
                (Some(_), Some(name)) if name.kind() == STAR => {
                    Arg::Wildcard(self.token_span(&name))
                }
                (Some(_), Some(name)) => {
                    let name = self.name(&name);
                    match arg.type_or_expr() {
                        None => Arg::Implicit(name),
                        Some(ast::TypeOrExpr::Expr(ast::Expr::ParenExpr(value))) => Arg::Named {
                            name,
                            value: self.paren_inner(scope, &value),
                        },
                        Some(value) => Arg::Named {
                            name,
                            value: Some(self.type_or_expr(scope, value)),
                        },
                    }
                }
                _ => Arg::Positional(
                    arg.type_or_expr()
                        .map(|value| self.type_or_expr(scope, value)),
                ),
            };
            args.push(arg);
        }
        // `()` is no argument, not one left empty.
        if let [Arg::Positional(None)] = args.as_slice() {
            args.clear();
        }
        args
    }

    // -------------------------------------------------------------- generate

    /// Adds the arms of `stmt`, a generate `if`, and of the `else if`s after
    /// it, to `arms`.
    fn generate_if(&mut self, scope: ScopeId, stmt: &ast::IfStmt, arms: &mut Vec<GenerateArm>) {
        let condition = self.condition(scope, stmt.condition(), stmt.syntax());
        let body = self.generate_body(scope, stmt.then_branch());
        arms.push(GenerateArm {
            conditions: vec![condition],
            body,
        });
        match stmt.else_branch() {
            Some(ast::Item::IfStmt(next)) => self.generate_if(scope, &next, arms),
            Some(item) => {
                let body = self.generate_body(scope, Some(item));
                arms.push(GenerateArm {
                    conditions: Vec::new(),
                    body,
                });
            }
            None => {}
        }
    }

    /// The scope of a generate construct's body: its block, or the one item
    /// written without one.
    fn generate_body(&mut self, scope: ScopeId, item: Option<ast::Item>) -> ScopeId {
        match item {
            Some(ast::Item::Block(block)) => self.generate_block(scope, &block, None),
            Some(ast::Item::LabeledStmt(stmt))
                if matches!(stmt.item(), Some(ast::Item::Block(_))) =>
            {
                let Some(ast::Item::Block(block)) = stmt.item() else {
                    unreachable!("matched above");
                };
                self.generate_block(scope, &block, stmt.label())
            }
            item => {
                let body = self.scope(ScopeKind::Generate, Some(scope), None);
                if let Some(item) = item {
                    self.member(body, item);
                }
                body
            }
        }
    }

    /// A generate block's scope, its name declared in `scope`: `label`, or
    /// the one after its `begin`.
    fn generate_block(
        &mut self,
        scope: ScopeId,
        block: &ast::Block,
        label: Option<SyntaxToken>,
    ) -> ScopeId {
        let body = self.scope(ScopeKind::Generate, Some(scope), None);
        if let Some(label) = label.or_else(|| block_label(block)) {
            let name = self.name(&label);
            let symbol = self.declare(scope, name, SymbolKind::Block(body));
            self.hir.scopes[body.index()].owner = Some(symbol);
        }
        for item in block.items() {
            self.member(body, item);
        }
        body
    }

    // ----------------------------------------------------------------- types

    pub(crate) fn data_type(&mut self, scope: ScopeId, ty: Option<ast::DataType>) -> Type {
        let Some(ty) = ty else {
            return Type::implicit();
        };
        let kind = match &ty {
            ast::DataType::TypeRef(ty) => return self.type_ref(scope, ty),
            ast::DataType::EnumType(ty) => {
                let base = self.data_type(scope, ty.type_ref().map(ast::DataType::TypeRef));
                let mut members = Vec::new();
                for variant in ty.enum_variants() {
                    let Some(name) = variant.name() else {
                        self.opaque_member(scope, variant.syntax());
                        continue;
                    };
                    let name = self.name(&name);
                    // `A[3]` declares `A0`, `A1` and `A2`; `A[2:1]`, `A2` and
                    // `A1`.
                    let names = match variant.dimension() {
                        None => vec![name],
                        Some(dim) => match enum_range(&dim) {
                            Some(indices) => (indices.map(|index| Name {
                                text: format!("{}{index}", name.text).into(),
                                span: name.span,
                            }))
                            .collect(),
                            None => {
                                self.opaque_member(scope, variant.syntax());
                                continue;
                            }
                        },
                    };
                    let mut value = variant.expr().map(|value| self.expr_of(scope, value));
                    for name in names {
                        let kind = SymbolKind::EnumMember {
                            value: value.take(),
                        };
                        members.push(self.declare(scope, name, kind));
                    }
                }
                TypeKind::Enum {
                    base: Box::new(base),
                    members,
                }
            }
            ast::DataType::StructType(ty) => {
                let fields = self.fields(scope, ty.struct_members());
                let dims: Vec<Dim> = ty.dimensions().map(|dim| self.dim(scope, &dim)).collect();
                let (packed, signing) = struct_head(ty.syntax());
                let kind = TypeKind::Struct {
                    keyword: STRUCT_KW,
                    packed,
                    signing,
                    fields,
                };
                let packed = dims.len();
                return Type { kind, dims, packed };
            }
            ast::DataType::UnionType(ty) => {
                let fields = self.fields(scope, ty.struct_members());
                let dims: Vec<Dim> = ty.dimensions().map(|dim| self.dim(scope, &dim)).collect();
                let (packed, signing) = struct_head(ty.syntax());
                let kind = TypeKind::Struct {
                    keyword: UNION_KW,
                    packed,
                    signing,
                    fields,
                };
                let packed = dims.len();
                return Type { kind, dims, packed };
            }
            ast::DataType::TypeReference(reference) => match reference.type_or_expr() {
                Some(inner) => TypeKind::TypeOf(self.type_or_expr(scope, inner)),
                None => TypeKind::Opaque(self.opaque(reference.syntax(), None)),
            },
        };
        Type {
            kind,
            dims: Vec::new(),
            packed: 0,
        }
    }

    fn fields(
        &mut self,
        scope: ScopeId,
        members: ast::AstChildren<ast::StructMember>,
    ) -> Vec<Field> {
        let mut fields = Vec::new();
        for member in members {
            let ty = self.data_type(scope, member.data_type());
            for declarator in member.declarators() {
                let Some(name) = declarator.name() else {
                    continue;
                };
                let name = self.name(&name);
                let ty = self.with_unpacked(scope, ty.clone(), Some(&declarator));
                fields.push(Field { name, ty });
            }
        }
        fields
    }

    /// A type named by keyword or by path: `logic signed [3:0]`, `pkg::t`,
    /// `intf.mp`, `C#(8)`, `virtual intf`.
    fn type_ref(&mut self, scope: ScopeId, ty: &ast::TypeRef) -> Type {
        let mut keyword = None;
        let mut signing = None;
        let mut path: Option<ExprId> = None;
        let mut separator = None;
        let mut dims = Vec::new();
        let mut known = true;
        for element in ty.syntax().children_with_tokens() {
            let token = match element {
                rowan::NodeOrToken::Node(node) => {
                    if let Some(dim) = ast::Dimension::cast(node.clone()) {
                        dims.push(self.dim(scope, &dim));
                    } else if let (Some(list), Some(callee)) = (ast::ArgList::cast(node), path) {
                        let args = self.args(scope, &list);
                        let span = self.hir[callee].span;
                        path = Some(self.expr(ExprKind::Call { callee, args }, span));
                    } else {
                        known = false;
                    }
                    continue;
                }
                rowan::NodeOrToken::Token(token) => token,
            };
            match token.kind() {
                kind if kind.is_trivia() => {}
                SIGNED_KW | UNSIGNED_KW => signing = Some(token.kind()),
                IDENT | ESCAPED_IDENT => {
                    let name = self.name(&token);
                    let span = name.span;
                    let kind = match (path, separator.take()) {
                        (None, None) => ExprKind::Name(name.text),
                        (Some(base), Some(COLON_COLON)) => ExprKind::Scoped { base, name },
                        (Some(base), Some(DOT)) => ExprKind::Member { base, name },
                        _ => {
                            known = false;
                            continue;
                        }
                    };
                    path = Some(self.expr(kind, span));
                }
                SYSTEM_IDENT if path.is_none() => {
                    let span = self.token_span(&token);
                    path = Some(self.expr(ExprKind::System(token.text().into()), span));
                }
                COLON_COLON | DOT if path.is_some() => separator = Some(token.kind()),
                VIRTUAL_KW | INTERFACE_KW | HASH => {}
                kind if kind.is_keyword() && path.is_none() && keyword.is_none() => {
                    keyword = Some(kind)
                }
                _ => known = false,
            }
        }
        let kind = match (path, keyword) {
            _ if !known || separator.is_some() => TypeKind::Opaque(self.opaque(ty.syntax(), None)),
            (Some(path), None) => TypeKind::Named(path),
            (None, Some(keyword)) => TypeKind::Builtin { keyword, signing },
            (None, None) => TypeKind::Implicit { signing },
            (Some(_), Some(_)) => TypeKind::Opaque(self.opaque(ty.syntax(), None)),
        };
        let packed = dims.len();
        Type { kind, dims, packed }
    }

    fn dim(&mut self, scope: ScopeId, dim: &ast::Dimension) -> Dim {
        let bounds: Vec<ast::TypeOrExpr> = dim.bounds().collect();
        match bounds.as_slice() {
            [] => Dim::Empty,
            [single] => Dim::Single(self.type_or_expr(scope, single.clone())),
            [hi, lo] => Dim::Range(
                self.type_or_expr(scope, hi.clone()),
                self.type_or_expr(scope, lo.clone()),
            ),
            _ => {
                let span = self.span(dim.syntax());
                let opaque = self.opaque(dim.syntax(), None);
                Dim::Single(self.expr(ExprKind::Opaque(opaque), span))
            }
        }
    }

    /// `ty` with the unpacked dimensions `declarator` adds.
    fn with_unpacked(
        &mut self,
        scope: ScopeId,
        mut ty: Type,
        declarator: Option<&ast::Declarator>,
    ) -> Type {
        for dim in declarator.into_iter().flat_map(|it| it.dimensions()) {
            let dim = self.dim(scope, &dim);
            ty.dims.push(dim);
        }
        ty
    }
}

/// How a parameter declaration declares, which the next one in a parameter
/// port list may continue.
struct ParamStyle {
    local: bool,
    is_type: bool,
    ty: Type,
    symbols: Vec<SymbolId>,
}

impl Type {
    fn implicit() -> Type {
        Type {
            kind: TypeKind::Implicit { signing: None },
            dims: Vec::new(),
            packed: 0,
        }
    }
}

/// The directives sema reads, as directives in a raw tree or as the trivia
/// an expansion leaves of them.
struct Directives {
    /// Each `` `default_nettype `` and `` `resetall ``, in order, and whether
    /// it lets a net be declared implicitly.
    nettypes: Vec<(TextSize, bool)>,
    /// Each `` `include `` not followed: every one in a raw tree, and one an
    /// expansion could not find.
    includes: Vec<TextSize>,
    /// Each `` `timescale `` and `` `resetall ``, in order, and the time
    /// unit and precision it sets; `None` for the default.
    timescales: Vec<(TextSize, Option<Timescale>)>,
}

fn directives(root: &SyntaxNode) -> Directives {
    let mut tokens = (root.descendants_with_tokens())
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() != WHITESPACE)
        .peekable();
    let mut nettypes = Vec::new();
    let mut includes = Vec::new();
    let mut timescales = Vec::new();
    while let Some(token) = tokens.next() {
        let at = token.text_range().start();
        match token.text() {
            "`resetall" => {
                nettypes.push((at, true));
                timescales.push((at, None));
            }
            "`default_nettype" => {
                let allows = tokens.next().is_none_or(|nettype| nettype.text() != "none");
                nettypes.push((at, allows));
            }
            "`include" => includes.push(at),
            "`timescale" => {
                let mut operands = String::new();
                while let Some(operand) = tokens.next_if(|it| it.kind() == DIRECTIVE_TRIVIA) {
                    operands.push_str(operand.text());
                }
                let timescale = operands.split_once('/').and_then(|(unit, precision)| {
                    Some(Timescale {
                        unit: time_unit(unit)?,
                        precision: time_unit(precision)?,
                    })
                });
                // One malformed is reported where it is written, and changes
                // nothing here.
                if timescale.is_some() {
                    timescales.push((at, timescale));
                }
            }
            _ => {}
        }
    }
    Directives {
        nettypes,
        includes,
        timescales,
    }
}

/// A time unit and a precision, each a power of ten of a second: `1ns` is
/// -9, `100ps` -10.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Timescale {
    unit: i32,
    precision: i32,
}

impl Timescale {
    /// What a design element has where nothing sets it, as most tools have
    /// it.
    const DEFAULT: Timescale = Timescale {
        unit: -9,
        precision: -9,
    };

    /// This, but for what `node`'s `timeunit` and `timeprecision` among its
    /// children declare.
    fn declared(self, node: &SyntaxNode) -> Timescale {
        let mut timescale = self;
        for decl in node.children().filter_map(ast::TimeunitDecl::cast) {
            let (Some(keyword), Some(expr)) = (decl.keyword(), decl.expr()) else {
                continue;
            };
            // `timeunit 1ns / 1ps;` sets both.
            let (unit, precision) = match &expr {
                ast::Expr::BinExpr(both) => (both.lhs(), both.rhs()),
                _ => (Some(expr.clone()), None),
            };
            let exponent = |expr: Option<ast::Expr>| time_unit(&expr?.syntax().text().to_string());
            match keyword.kind() {
                TIMEUNIT_KW => {
                    timescale.unit = exponent(unit).unwrap_or(timescale.unit);
                    timescale.precision = exponent(precision).unwrap_or(timescale.precision);
                }
                _ => timescale.precision = exponent(unit).unwrap_or(timescale.precision),
            }
        }
        timescale
    }

    /// `text`, a time literal, in this unit, rounded to this precision.
    fn scale(self, text: &str) -> Option<f64> {
        let text: String = text.chars().filter(|&c| c != '_').collect();
        let split = text.find(|c: char| c.is_ascii_alphabetic())?;
        let (magnitude, unit) = text.split_at(split);
        let magnitude: f64 = magnitude.parse().ok()?;
        let exponent = time_unit(&format!("1{unit}"))?;
        // A power of ten below one is inexact, so it divides: `15ps` is 1.5
        // steps of 10ps, not 0.15 / 0.1 of 100ps.
        let scale = |value: f64, by: i32| match by {
            0.. => value * 10f64.powi(by),
            _ => value / 10f64.powi(-by),
        };
        let steps = scale(magnitude, exponent - self.precision).round();
        Some(scale(steps, self.precision - self.unit))
    }
}

/// `1ns`, `10ps` or `100s` as a power of ten of a second.
fn time_unit(text: &str) -> Option<i32> {
    let text: String = text.split_whitespace().collect();
    let split = text.find(|c: char| c.is_ascii_alphabetic())?;
    let (magnitude, unit) = text.split_at(split);
    let magnitude = match magnitude {
        "1" => 0,
        "10" => 1,
        "100" => 2,
        _ => return None,
    };
    let unit = match unit {
        "s" => 0,
        "ms" => -3,
        "us" => -6,
        "ns" => -9,
        "ps" => -12,
        "fs" => -15,
        _ => return None,
    };
    Some(unit + magnitude)
}

/// The indices an enum name's range counts through, in order: `[n]` from 0
/// up, `[a:b]` from `a` to `b`. `None` unless the bounds are numbers, as
/// they must be, or for a range too long to be meant.
fn enum_range(dim: &ast::Dimension) -> Option<Box<dyn Iterator<Item = u64>>> {
    const LONGEST: u64 = 1 << 16;
    let number = |bound: ast::TypeOrExpr| {
        let text: String = (bound.syntax().descendants_with_tokens())
            .filter_map(SyntaxElement::into_token)
            .filter(|token| !token.kind().is_trivia())
            .map(|token| token.text().to_string())
            .collect();
        match literal(&text) {
            Literal::Bits(bits) if !bits.is_negative() => bits.to_u64(),
            _ => None,
        }
    };
    let bounds: Vec<_> = dim.bounds().collect();
    match bounds.as_slice() {
        [count] => {
            let count = number(count.clone()).filter(|&n| (1..=LONGEST).contains(&n))?;
            Some(Box::new(0..count))
        }
        [first, last] => {
            let (first, last) = (number(first.clone())?, number(last.clone())?);
            if first.abs_diff(last) >= LONGEST {
                return None;
            }
            match first <= last {
                true => Some(Box::new(first..=last)),
                false => Some(Box::new((last..=first).rev())),
            }
        }
        _ => None,
    }
}

/// Whether a struct or union is `packed`, and its signing, from the
/// keywords before its `{`.
fn struct_head(node: &SyntaxNode) -> (bool, Option<SyntaxKind>) {
    let mut packed = false;
    let mut signing = None;
    let tokens = (node.children_with_tokens())
        .map_while(|element| element.into_token())
        .take_while(|token| token.kind() != L_BRACE);
    for token in tokens {
        match token.kind() {
            PACKED_KW => packed = true,
            SIGNED_KW | UNSIGNED_KW => signing = Some(token.kind()),
            _ => {}
        }
    }
    (packed, signing)
}

/// The name after a block's `begin :` or `fork :`.
pub(crate) fn block_label(block: &ast::Block) -> Option<SyntaxToken> {
    let mut tokens = (block.syntax().children_with_tokens())
        .take_while(|element| element.as_node().is_none())
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .skip(1);
    let colon = tokens.next()?;
    let label = tokens.next()?;
    (colon.kind() == COLON && matches!(label.kind(), IDENT | ESCAPED_IDENT)).then_some(label)
}
