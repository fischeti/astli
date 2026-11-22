//! `textDocument/documentSymbol`: the outline of one file, from its raw
//! tree.
//!
//! The tree keeps every conditional branch, so a declaration in a branch a
//! build would not take is listed like any other. What a macro call stands
//! for is not known without expanding it, and a `VERBATIM` region was never
//! parsed, so nothing inside either is listed.

use astli_syntax::ast::{self, AstNode};
use astli_syntax::{
    SyntaxKind, SyntaxKind::*, SyntaxNode, SyntaxToken, child_token, significant_child_tokens,
    significant_range,
};
use lsp_server::ResponseError;
use lsp_types::{
    BaseSymbolInformation, DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, Location,
    SymbolKind, Uri,
};
use rowan::{TextRange, TextSize};

use crate::buffers::Buffer;
use crate::position::Encoding;
use crate::server::Server;

impl Server {
    pub fn document_symbols(
        &mut self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>, ResponseError> {
        let uri = params.text_document.uri;
        let encoding = self.encoding;
        let hierarchical = (self.client.text_document.as_ref())
            .and_then(|it| it.document_symbol.as_ref())
            .and_then(|it| it.hierarchical_document_symbol_support)
            .unwrap_or(false);
        let Some((buffer, tree)) = self.raw(&uri) else {
            return Ok(None);
        };
        let symbols = symbols(tree.root());
        let place = Place { buffer, encoding };
        Ok(Some(match hierarchical {
            true => place.document(symbols).into(),
            false => {
                let mut flat = Vec::new();
                place.flatten(&uri, None, symbols, &mut flat);
                flat.into()
            }
        }))
    }
}

/// A symbol as the tree has it, its places in bytes.
#[derive(Debug)]
pub struct Symbol {
    pub name: String,
    pub detail: Option<String>,
    pub kind: SymbolKind,
    /// The whole construct.
    pub range: TextRange,
    /// Its name.
    pub selection: TextRange,
    pub children: Vec<Symbol>,
    port: Port,
}

/// Where a port symbol comes from, so that a port is listed once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Port {
    /// Not a port.
    None,
    /// A header's port that only names it, `a` in `module m (a);`, with
    /// neither a direction nor a type, its own or the port's before it.
    Named,
    /// A port declared in the body, `input a;`.
    Declared,
}

/// The symbols the tree at `root` declares, nested as written.
pub fn symbols(root: &SyntaxNode) -> Vec<Symbol> {
    let mut found = Vec::new();
    within(root, &mut found);
    found
}

/// The symbols of the nodes inside `node`.
fn within(node: &SyntaxNode, found: &mut Vec<Symbol>) {
    for child in node.children() {
        visit(&child, found);
    }
}

/// The symbols `node` declares, or those of the nodes inside it for one
/// that declares nothing itself.
fn visit(node: &SyntaxNode, found: &mut Vec<Symbol>) {
    let kind = node.kind();
    let (name, symbol_kind, detail) = match kind {
        // Never parsed, or not known without expanding it; neither holds a
        // declaration an expression could.
        VERBATIM | MACRO_CALL | DIRECTIVE | ATTRIBUTES | ARG_LIST | TYPE_REF | DIMENSION
        | IMPORT_DECL | EXTERN_DECL | MODPORT_METHOD => return,
        _ if ast::Expr::can_cast(kind) => return,

        MODULE_DECL => (
            named(node),
            SymbolKind::Module,
            keyword(node, MACROMODULE_KW),
        ),
        INTERFACE_DECL => (named(node), SymbolKind::Interface, None),
        PROGRAM_DECL => (named(node), SymbolKind::Module, keyword(node, PROGRAM_KW)),
        PACKAGE_DECL => (named(node), SymbolKind::Package, None),
        CLASS_DECL => (named(node), SymbolKind::Class, None),
        CHECKER_DECL => (named(node), SymbolKind::Module, keyword(node, CHECKER_KW)),
        UDP_DECL => (named(node), SymbolKind::Module, keyword(node, PRIMITIVE_KW)),
        FUNCTION_DECL | TASK_DECL => return subroutine(node, found),
        PROPERTY_DECL => (
            named(node),
            SymbolKind::Function,
            keyword(node, PROPERTY_KW),
        ),
        SEQUENCE_DECL => (
            named(node),
            SymbolKind::Function,
            keyword(node, SEQUENCE_KW),
        ),
        LET_DECL => (named(node), SymbolKind::Function, keyword(node, LET_KW)),
        COVERGROUP_DECL => (named(node), SymbolKind::Class, keyword(node, COVERGROUP_KW)),
        MODPORT => (
            named(node),
            SymbolKind::Interface,
            Some("modport".to_string()),
        ),

        TYPEDEF | NETTYPE_DECL => return typedef(node, found),
        PARAM_DECL | VAR_DECL | PORT_DECL | STRUCT_MEMBER => return declarators(node, found),
        PORT => return port(node, found),
        ENUM_VARIANT => (named(node), SymbolKind::EnumMember, None),
        INSTANTIATION | GATE_INSTANTIATION => return instances(node, found),

        PROCEDURAL_BLOCK => return process(node, found),
        BLOCK => match label(node) {
            Some(label) => (Some(label), SymbolKind::Namespace, None),
            None => return within(node, found),
        },
        LABELED_STMT => (child_token(node, &[IDENT]), SymbolKind::Namespace, None),

        _ => return within(node, found),
    };
    // A construct missing its name, as a half-typed one may be, is no
    // symbol; what it holds still is.
    let Some(name) = name else {
        return within(node, found);
    };
    let mut children = Vec::new();
    within(node, &mut children);
    ports_once(&mut children);
    found.extend(symbol(node, &name, symbol_kind, detail, children));
}

