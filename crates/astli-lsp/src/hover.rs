//! What hover says of the name or the macro call under the cursor, as
//! markdown short enough for a tooltip.
//!
//! A name is a [`Site`] of its file, and what is said of it comes from the
//! design: its kind, its type as elaborated, a parameter's value, what an
//! instance's overrides evaluate to. What the design does not keep comes
//! from the raw tree of the file that declares it: the declaration as
//! written, and the comment above it or after it on its line, both shown
//! as code. A value depends on the instance, and which one a reader means
//! is state the protocol has no place for, so a value is said at the
//! defaults of the definition around it and, where there are tops, once for
//! each value the bodies they build of it take, written by its type, as
//! [`value`] says.

use std::collections::HashMap;
use std::fmt::Write;
use std::io;
use std::path::Path;
use std::ptr;

use astli_parse::SyntaxTree;
use astli_preproc::{Session, render};
use astli_sema::{
    BodyId, FileId, MemberTy, Packed, Parameter, ParameterKind, Resolution, ScopeId, ScopeKind,
    Shape, Site, SiteKind, SymbolKind, SymbolRef, Ty,
};
use astli_syntax::{
    SyntaxKind, SyntaxKind::*, SyntaxNode, SyntaxToken, first_significant, last_significant,
};
use astli_text::{ExpansionId, Reader, SourceId, Span};
use lsp_server::{RequestId, ResponseError};
use lsp_types::{Contents, HoverParams, MarkupContent, MarkupKind};
use rowan::{TextRange, TextSize};
use serde_json::Value as Json;

use crate::cursor::Cursor;
use crate::server::Server;
use crate::snapshot::{Analysed, Unit};
use crate::tasks::Cancel;

/// At most this many lines of a declaration or a `` `define `` as written.
const LINES: usize = 8;
/// At most this many lines of the comment above a declaration, those
/// nearest it.
const COMMENT: usize = 6;
/// At most this many lines of what a macro call expands to, laid out.
const EXPANDED: usize = 20;
/// At most this many characters of what a macro call expands to, when it
/// does not lay out as module items.
const CHARACTERS: usize = 400;
/// At most this many of the values the tops build a parameter with.
const VALUES: usize = 4;

impl Server {
    /// Hands the request to the pool, with its cursor as things are now.
    pub fn hover(&mut self, id: RequestId, params: Json) {
        let cursor = self.cursor(&params);
        let method = "textDocument/hover";
        self.spawn(id, method, params, move |_: HoverParams, cancel| {
            let Some(cursor) = cursor else {
                return Ok(None);
            };
            hover(&cursor, cancel)
        });
    }
}

/// What stands at `cursor`, in each unit that reads the file: a macro
/// call, the header an `` `include `` read, or the names there; each
/// distinct thing said once, over the range of the first.
fn hover(cursor: &Cursor, cancel: &Cancel) -> Result<Option<lsp_types::Hover>, ResponseError> {
    let mut said: Vec<String> = Vec::new();
    let mut range = None;
    let mut hover: Option<Hover> = None;
    for here in cursor.lookup(cancel)? {
        cancel.check()?;
        // The units of one design share what is parsed of their files.
        let hover = match &mut hover {
            Some(it) if ptr::eq(it.analysed, here.design) => it,
            _ => hover.insert(Hover::new(here.design)),
        };
        let origins = &*here.unit.origins;
        let mut found: Vec<(String, Span)> = Vec::new();
        if let Some(call) = here.call {
            let name = origins.expansion(call).name;
            found.push((hover.macro_call(here.unit, call), name));
        } else if let Some(header) = here.include
            && let (Some(path), Some(directive)) =
                (origins.path(header), origins.included_from(header))
        {
            let path = path.strip_prefix(&cursor.snapshot.root).unwrap_or(path);
            found.push((format!("includes `{}`\n", path.display()), directive));
        } else {
            for site in &here.sites {
                found.extend(hover.site(here.unit, site).map(|it| (it, site.span)));
            }
        }
        for (text, span) in found {
            range.get_or_insert_with(|| cursor.range(origins, span));
            if !said.contains(&text) {
                said.push(text);
            }
        }
    }
    Ok(joined(&said).map(|value| lsp_types::Hover {
        contents: Contents::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range,
    }))
}

