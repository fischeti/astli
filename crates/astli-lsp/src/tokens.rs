//! `textDocument/semanticTokens`: what each name of an open file is, as
//! only resolution knows it, for the editor to colour over what its grammar
//! already colours.
//!
//! A token is a name the file spells where it is written: a site of the
//! unit's HIR, or the `` `NAME `` of a macro call. Keywords, literals,
//! comments and operators are the grammar's, and get none, so that the
//! answer is small and the theme keeps its colours for them.
//!
//! A file is read through the first unit that holds it. A header several
//! units include may mean something else in each, but its names almost
//! always are the same kind in all of them, and asking every unit would
//! cost each of them a walk of its HIR on every edit.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use astli_sema::{ExprKind, Resolution, Site, SiteKind, SymbolKind, SymbolRef, TypeKind};
use astli_syntax::SyntaxKind::*;
use astli_text::{SourceId, Span};
use lsp_server::{ErrorCode, Request, RequestId, ResponseError};
use lsp_types::{
    Range, SemanticToken, SemanticTokens, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensParams, SemanticTokensRangeParams, TextDocumentIdentifier,
};
use serde::Deserialize;
use serde_json::Value;

use crate::buffers;
use crate::cursor::held;
use crate::position::Encoding;
use crate::server::{Server, error};
use crate::tasks::Cancel;
use crate::{Analysed, Snapshot, Unit};

/// The token types, in the legend's order: an index here is a token's type.
const TYPES: &[&str] = &[
    "namespace",
    "type",
    "class",
    "enum",
    "interface",
    "struct",
    "typeParameter",
    "variable",
    "property",
    "enumMember",
    "function",
    "macro",
];

/// The modifiers, in the legend's order: bit `n` of a token's modifiers is
/// the `n`th. A port's direction and a net have no standard modifier.
const MODIFIERS: &[&str] = &[
    "declaration",
    "readonly",
    "defaultLibrary",
    "input",
    "output",
    "inout",
    "ref",
    "net",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum Role {
    Namespace,
    Type,
    Class,
    Enum,
    Interface,
    Struct,
    TypeParameter,
    Variable,
    Property,
    EnumMember,
    Function,
    Macro,
}

const DECLARATION: u32 = 1 << 0;
const READONLY: u32 = 1 << 1;
const DEFAULT_LIBRARY: u32 = 1 << 2;
const INPUT: u32 = 1 << 3;
const OUTPUT: u32 = 1 << 4;
const INOUT: u32 = 1 << 5;
const REF: u32 = 1 << 6;
const NET: u32 = 1 << 7;

/// What a name is, as the client is told it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Kind {
    ty: Role,
    modifiers: u32,
}

impl Kind {
    fn of(ty: Role) -> Kind {
        Kind { ty, modifiers: 0 }
    }

    fn with(self, modifiers: u32) -> Kind {
        Kind {
            modifiers: self.modifiers | modifiers,
            ..self
        }
    }
}

/// What the server announces in `initialize`: the legend, the whole file,
/// and a range of it, which a client asks for a large file's visible part.
pub fn options() -> SemanticTokensOptions {
    let legend = SemanticTokensLegend {
        token_types: TYPES.iter().map(|it| it.to_string()).collect(),
        token_modifiers: MODIFIERS.iter().map(|it| it.to_string()).collect(),
    };
    SemanticTokensOptions {
        legend,
        range: Some(true.into()),
        full: Some(true.into()),
        ..SemanticTokensOptions::default()
    }
}

/// An open file, and what its tokens read, as they were when the request
/// came.
pub struct Document {
    snapshot: Arc<Snapshot>,
    path: PathBuf,
    text: Arc<str>,
    encoding: Encoding,
}

impl Server {
    /// Hands the request for the whole file to the pool.
    pub fn semantic_tokens(&mut self, id: RequestId, params: Value) {
        let document = self.document(&params);
        let method = "textDocument/semanticTokens/full";
        self.spawn(
            id,
            method,
            params,
            move |_: SemanticTokensParams, cancel| {
                let Some(document) = document else {
                    return Ok(None);
                };
                tokens(&document, None, cancel)
            },
        );
    }

    /// Hands the request for a range of the file to the pool.
    pub fn semantic_tokens_range(&mut self, id: RequestId, params: Value) {
        let document = self.document(&params);
        let method = "textDocument/semanticTokens/range";
        self.spawn(
            id,
            method,
            params,
            move |it: SemanticTokensRangeParams, cancel| {
                let Some(document) = document else {
                    return Ok(None);
                };
                tokens(&document, Some(it.range), cancel)
            },
        );
    }

    /// The open file the params name, if its design has loaded.
    fn document(&self, params: &Value) -> Option<Document> {
        let named = TextDocumentIdentifier::deserialize(params.get("textDocument")?).ok()?;
        let path = buffers::path(&named.uri)?;
        let text = self.buffers.get(&path)?.text.clone();
        Some(Document {
            snapshot: self.snapshot(&path)?,
            path,
            text,
            encoding: self.encoding,
        })
    }