/// Drops each port a header only names that the body declares, which says
/// its direction and type. One named apart from its signal,
/// `.renamed(c)`, is listed under its own name, which the body cannot
/// declare.
fn ports_once(symbols: &mut Vec<Symbol>) {
    let declared: Vec<String> = (symbols.iter())
        .filter(|it| it.port == Port::Declared)
        .map(|it| it.name.clone())
        .collect();
    symbols.retain(|it| it.port != Port::Named || !declared.contains(&it.name));
}

/// The symbol `node` declares under `name`, unless its name is empty.
fn symbol(
    node: &SyntaxNode,
    name: &SyntaxToken,
    kind: SymbolKind,
    detail: Option<String>,
    children: Vec<Symbol>,
) -> Option<Symbol> {
    let text = name.text().trim_end();
    let range = significant_range(node)?;
    (!text.is_empty()).then(|| Symbol {
        name: text.to_string(),
        detail,
        kind,
        range,
        // An escaped name's token holds the space that ends it.
        selection: TextRange::at(name.text_range().start(), TextSize::of(text)),
        children,
        port: Port::None,
    })
}

/// The name a declaration writes as its own token, the first identifier
/// among its tokens.
fn named(node: &SyntaxNode) -> Option<SyntaxToken> {
    significant_child_tokens(node).find(|it| matches!(it.kind(), IDENT | ESCAPED_IDENT))
}

/// `keyword`'s text, if `node` writes it.
fn keyword(node: &SyntaxNode, keyword: SyntaxKind) -> Option<String> {
    child_token(node, &[keyword]).map(|it| it.text().to_string())
}

/// A block's label, after `begin :` or `fork :`.
fn label(node: &SyntaxNode) -> Option<SyntaxToken> {
    let mut tokens = significant_child_tokens(node).skip_while(|it| it.kind() != COLON);
    tokens
        .nth(1)
        .filter(|it| matches!(it.kind(), IDENT | ESCAPED_IDENT))
}

/// A function or a task. Its name is the tokens between the keyword and
/// the formals, which may be scoped, `C::f`, and the symbol's name is all
/// of them.
fn subroutine(node: &SyntaxNode, found: &mut Vec<Symbol>) {
    let task = node.kind() == TASK_DECL;
    let name: Vec<SyntaxToken> = significant_child_tokens(node)
        .skip_while(|it| !matches!(it.kind(), FUNCTION_KW | TASK_KW))
        .skip(1)
        .take_while(|it| !matches!(it.kind(), L_PAREN | SEMICOLON))
        .filter(|it| {
            matches!(
                it.kind(),
                IDENT | ESCAPED_IDENT | NEW_KW | COLON_COLON | DOT
            )
        })
        .collect();
    let (Some(first), Some(last)) = (name.first(), name.last()) else {
        return within(node, found);
    };
    let Some(range) = significant_range(node) else {
        return;
    };
    let in_class = node.parent().is_some_and(|it| it.kind() == CLASS_DECL);
    let kind = match (last.kind(), in_class) {
        (NEW_KW, _) => SymbolKind::Constructor,
        (_, true) => SymbolKind::Method,
        (_, false) => SymbolKind::Function,
    };
    let detail = match task {
        true => Some("task".to_string()),
        false => ast::FunctionDecl::cast(node.clone())
            .and_then(|it| it.type_ref())
            .map(|it| text(it.syntax())),
    };
    let mut children = Vec::new();
    within(node, &mut children);
    found.push(Symbol {
        name: name.iter().map(|it| it.text().trim_end()).collect(),
        detail,
        kind,
        range,
        selection: first.text_range().cover(last.text_range()),
        children,
        port: Port::None,
    });
}