/// What hover says of several things at one place: each, a rule between
/// two; `None` for nothing.
fn joined(said: &[String]) -> Option<String> {
    let said: Vec<&str> = said.iter().map(|it| it.trim_end()).collect();
    (!said.is_empty()).then(|| said.join("\n\n---\n\n") + "\n")
}

/// What hover says, over one design, for one request: each file a
/// declaration is looked for in is parsed once.
pub struct Hover<'a> {
    analysed: &'a Analysed,
    /// Each raw tree parsed so far, by its unit's file and its source in the
    /// unit's origins.
    trees: HashMap<(FileId, SourceId), SyntaxTree>,
}

impl<'a> Hover<'a> {
    pub fn new(analysed: &'a Analysed) -> Hover<'a> {
        Hover {
            analysed,
            trees: HashMap::new(),
        }
    }

    /// What hover says of the macro call `call`, one of `unit`'s calls: the
    /// `` `define `` it read and what it expands to there, or for a
    /// built-in what it stands for.
    pub fn macro_call(&mut self, unit: &Unit, call: ExpansionId) -> String {
        let origins = &*unit.origins;
        let expansion = origins.expansion(call);
        let name = origins.slice(expansion.name).trim_start_matches('`');
        let wrote = expanded(unit, call);
        let Some(def) = expansion.def else {
            let means = match name {
                "__FILE__" => ": the path of the file it is written in",
                "__LINE__" => ": the line it is written on",
                _ => "",
            };
            let wrote = wrote.map(|it| format!("\n\nexpands here to `{it}`"));
            return format!(
                "built-in macro `{name}`{means}{}\n",
                wrote.unwrap_or_default()
            );
        };
        let mut out = format!("macro `{name}`\n\n");
        let def = origins.spelled(def);
        if let Some(code) = self.directive(unit, def) {
            fenced(&mut out, &code);
        }
        match wrote.as_deref() {
            None => {}
            Some("") => out += "expands here to nothing\n",
            Some(wrote) => {
                out += "expands here to\n\n";
                fenced(&mut out, &expansion_text(wrote));
            }
        }
        out
    }

    /// What `site` of `unit`'s file is, if anything sema resolved.
    pub fn site(&mut self, unit: &Unit, site: &Site) -> Option<String> {
        let elaboration = &self.analysed.elaboration;
        match site.kind {
            SiteKind::Use(_, found) | SiteKind::Imported(found) => match found {
                Resolution::Declared(at) => Some(self.symbol(at)),
                Resolution::Implicit(_) => {
                    let name = unit.origins.slice(site.span);
                    Some(format!("implicit net `{name}`: 1 bit\n"))
                }
                Resolution::Unknown | Resolution::Undeclared(_) => None,
            },
            SiteKind::Declaration(at) => Some(self.symbol(at)),
            SiteKind::Definition {
                instance,
                definition,
            } => {
                definition?;
                let at = SymbolRef {
                    file: unit.file,
                    symbol: instance,
                };
                Some(self.instance(at))
            }
            SiteKind::Formal { formal, .. } | SiteKind::Positional { formal, .. } => {
                Some(self.symbol(formal?))
            }
            SiteKind::Package(at) | SiteKind::Target(_, at) => Some(self.symbol(at?)),
            SiteKind::Member { expr, scope } => {
                let body = elaboration.body_at(unit.file, scope);
                let member = elaboration.member(body, expr)?;
                Some(self.member(&member))
            }
            SiteKind::Field => Some(self.field(unit, site.span)),
        }
    }

    /// What the declaration `at` is: its kind, name and type, as written,
    /// and a parameter's or an enum member's value.
    fn symbol(&mut self, at: SymbolRef) -> String {
        let elaboration = &self.analysed.elaboration;
        let design = elaboration.design();
        let symbol = design.symbol(at);
        if let SymbolKind::Instance(_) = symbol.kind {
            return self.instance(at);
        }
        let body = elaboration.body_at(at.file, symbol.scope);
        let name = &*symbol.name.text;
        let mut out = format!("{} `{name}`", kind(&symbol.kind));
        let typed = match &symbol.kind {
            SymbolKind::Port(_)
            | SymbolKind::Parameter(Parameter { is_type: false, .. })
            | SymbolKind::Net(_)
            | SymbolKind::Variable(_)
            | SymbolKind::Genvar(_)
            | SymbolKind::Typedef(_)
            | SymbolKind::EnumMember { .. }
            | SymbolKind::Subroutine { .. } => typed(&elaboration.symbol_type(body, at), name),
            _ => None,
        };
        match (&symbol.kind, typed) {
            (SymbolKind::Subroutine { .. }, Some(typed)) => write!(out, " returning {typed}"),
            (SymbolKind::Typedef(_), Some(typed)) if !typed.starts_with('`') => {
                write!(out, ", {typed}")
            }
            (_, Some(typed)) => write!(out, ": {typed}"),
            (_, None) => Ok(()),
        }
        .unwrap();
        out += "\n\n";
        if let Some(code) = self.written(at.file, symbol.name.span) {
            fenced(&mut out, &code);
        }
        let is_type = match &symbol.kind {
            SymbolKind::Parameter(parameter) => Some(parameter.is_type),
            SymbolKind::EnumMember { .. } => Some(false),
            _ => None,
        };
        if let Some(is_type) = is_type {
            let around = self.around(at.file, symbol.scope);
            let value = |body| match is_type {
                true => value::shown(&elaboration.type_of(body, at.symbol)),
                false => {
                    // An enum member's own value is its base type's, not
                    // its own name.
                    let ty = match (elaboration.type_of(body, at.symbol), &symbol.kind) {
                        (
                            Ty::Packed(Packed {
                                shape: Shape::Enum(ty),
                                ..
                            }),
                            SymbolKind::EnumMember { .. },
                        ) => Ty::Packed(ty.base.clone()),
                        (ty, _) => ty,
                    };
                    value::said(elaboration, body, &ty, &elaboration.value(body, at.symbol))
                }
            };
            out += &self.across("value", around, body, value);
        }
        out
    }

    /// What the instance `at` is: its definition, and what its overrides
    /// evaluate to in the body it builds.
    fn instance(&mut self, at: SymbolRef) -> String {
        let elaboration = &self.analysed.elaboration;
        let design = elaboration.design();
        let symbol = design.symbol(at);
        let SymbolKind::Instance(instance) = &symbol.kind else {
            unreachable!("an instance")
        };
        let named = &*instance.definition.text;
        let definition = (instance.gate.is_none())
            .then(|| design.definition(named))
            .flatten();
        let mut out = format!("instance `{}` of ", symbol.name.text);
        match definition {
            Some(it) => write!(out, "{} `{named}`", kind(&design.symbol(it).kind)),
            None => write!(out, "`{named}`"),
        }
        .unwrap();
        out += "\n\n";
        let written = match definition {
            Some(it) => self.written(it.file, design.symbol(it).name.span),
            None => self.written(at.file, symbol.name.span),
        };
        if let Some(code) = written {
            fenced(&mut out, &code);
        }
        let overrides = elaboration.overrides(at);
        if definition.is_some() && !overrides.is_empty() {
            let holder = elaboration.body_at(at.file, symbol.scope);
            let around = self.around(at.file, symbol.scope);
            let given = |body| {
                let Some((definition, built)) = elaboration.child(body, at) else {
                    return "unknown".to_string();
                };
                let hir = &design[definition.file];
                let each = overrides.iter().map(|&(parameter, _)| {
                    let name = &hir[parameter].name.text;
                    match &hir[parameter].kind {
                        SymbolKind::Parameter(it) if it.is_type => {
                            let ty = elaboration.type_of(built, parameter);
                            format!("{name} = {}", value::shown(&ty))
                        }
                        _ => {
                            let ty = elaboration.type_of(built, parameter);
                            let value = elaboration.value(built, parameter);
                            format!("{name} = {}", value::said(elaboration, built, &ty, &value))
                        }
                    }
                });
                each.collect::<Vec<_>>().join(", ")
            };
            out += &self.across("overrides", around, holder, given);
        }
        out
    }

    /// What a struct's or union's member is.
    fn member(&mut self, member: &MemberTy) -> String {
        let mut out = format!("member `{}`", member.name);
        if let Some(typed) = typed(&member.ty, &member.name) {
            write!(out, ": {typed}").unwrap();
        }
        out += "\n\n";
        if let Some(code) = self.written(member.file, member.span) {
            fenced(&mut out, &code);
        }
        out
    }

    /// What the member declared at `span` of `unit`'s file is, found among
    /// the types of what the file declares, since a member is no symbol.
    fn field(&mut self, unit: &Unit, span: Span) -> String {
        let elaboration = &self.analysed.elaboration;
        let design = elaboration.design();
        let found = (design[unit.file].symbols()).find_map(|(symbol, it)| {
            let typed = matches!(
                it.kind,
                SymbolKind::Typedef(_)
                    | SymbolKind::Variable(_)
                    | SymbolKind::Net(_)
                    | SymbolKind::Port(_)
                    | SymbolKind::Parameter(_)
            );
            if !typed {
                return None;
            }
            let at = SymbolRef {
                file: unit.file,
                symbol,
            };
            let ty = elaboration.symbol_type(elaboration.body_at(unit.file, it.scope), at);
            declared(&ty, &|member| unit.origins.spelled(member.span) == span).cloned()
        });
        match found {
            Some(member) => self.member(&member),
            None => {
                let mut out = format!("member `{}`\n\n", unit.origins.slice(span));
                if let Some(code) = self.written(unit.file, span) {
                    fenced(&mut out, &code);
                }
                out
            }
        }
    }

    /// The definition around `scope` of `file`, whose bodies a value
    /// declared there depends on: `None` at a file's top level, in a
    /// package, which has one body, and in a definition without
    /// parameters, whose bodies all give it the same.
    fn around(&self, file: FileId, scope: ScopeId) -> Option<SymbolRef> {
        let design = self.analysed.elaboration.design();
        let hir = &design[file];
        let definition = hir.enclosing(scope, ScopeKind::Definition)?;
        let at = SymbolRef {
            file,
            symbol: hir[definition].owner?,
        };
        match &design.symbol(at).kind {
            SymbolKind::Definition {
                keyword: PACKAGE_KW,
                ..
            } => None,
            SymbolKind::Definition { parameters, .. } if parameters.is_empty() => None,
            _ => Some(at),
        }
    }

    /// What `of` says at `defaults`, the body of `around` at its defaults,
    /// and where there are tops, each distinct thing it says in a body they
    /// build of `around`, with the path of an instance that has it.
    fn across(
        &self,
        what: &str,
        around: Option<SymbolRef>,
        defaults: BodyId,
        of: impl Fn(BodyId) -> String,
    ) -> String {
        let design = self.analysed.elaboration.design();
        let at_defaults = of(defaults);
        let Some(around) = around else {
            return format!("{what} `{at_defaults}`\n");
        };
        let definition = &design.symbol(around).name.text;
        let mut out = format!("{what} `{at_defaults}` at `{definition}`'s defaults");
        let Some(built) = self.analysed.built() else {
            return out + "\n";
        };
        let mut distinct: Vec<(String, &str)> = Vec::new();
        for frame in built.bodies(design, around) {
            let value = of(frame.body);
            if !distinct.iter().any(|(it, _)| *it == value) {
                distinct.push((value, &frame.path));
            }
        }
        match distinct.as_slice() {
            [] => write!(out, ", and the tops build no `{definition}`"),
            [(only, _)] if *only == at_defaults => {
                write!(out, " and in every `{definition}` the tops build")
            }
            _ => {
                out += "; as the tops build it:\n\n";
                for (value, path) in distinct.iter().take(VALUES) {
                    writeln!(out, "- `{value}` in `{path}`").unwrap();
                }
                match distinct.len().saturating_sub(VALUES) {
                    0 => Ok(()),
                    1 => write!(out, "- and 1 more"),
                    more => write!(out, "- and {more} more"),
                }
            }
        }
        .unwrap();
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }

    /// The declaration whose name is at `span` of `file`, as written, with
    /// the comment above it or after it on its line; `None` where the raw
    /// tree has no declaration there, as for a name a macro wrote.
    fn written(&mut self, file: FileId, span: Span) -> Option<String> {
        let span = self.spelled(file, span)?;
        let (source, tree) = self.tree(file, span)?;
        let offset = TextSize::from(span.start);
        let token = tree.root().token_at_offset(offset).right_biased()?;
        let node = declaration(&token)?;
        Some(as_written(source, &node, &token))
    }

    /// The `` `define `` at `span` of `unit`'s origins, as written, with the
    /// comment above it.
    fn directive(&mut self, unit: &Unit, span: Span) -> Option<String> {
        let (source, tree) = self.tree(unit.file, span)?;
        let token = (tree.root().token_at_offset(TextSize::from(span.start))).right_biased()?;
        let start = above(&token).unwrap_or(token.text_range().start());
        let range = TextRange::new(start, TextSize::from(span.end));
        Some(cut(&dedented(source, range), LINES))
    }

    /// `span` of `file` as its unit's origins spell it.
    fn spelled(&self, file: FileId, span: Span) -> Option<Span> {
        Some(self.analysed.unit(file)?.origins.spelled(span))
    }

    /// The text and the raw tree of the source `span` of `file` is spelled
    /// in, parsed the first time it is asked for.
    fn tree(&mut self, file: FileId, span: Span) -> Option<(&str, &SyntaxTree)> {
        let origins = &self.analysed.unit(file)?.origins;
        let src = origins.spelled(span).src_id;
        let tree = self.trees.entry((file, src)).or_insert_with(|| {
            let path = origins.path(src).unwrap_or(Path::new(""));
            SyntaxTree::parse(path, origins.text(src).to_string())
        });
        Some((tree.source(), tree))
    }
}

/// What a declaration of `kind` is called.
fn kind(kind: &SymbolKind) -> String {
    let said = match kind {
        SymbolKind::Definition {
            keyword: MACROMODULE_KW,
            ..
        } => "module",
        SymbolKind::Definition { keyword, .. } => keyword.keyword_text().unwrap_or("definition"),
        SymbolKind::Class(_) => "class",
        SymbolKind::Constraint(_) => "constraint",
        SymbolKind::Port(port) => {
            return match port.direction.and_then(SyntaxKind::keyword_text) {
                Some(direction) => format!("{direction} port"),
                None => "port".to_string(),
            };
        }
        SymbolKind::Parameter(parameter) => match (parameter.kind, parameter.is_type) {
            (ParameterKind::Overridable, false) => "parameter",
            (ParameterKind::Overridable, true) => "type parameter",
            (ParameterKind::Specparam, _) => "specparam",
            (_, false) => "local parameter",
            (_, true) => "local type parameter",
        },
        SymbolKind::Net(_) => "net",
        SymbolKind::Variable(_) => "variable",
        SymbolKind::Genvar(_) => "genvar",
        SymbolKind::Typedef(_) | SymbolKind::Forward => "typedef",
        SymbolKind::EnumMember { .. } => "enum member",
        SymbolKind::Subroutine { keyword, .. } => keyword.keyword_text().unwrap_or("subroutine"),
        SymbolKind::Property { keyword, .. } => keyword.keyword_text().unwrap_or("property"),
        SymbolKind::Let { .. } => "let",
        SymbolKind::Modport(_) => "modport",
        SymbolKind::Clocking(_) => "clocking block",
        SymbolKind::Covergroup(_) => "covergroup",
        SymbolKind::Coverpoint(point) => match point.keyword {
            CROSS_KW => "cross",
            _ => "cover point",
        },
        SymbolKind::Iterator => "iterator",
        SymbolKind::Instance(_) => "instance",
        SymbolKind::Block(_) => "block",
        SymbolKind::Label(_) => "label",
        SymbolKind::Other(_) => "declaration",
    };
    said.to_string()
}

/// `ty` as code, with its width where it does not say it: `None` where
/// sema does not know it. One named `name`, a typedef's own, is its width
/// alone.
fn typed(ty: &Ty, name: &str) -> Option<String> {
    if *ty == Ty::Unknown {
        return None;
    }
    let says_width = matches!(
        ty,
        Ty::Packed(Packed {
            shape: Shape::Scalar | Shape::Vector { .. },
            ..
        })
    );
    let shown = value::shown(ty);
    let width = match ty.bits() {
        Some(1) if !says_width => Some("1 bit".to_string()),
        Some(bits) if !says_width => Some(format!("{bits} bits")),
        _ => None,
    };
    match (shown == name, width) {
        (true, Some(width)) => Some(width),
        (true, None) => None,
        (false, Some(width)) => Some(format!("`{shown}`, {width}")),
        (false, None) => Some(format!("`{shown}`")),
    }
}

/// The member `ty` declares, or a struct in it does, that `is` holds for.
fn declared<'t>(ty: &'t Ty, is: &dyn Fn(&MemberTy) -> bool) -> Option<&'t MemberTy> {
    let members = match ty {
        Ty::Struct(it)
        | Ty::Packed(Packed {
            shape: Shape::Struct(it),
            ..
        }) => &it.members,
        Ty::Array(it) => return declared(&it.element, is),
        _ => return None,
    };
    (members.iter()).find_map(|member| match is(member) {
        true => Some(member),
        false => declared(&member.ty, is),
    })
}