    /// Asks the client to ask for every file's tokens again, as a new
    /// snapshot may say otherwise of them: a request made while the
    /// design was loading was answered `ContentModified`, and a client asks
    /// again only on the next edit, or when told to.
    pub fn refresh_tokens(&self) {
        static ASKED: AtomicU64 = AtomicU64::new(0);
        let refresh = (self.client.workspace.as_ref())
            .and_then(|it| it.semantic_tokens)
            .and_then(|it| it.refresh_support)
            .unwrap_or(false);
        if refresh {
            let id = format!("astli/refresh/{}", ASKED.fetch_add(1, Ordering::Relaxed));
            let method = "workspace/semanticTokens/refresh".to_string();
            let request = Request::new(RequestId::from(id), method, Value::Null);
            let _ = self.sender.send(request.into());
        }
    }
}

/// The tokens of `document`, or of those that overlap `within`, as the
/// first unit that holds the file reads it; `None` where none does.
/// `ContentModified` if that unit read other text than the buffer holds,
/// as a lookup at a position answers.
fn tokens(
    document: &Document,
    within: Option<Range>,
    cancel: &Cancel,
) -> Result<Option<SemanticTokens>, ResponseError> {
    let Some((design, unit, source)) = first(document, cancel)? else {
        return Ok(None);
    };
    let origins = &unit.origins;
    let text = origins.text(source);
    if text != &*document.text {
        let said = "the file changed since its design was loaded";
        return Err(error(ErrorCode::ContentModified, said));
    }
    let lines = origins.lines(source);
    let within = within.map(|it| document.encoding.bytes(text, lines, it));
    let mut found = named(design, unit, source, cancel)?;
    for &call in unit.calls.iter() {
        let expansion = origins.expansion(call);
        if expansion.name.src_id == source {
            let kind = Kind::of(Role::Macro);
            // A built-in, `` `__FILE__ ``, is defined by no directive.
            let kind = match expansion.def {
                Some(_) => kind,
                None => kind.with(DEFAULT_LIBRARY),
            };
            found.push((expansion.name, kind));
        }
    }
    found.sort_by_key(|(span, _)| (span.start, span.end));
    let mut data = Vec::with_capacity(found.len());
    let (mut line, mut start, mut end) = (0, 0, 0);
    for (span, kind) in found {
        // Two tokens may not overlap; the first kept stands.
        if span.start < end || span.start == span.end {
            continue;
        }
        if within
            .as_ref()
            .is_some_and(|it| span.end <= it.start || it.end <= span.start)
        {
            continue;
        }
        let spelled = &text[span.start as usize..span.end as usize];
        // Each token is one name, and a name holds no line break: a token
        // over several lines is one a client may not take.
        debug_assert!(!spelled.contains('\n'), "a token over lines: {spelled:?}");
        if spelled.contains('\n') {
            continue;
        }
        let at = document.encoding.position(text, lines, span.start);
        let length = document.encoding.position(text, lines, span.end).character - at.character;
        let delta_line = at.line - line;
        let delta_start = match delta_line {
            0 => at.character - start,
            _ => at.character,
        };
        data.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type: kind.ty as u32,
            token_modifiers_bitset: kind.modifiers,
        });
        (line, start, end) = (at.line, at.character, span.end);
    }
    Ok(Some(SemanticTokens::new(None, data)))
}

/// The first unit, of the first design, that holds the file, and the file
/// as it reads it: its own text, or its first inclusion of a header.
fn first<'a>(
    document: &'a Document,
    cancel: &Cancel,
) -> Result<Option<(&'a Analysed, &'a Unit, SourceId)>, ResponseError> {
    for design in &document.snapshot.designs {
        for unit in &design.units {
            cancel.check()?;
            if let Some(&source) = held(&unit.origins, &document.path).first() {
                return Ok(Some((design, unit, source)));
            }
        }
    }
    Ok(None)
}

/// Each name `unit` spells in `source`, and what it is. A name at one span
/// may be several sites: a name a macro's body writes is one in each
/// expansion, and `.name` names a port and connects a name to it. It is
/// what they agree on, the port where it is a connection's, and nothing
/// where they differ otherwise, as the body of a macro whose expansions
/// mean different things by it.
fn named(
    design: &Analysed,
    unit: &Unit,
    source: SourceId,
    cancel: &Cancel,
) -> Result<Vec<(Span, Kind)>, ResponseError> {
    let sites = unit.sites(design.elaboration.design());
    let mut found: Vec<(Span, Kind)> = Vec::new();
    let mut at: Option<Span> = None;
    let mut kinds: Vec<(Kind, bool)> = Vec::new();
    let mut flush = |at: Option<Span>, kinds: &mut Vec<(Kind, bool)>| {
        if let Some(span) = at {
            let agreed = match kinds.as_slice() {
                [] => None,
                [(kind, _), rest @ ..] if rest.iter().all(|it| it.0 == *kind) => Some(*kind),
                _ => kinds.iter().find(|(_, formal)| *formal).map(|it| it.0),
            };
            if let Some(kind) = agreed {
                found.push((span, kind));
            }
        }
        kinds.clear();
    };
    for (index, site) in sites.iter().enumerate() {
        if index % 4096 == 0 {
            cancel.check()?;
        }
        // A positional argument's site is the whole expression, whose names
        // are sites of their own.
        if site.span.src_id != source || matches!(site.kind, SiteKind::Positional { .. }) {
            continue;
        }
        if at != Some(site.span) {
            flush(at.take(), &mut kinds);
            at = Some(site.span);
        }
        if let Some(kind) = of_site(design, unit, site) {
            let formal = matches!(site.kind, SiteKind::Formal { .. });
            kinds.push((kind, formal));
        }
    }
    flush(at, &mut kinds);
    Ok(found)
}