/// A `typedef` or a `nettype`: an enum's variants and a struct's members
/// are its children.
fn typedef(node: &SyntaxNode, found: &mut Vec<Symbol>) {
    let data_type = node
        .children()
        .find(|it| ast::DataType::can_cast(it.kind()));
    let kind = match (node.kind(), data_type.as_ref().map(SyntaxNode::kind)) {
        (NETTYPE_DECL, _) => SymbolKind::TypeParameter,
        (_, Some(ENUM_TYPE)) => SymbolKind::Enum,
        (_, Some(STRUCT_TYPE | UNION_TYPE)) => SymbolKind::Struct,
        _ => SymbolKind::TypeParameter,
    };
    let detail = keyword(node, NETTYPE_KW);
    let mut children = Vec::new();
    if let Some(data_type) = &data_type {
        visit(data_type, &mut children);
    }
    let name = (node.children())
        .find_map(ast::Declarator::cast)
        .and_then(|it| it.name());
    match name {
        Some(name) => found.extend(symbol(node, &name, kind, detail, children)),
        None => found.extend(children),
    }
}

/// Each name a declaration declares, with what is written before the first
/// as its detail: `wire`, `input logic [7:0]`, `localparam int`. Each
/// symbol's range is the whole declaration, so that one of several names
/// is found from anywhere in it.
fn declarators(node: &SyntaxNode, found: &mut Vec<Symbol>) {
    let kind = match node.kind() {
        PARAM_DECL if child_token(node, &[TYPE_KW]).is_some() => SymbolKind::TypeParameter,
        PARAM_DECL => SymbolKind::Constant,
        PORT_DECL => SymbolKind::Property,
        STRUCT_MEMBER => SymbolKind::Field,
        _ => SymbolKind::Variable,
    };
    let detail = header(node);
    let port = match node.kind() {
        PORT_DECL => Port::Declared,
        _ => Port::None,
    };
    let mut children = Vec::new();
    // An anonymous struct's members, or an enum's variants, belong to the
    // declaration rather than to any one of its names.
    for data_type in node
        .children()
        .filter(|it| ast::DataType::can_cast(it.kind()))
    {
        visit(&data_type, &mut children);
    }
    let names: Vec<SyntaxToken> = (node.children())
        .filter_map(ast::Declarator::cast)
        .filter_map(|it| it.name())
        .collect();
    let last = names.len().saturating_sub(1);
    let mut children = Some(children);
    for (at, name) in names.iter().enumerate() {
        let children = if at == last { children.take() } else { None };
        let children = children.unwrap_or_default();
        let symbol = symbol(node, name, kind, detail.clone(), children);
        found.extend(symbol.map(|it| Symbol { port, ..it }));
    }
    if let Some(children) = children {
        found.extend(children);
    }
}

/// A port in a header: `input logic clk`, or `rst` after it, which takes
/// its direction and type from the port before it; `.name(signal)`.
fn port(node: &SyntaxNode, found: &mut Vec<Symbol>) {
    let port = ast::Port::cast(node.clone());
    let name = match port.as_ref().and_then(|it| it.declarator()) {
        Some(declarator) => declarator.name(),
        None => (significant_child_tokens(node))
            .skip_while(|it| it.kind() != DOT)
            .nth(1)
            .filter(|it| matches!(it.kind(), IDENT | ESCAPED_IDENT)),
    };
    let Some(name) = name else {
        return;
    };
    let detail = std::iter::once(node.clone())
        .chain(node.siblings(rowan::Direction::Prev).skip(1))
        .filter(|it| it.kind() == PORT)
        .find_map(|it| header(&it));
    let named = detail.is_none() && port.and_then(|it| it.declarator()).is_some();
    let symbol = symbol(node, &name, SymbolKind::Property, detail, Vec::new());
    found.extend(symbol.map(|it| Symbol {
        port: if named { Port::Named } else { Port::None },
        ..it
    }));
}

/// Each named instance, with the definition it instantiates, or the gate,
/// as its detail. Each symbol's range is the whole instantiation, as a
/// declaration's is.
fn instances(node: &SyntaxNode, found: &mut Vec<Symbol>) {
    let detail = match node.kind() {
        GATE_INSTANTIATION => significant_child_tokens(node)
            .find(|it| it.kind().is_keyword())
            .map(|it| it.text().to_string()),
        _ => (node.children())
            .find(|it| it.kind() == TYPE_REF)
            .map(|it| text(&it)),
    };
    for instance in node.children().filter_map(ast::Instance::cast) {
        if let Some(name) = instance.name() {
            let symbol = symbol(node, &name, SymbolKind::Object, detail.clone(), Vec::new());
            found.extend(symbol);
        }
    }
}