/// The declaration whose name `name` is: the node holding the declarator
/// or instance it names, or the definition, subroutine or other construct
/// it is the name of. `None` for a name that declares nothing as written.
fn declaration(name: &SyntaxToken) -> Option<SyntaxNode> {
    let parent = name.parent()?;
    match parent.kind() {
        DECLARATOR | INSTANCE => parent.parent(),
        MODULE_DECL | INTERFACE_DECL | PROGRAM_DECL | PACKAGE_DECL | CLASS_DECL | CHECKER_DECL
        | UDP_DECL | COVERGROUP_DECL | FUNCTION_DECL | TASK_DECL | PROPERTY_DECL
        | SEQUENCE_DECL | LET_DECL | CLOCKING_DECL | ENUM_VARIANT | MODPORT | NETTYPE_DECL => {
            Some(parent)
        }
        _ => None,
    }
}

/// `node`, the declaration of `name`, as written: a definition's or a
/// subroutine's header; of several declarators or instances, what comes
/// before the first and the one `name` names, without its connections. The
/// comment above it, or after it on its line, is shown with it.
fn as_written(source: &str, node: &SyntaxNode, name: &SyntaxToken) -> String {
    let Some(first) = first_significant(node) else {
        return String::new();
    };
    let start = first.text_range().start();
    let item = (node.children()).find(|it| matches!(it.kind(), DECLARATOR | INSTANCE));
    let (code, last) = match item {
        Some(item) => {
            let header = dedented(source, TextRange::new(start, significant_start(&item)));
            let own = (node.children())
                .find(|it| it.text_range().contains_range(name.text_range()))
                .unwrap_or(item);
            let end = (own.children())
                .find(|it| it.kind() == ARG_LIST)
                .map_or_else(|| significant_end(&own), |it| significant_start(&it));
            let named = dedented(source, TextRange::new(significant_start(&own), end));
            let code = format!("{} {}", header.trim(), named.trim());
            (code.trim().to_string(), last_significant(&own))
        }
        None => {
            // A header ends at its first `;`, a construct without one at
            // its end.
            let header = (node.children_with_tokens())
                .filter_map(|it| it.into_token())
                .find(|it| it.kind() == SEMICOLON)
                .map_or_else(|| significant_end(node), |it| it.text_range().end());
            let code = dedented(source, TextRange::new(start, header));
            (code, last_significant(node))
        }
    };
    let code = cut(&code, LINES);
    if let Some(comment) = above(&first) {
        let comment = dedented(source, TextRange::new(comment, start));
        let lines: Vec<&str> = comment.trim_end().lines().collect();
        let cut = lines.len().saturating_sub(COMMENT);
        let elided = if cut > 0 { "// …\n" } else { "" };
        return format!("{elided}{}\n{code}", lines[cut..].join("\n"));
    }
    match last.and_then(|it| after(&it)) {
        Some(comment) => format!("{code} {}", comment.text().trim_end()),
        None => code,
    }
}