/// What the name at `site` is: what it refers to, or declares.
fn of_site(design: &Analysed, unit: &Unit, site: &Site) -> Option<Kind> {
    let symbol = |it: SymbolRef| of_symbol(design, it);
    match site.kind {
        // A name an opaque region spells may stand for several things.
        SiteKind::Use(expr, _)
            if matches!(
                design.elaboration.design()[unit.file][expr].kind,
                ExprKind::Loose(_)
            ) =>
        {
            None
        }
        SiteKind::Use(_, Resolution::Declared(it)) => symbol(it),
        SiteKind::Use(_, Resolution::Implicit(_)) => Some(Kind::of(Role::Variable).with(NET)),
        SiteKind::Use(..) => None,
        SiteKind::Declaration(it) => symbol(it).map(|kind| kind.with(DECLARATION)),
        SiteKind::Definition { definition, .. } => definition.and_then(symbol),
        SiteKind::Formal { formal, .. } => formal.and_then(symbol),
        SiteKind::Positional { .. } => None,
        SiteKind::Package(it) | SiteKind::Target(_, it) => it.and_then(symbol),
        SiteKind::Imported(Resolution::Declared(it)) => symbol(it),
        SiteKind::Imported(_) => None,
        // A member is a struct's only where the type of what is before the
        // dot says so; otherwise it may be a method.
        SiteKind::Member { expr, scope } => {
            let elaboration = &design.elaboration;
            let body = elaboration.body_at(unit.file, scope);
            (elaboration.member(body, expr)).map(|_| Kind::of(Role::Property))
        }
        SiteKind::Field => Some(Kind::of(Role::Property).with(DECLARATION)),
    }
}

/// What a name that refers to `symbol` is. `None` for what has no type that
/// says more than the grammar does, as a constraint, or a nettype.
fn of_symbol(design: &Analysed, symbol: SymbolRef) -> Option<Kind> {
    let kind = match &design.elaboration.design().symbol(symbol).kind {
        SymbolKind::Definition { keyword, .. } => match keyword {
            PACKAGE_KW => Kind::of(Role::Namespace),
            INTERFACE_KW => Kind::of(Role::Interface),
            _ => Kind::of(Role::Class),
        },
        SymbolKind::Class(_) | SymbolKind::Covergroup(_) => Kind::of(Role::Class),
        SymbolKind::Port(port) => Kind::of(Role::Variable).with(match port.direction {
            Some(INPUT_KW) => INPUT,
            Some(OUTPUT_KW) => OUTPUT,
            Some(INOUT_KW) => INOUT,
            Some(REF_KW) => REF,
            _ => 0,
        }),
        SymbolKind::Parameter(parameter) if parameter.is_type => Kind::of(Role::TypeParameter),
        SymbolKind::Parameter(_) => Kind::of(Role::Variable).with(READONLY),
        SymbolKind::Net(_) => Kind::of(Role::Variable).with(NET),
        SymbolKind::Variable(_) | SymbolKind::Genvar(_) | SymbolKind::Iterator => {
            Kind::of(Role::Variable)
        }
        SymbolKind::Typedef(ty) => Kind::of(match ty.kind {
            TypeKind::Enum(_) => Role::Enum,
            TypeKind::Struct { .. } => Role::Struct,
            _ => Role::Type,
        }),
        SymbolKind::Forward => Kind::of(Role::Type),
        SymbolKind::EnumMember { .. } => Kind::of(Role::EnumMember).with(READONLY),
        SymbolKind::Subroutine { .. } | SymbolKind::Property { .. } | SymbolKind::Let { .. } => {
            Kind::of(Role::Function)
        }
        // What a hierarchical name reaches into: an instance, a named block
        // or a clocking block, as a package is what `::` reaches into.
        SymbolKind::Instance(_)
        | SymbolKind::Block(_)
        | SymbolKind::Label(_)
        | SymbolKind::Clocking(_) => Kind::of(Role::Namespace),
        SymbolKind::Modport(_) => Kind::of(Role::Interface),
        SymbolKind::Coverpoint(_) => Kind::of(Role::Property),
        SymbolKind::Constraint(_) | SymbolKind::Other(_) => return None,
    };
    Some(kind)
}
