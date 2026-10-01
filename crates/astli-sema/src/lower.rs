//! Lowering a tree to its [`Hir`]: the declarations here, statements and
//! expressions in `body`.

mod body;

use astli_parse::Parsed;
use astli_syntax::SyntaxKind::{self, *};
use astli_syntax::ast::{self, AstNode};
use astli_syntax::{SyntaxNode, SyntaxToken};
use astli_text::Span;
use rowan::TextSize;

use crate::hir::*;

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
    };
    let root = lower.scope(ScopeKind::File, None, None);
    let mut definitions = Vec::new();
    if let Some(file) = ast::SourceFile::cast(parsed.root.clone()) {
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
        self.hir.symbols.push(Symbol { name, kind });
        let id = SymbolId(self.hir.symbols.len() as u32 - 1);
        self.push(scope, Member::Declare(id));
        id
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
            ast::Item::Instantiation(instantiation) => self.instantiation(scope, &instantiation),
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
                (label, _) => self.other(scope, stmt.syntax(), label),
            },
            // A lone `;`.
            ast::Item::ExprStmt(stmt) if stmt.syntax().children().next().is_none() => {}
            item => self.opaque_member(scope, item.syntax()),
        }
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
                    self.other(scope, modport.syntax(), modport.name());
                }
            }
            ast::Item::NettypeDecl(decl) => {
                let name = decl.declarator().and_then(|it| it.name());
                self.other(scope, decl.syntax(), name);
            }
            ast::Item::LetDecl(decl) => self.other(scope, decl.syntax(), decl.name()),
            ast::Item::PropertyDecl(decl) => self.other(scope, decl.syntax(), decl.name()),
            ast::Item::SequenceDecl(decl) => self.other(scope, decl.syntax(), decl.name()),
            ast::Item::CovergroupDecl(decl) => self.other(scope, decl.syntax(), decl.name()),
            ast::Item::ClockingDecl(decl) => self.other(scope, decl.syntax(), decl.name()),
            // Time units name nothing and read no name.
            ast::Item::TimeunitDecl(_) => {}
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
        let mut parameters = Vec::new();
        let mut parameters_known = true;
        let listed = params.is_some();
        if let Some(list) = params {
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
                        parameters_known = false;
                    }
                }
            }
        }
        let ports_known = match ports {
            Some(list) => self.port_list(body, &list, INOUT_KW),
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

    fn class(&mut self, scope: ScopeId, decl: &ast::ClassDecl) {
        match decl.name() {
            Some(name) => {
                let body = self.opaque(decl.syntax(), Some(&name));
                let name = self.name(&name);
                self.declare(scope, name, SymbolKind::Class(body));
            }
            None => self.opaque_member(scope, decl.syntax()),
        }
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
        let name = tokens().find(|token| matches!(token.kind(), IDENT | ESCAPED_IDENT));
        // A class method defined outside its class, `C::f`, or a
        // constructor, belongs to a class, which is not lowered.
        let in_class = tokens().any(|token| matches!(token.kind(), COLON_COLON | NEW_KW));
        let (Some(keyword), Some(name), false) = (keyword, name, in_class) else {
            return self.opaque_member(scope, node);
        };
        let name = self.name(&name);
        let body = self.scope(ScopeKind::Subroutine, Some(scope), None);
        let kind = SymbolKind::Subroutine {
            keyword: keyword.kind(),
            scope: body,
            ports: Vec::new(),
            returns,
            body: Vec::new(),
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

    /// Nets, variables and genvars.
    fn var_decl(&mut self, scope: ScopeId, decl: &ast::VarDecl) {
        let keyword = (decl.syntax().children_with_tokens())
            .filter_map(|element| element.into_token())
            .map(|token| token.kind())
            .find(|&kind| kind == GENVAR_KW || kind == VAR_KW || kind.is_net_type());
        let ty = self.data_type(scope, decl.data_type());
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
                Some(kind) if kind.is_net_type() => SymbolKind::Net(Data { keyword, ty, init }),
                _ => SymbolKind::Variable(Data { keyword, ty, init }),
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
            let tokens =
                (port.syntax().children_with_tokens()).filter_map(|element| element.into_token());
            for token in tokens {
                match token.kind() {
                    INPUT_KW | OUTPUT_KW | INOUT_KW | REF_KW => direction = Some(token.kind()),
                    VAR_KW => keyword = Some(VAR_KW),
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
                };
                (None, None, ty)
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
            let default = (declarator.as_ref().and_then(|it| it.init()))
                .map(|init| self.type_or_expr(scope, init));
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

    fn instantiation(&mut self, scope: ScopeId, instantiation: &ast::Instantiation) {
        let definition = (instantiation.type_ref()).and_then(|ty| {
            (ty.syntax().children_with_tokens())
                .filter_map(|element| element.into_token())
                .find(|token| matches!(token.kind(), IDENT | ESCAPED_IDENT))
        });
        let Some(definition) = definition else {
            return self.opaque_member(scope, instantiation.syntax());
        };
        let definition = self.name(&definition);
        let parameters = match instantiation.arg_list() {
            Some(list) => self.args(scope, &list),
            None => Vec::new(),
        };
        for instance in instantiation.instances() {
            let Some(name) = instance.name() else {
                self.opaque_member(scope, instance.syntax());
                continue;
            };
            let name = self.name(&name);
            let dims = instance
                .dimensions()
                .map(|dim| self.dim(scope, &dim))
                .collect();
            let connections = match instance.arg_list() {
                Some(list) => self.args(scope, &list),
                None => Vec::new(),
            };
            let instance = Instance {
                definition: definition.clone(),
                parameters: parameters.clone(),
                dims,
                connections,
            };
            self.declare(scope, name, SymbolKind::Instance(instance));
        }
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
                    let value = variant.expr().map(|value| self.expr_of(scope, value));
                    members.push(self.declare(scope, name, SymbolKind::EnumMember { value }));
                }
                TypeKind::Enum {
                    base: Box::new(base),
                    members,
                }
            }
            ast::DataType::StructType(ty) => {
                let fields = self.fields(scope, ty.struct_members());
                let dims = ty.dimensions().map(|dim| self.dim(scope, &dim)).collect();
                let kind = TypeKind::Struct {
                    keyword: STRUCT_KW,
                    fields,
                };
                return Type { kind, dims };
            }
            ast::DataType::UnionType(ty) => {
                let fields = self.fields(scope, ty.struct_members());
                let dims = ty.dimensions().map(|dim| self.dim(scope, &dim)).collect();
                let kind = TypeKind::Struct {
                    keyword: UNION_KW,
                    fields,
                };
                return Type { kind, dims };
            }
            ast::DataType::TypeReference(reference) => match reference.type_or_expr() {
                Some(inner) => TypeKind::TypeOf(self.type_or_expr(scope, inner)),
                None => TypeKind::Opaque(self.opaque(reference.syntax(), None)),
            },
        };
        Type {
            kind,
            dims: Vec::new(),
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
        Type { kind, dims }
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
}

fn directives(root: &SyntaxNode) -> Directives {
    let mut tokens = (root.descendants_with_tokens())
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() != WHITESPACE);
    let mut nettypes = Vec::new();
    let mut includes = Vec::new();
    while let Some(token) = tokens.next() {
        let at = token.text_range().start();
        match token.text() {
            "`resetall" => nettypes.push((at, true)),
            "`default_nettype" => {
                let allows = tokens.next().is_none_or(|nettype| nettype.text() != "none");
                nettypes.push((at, allows));
            }
            "`include" => includes.push(at),
            _ => {}
        }
    }
    Directives { nettypes, includes }
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