/// Where `node` starts, past its leading trivia.
fn significant_start(node: &SyntaxNode) -> TextSize {
    first_significant(node).map_or(node.text_range().start(), |it| it.text_range().start())
}

/// Where `node` ends, before its trailing trivia.
fn significant_end(node: &SyntaxNode) -> TextSize {
    last_significant(node).map_or(node.text_range().end(), |it| it.text_range().end())
}

/// Where the comment directly above `first` starts: the comments on lines
/// of their own right before it, no blank line between them.
fn above(first: &SyntaxToken) -> Option<TextSize> {
    let mut start = None;
    // A comment is taken once what is before it shows it on a line of its
    // own.
    let mut pending: Option<TextSize> = None;
    let mut token = first.prev_token();
    while let Some(it) = token {
        match it.kind() {
            WHITESPACE => {
                let newlines = it.text().matches('\n').count();
                if let Some(comment) = pending.take() {
                    if newlines == 0 {
                        break;
                    }
                    start = Some(comment);
                }
                if newlines > 1 {
                    break;
                }
            }
            LINE_COMMENT | BLOCK_COMMENT => {
                if let Some(comment) = pending.replace(it.text_range().start()) {
                    start = Some(comment);
                }
            }
            _ => {
                pending = None;
                break;
            }
        }
        token = it.prev_token();
    }
    // The file's first comment has nothing before it.
    pending.or(start)
}