/// An `always`, `initial` or `final`, named by the label of the block it
/// runs, `always_ff @(posedge clk) begin : p_count`, or by its keyword.
fn process(node: &SyntaxNode, found: &mut Vec<Symbol>) {
    let Some(keyword) = ast::ProceduralBlock::cast(node.clone()).and_then(|it| it.keyword()) else {
        return within(node, found);
    };
    // The block under any timing control before it.
    let mut body = node.children().find(|it| ast::Item::can_cast(it.kind()));
    while let Some(timed) = body.as_ref().filter(|it| it.kind() == TIMING_STMT) {
        body = timed.children().find(|it| ast::Item::can_cast(it.kind()));
    }
    let labelled = body
        .filter(|it| it.kind() == BLOCK)
        .and_then(|block| Some((label(&block)?, block)));
    let mut children = Vec::new();
    let (name, detail) = match &labelled {
        Some((label, block)) => {
            within(block, &mut children);
            (label, Some(keyword.text().to_string()))
        }
        None => {
            within(node, &mut children);
            (&keyword, None)
        }
    };
    found.extend(symbol(node, name, SymbolKind::Event, detail, children));
}

/// What a declaration writes before its first name, its attributes left
/// out, its whitespace and comments a space each, and cut at a `{`, so that
/// an enum's or a struct's body is not repeated; `None` if that is nothing.
fn header(node: &SyntaxNode) -> Option<String> {
    let mut words: Vec<String> = Vec::new();
    for element in node.children_with_tokens() {
        match element {
            rowan::NodeOrToken::Node(child) if child.kind() == DECLARATOR => break,
            rowan::NodeOrToken::Node(child) if child.kind() == ATTRIBUTES => {}
            rowan::NodeOrToken::Node(child) => words.push(text(&child)),
            rowan::NodeOrToken::Token(token) if token.kind().is_trivia() => {}
            // A port named apart from its signal, `.name(signal)`.
            rowan::NodeOrToken::Token(token) if token.kind() == DOT => break,
            rowan::NodeOrToken::Token(token) => words.push(token.text().to_string()),
        }
    }
    let header = words.join(" ");
    let header = header.split('{').next().unwrap_or_default().trim();
    (!header.is_empty()).then(|| header.to_string())
}

/// `node`'s text without what surrounds it, each run of whitespace and
/// comments in it one space.
fn text(node: &SyntaxNode) -> String {
    let mut text = String::new();
    let mut space = false;
    for token in (node.descendants_with_tokens()).filter_map(|it| it.into_token()) {
        if token.kind().is_trivia() {
            space = !text.is_empty();
            continue;
        }
        if std::mem::take(&mut space) {
            text.push(' ');
        }
        text.push_str(token.text());
    }
    text
}

/// What a buffer's places are converted with.
struct Place<'a> {
    buffer: &'a Buffer,
    encoding: Encoding,
}

impl Place<'_> {
    fn range(&self, range: TextRange) -> lsp_types::Range {
        let bytes = range.start().into()..range.end().into();
        (self.encoding).range(&self.buffer.text, &self.buffer.lines, bytes)
    }

    fn document(&self, symbols: Vec<Symbol>) -> Vec<DocumentSymbol> {
        (symbols.into_iter())
            .map(|symbol| {
                let children = self.document(symbol.children);
                #[expect(deprecated, reason = "the protocol's type still has the field")]
                DocumentSymbol {
                    name: symbol.name,
                    detail: symbol.detail,
                    kind: symbol.kind,
                    tags: None,
                    deprecated: None,
                    range: self.range(symbol.range),
                    selection_range: self.range(symbol.selection),
                    children: Some(children).filter(|it| !it.is_empty()),
                }
            })
            .collect()
    }

    /// The symbols for a client that takes no hierarchy: each in order
    /// before those inside it, with the name of the one around it.
    fn flatten(
        &self,
        uri: &Uri,
        container: Option<&str>,
        symbols: Vec<Symbol>,
        flat: &mut Vec<lsp_types::SymbolInformation>,
    ) {
        for symbol in symbols {
            #[expect(deprecated, reason = "the protocol's type still has the field")]
            flat.push(lsp_types::SymbolInformation {
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range: self.range(symbol.range),
                },
                base_symbol_information: BaseSymbolInformation {
                    name: symbol.name.clone(),
                    kind: symbol.kind,
                    tags: None,
                    container_name: container.map(str::to_string),
                },
            });
            self.flatten(uri, Some(&symbol.name), symbol.children, flat);
        }
    }
}