/// The comment after `last` on its line, past a `,` or a `;`.
fn after(last: &SyntaxToken) -> Option<SyntaxToken> {
    let mut token = last.next_token();
    while let Some(it) = token {
        match it.kind() {
            WHITESPACE if !it.text().contains('\n') => {}
            COMMA | SEMICOLON => {}
            LINE_COMMENT | BLOCK_COMMENT => return Some(it),
            _ => return None,
        }
        token = it.next_token();
    }
    None
}

/// `range` of `source`, each line after the first losing the indentation
/// the first has.
fn dedented(source: &str, range: TextRange) -> String {
    let (start, end) = (usize::from(range.start()), usize::from(range.end()));
    let line_start = source[..start].rfind('\n').map_or(0, |it| it + 1);
    let indent = start - line_start;
    let mut out = String::new();
    for (at, line) in source[start..end].split_inclusive('\n').enumerate() {
        let line = match at {
            0 => line,
            _ => {
                let blank = line.len() - line.trim_start_matches([' ', '\t']).len();
                &line[blank.min(indent)..]
            }
        };
        out += line;
    }
    out
}

/// `text`'s first `lines` lines, and an ellipsis where there were more.
fn cut(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.trim_end().lines().collect();
    let mut out = all[..all.len().min(lines)].join("\n");
    if all.len() > lines {
        out += "\n…";
    }
    out
}

/// What `call`, one of `unit`'s calls, expands to, as the load expanded
/// it: the unit is expanded again with its build, reading the texts its
/// origins hold, so that every include and every definition is what the
/// load read. `None` where the run finds another call at its place.
fn expanded(unit: &Unit, call: ExpansionId) -> Option<String> {
    let origins = &*unit.origins;
    let held = Held(
        (origins.files())
            .filter_map(|it| Some((origins.path(it)?, origins.text(it))))
            .collect(),
    );
    let text = held.0.get(unit.path.as_path())?.to_string();
    let mut session = Session::reading(&held).building((*unit.build).clone());
    let file = session.add(&unit.path, text);
    let expansion = session.expand(file);
    // The same reads expand alike, so the call is the same among the calls;
    // an id is its session's own, so its place is held to the call's.
    let at = unit.calls.iter().position(|&it| it == call)?;
    let again = *expansion.calls.get(at)?;
    let ran = session.origins();
    let (was, is) = (origins.expansion(call).call, ran.expansion(again).call);
    let same = (was.start, was.end) == (is.start, is.end)
        && origins.path(was.src_id) == ran.path(is.src_id);
    same.then(|| render(ran, &expansion.written_by(ran, again)))
}

/// The texts a unit's origins hold, by path: the files its load read.
struct Held<'a>(HashMap<&'a Path, &'a str>);

impl Reader for Held<'_> {
    fn exists(&self, path: &Path) -> bool {
        self.0.contains_key(path)
    }

    fn read(&self, path: &Path) -> io::Result<String> {
        match self.0.get(path) {
            Some(text) => Ok(text.to_string()),
            None => Err(io::ErrorKind::NotFound.into()),
        }
    }
}

/// What a macro call expands to, laid out as the formatter lays out module
/// items where it parses as them, or else as written, cut short.
fn expansion_text(wrote: &str) -> String {
    match laid_out(wrote) {
        Some(it) => cut(&it, EXPANDED),
        None => match wrote.char_indices().nth(CHARACTERS) {
            Some((at, _)) => format!("{}…", &wrote[..at]),
            None => wrote.to_string(),
        },
    }
}

/// `text` formatted as the items of a module, where it parses whole as
/// them.
fn laid_out(text: &str) -> Option<String> {
    let tree = SyntaxTree::parse("", format!("module m;\n{text}\nendmodule\n"));
    if !tree.diagnostics().is_empty() || !tree.unparsed().is_empty() {
        return None;
    }
    let formatted = astli_fmt::format(&tree, &astli_fmt::Options::default()).ok()?;
    let lines: Vec<&str> = formatted.lines().collect();
    let body = lines.get(1..lines.len().checked_sub(1)?)?;
    let indent = (body.iter())
        .filter(|it| !it.trim().is_empty())
        .map(|it| it.len() - it.trim_start().len())
        .min()?;
    let body: Vec<&str> = body
        .iter()
        .map(|it| it.get(indent..).unwrap_or(""))
        .collect();
    Some(body.join("\n").trim_matches('\n').to_string())
}

/// `code` as a fenced block, with a blank line after it.
fn fenced(out: &mut String, code: &str) {
    if code.trim().is_empty() {
        return;
    }
    // A fence longer than any run of backticks the code holds, as a macro's
    // name starts with one.
    let mut fence = "```".to_string();
    while code.contains(&fence) {
        fence.push('`');
    }
    writeln!(out, "{fence}systemverilog\n{code}\n{fence}\n").unwrap();
}

mod value;

#[cfg(test)]
mod tests;
