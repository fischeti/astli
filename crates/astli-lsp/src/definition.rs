//! `textDocument/definition`: where what stands at the cursor is declared,
//! a macro call's `` `define ``, or the header an `` `include `` read.
//!
//! It reads the snapshot through the cursor's lookup, so it answers once
//! the file's design has loaded, and only while the buffer holds the text
//! the design read.

use std::ptr;

use astli_index::Declaration;
use astli_sema::{ExprKind, Resolution, SiteKind, SymbolKind, SymbolRef};
use astli_text::Span;
use lsp_server::{RequestId, ResponseError};
use lsp_types::{Definition, DefinitionParams, DefinitionResponse, Location, LocationLink, Range};
use serde_json::Value;

use crate::cursor::{Cursor, Here, defined};
use crate::server::Server;
use crate::tasks::Cancel;
use crate::workspace_symbols;
use crate::{Analysed, Unit};

impl Server {
    /// Hands the request to the pool, with its cursor as things are now.
    pub fn definition(&mut self, id: RequestId, params: Value) {
        let cursor = self.cursor(&params);
        let links = (self.client.text_document.as_ref())
            .and_then(|it| it.definition.as_ref())
            .and_then(|it| it.link_support)
            .unwrap_or(false);
        let method = "textDocument/definition";
        self.spawn(id, method, params, move |_: DefinitionParams, cancel| {
            let Some(cursor) = cursor else {
                return Ok(None);
            };
            definition(&cursor, links, cancel)
        });
    }
}

/// A place the cursor leads to, from the range of what it was on.
struct Target {
    origin: Range,
    location: Location,
}

/// Every place what stands at `cursor` is declared, each once, in the order
/// found: as links where the client takes them, which say what at the
/// cursor led there.
fn definition(
    cursor: &Cursor,
    links: bool,
    cancel: &Cancel,
) -> Result<Option<DefinitionResponse>, ResponseError> {
    let mut found: Vec<Target> = Vec::new();
    for here in cursor.lookup(cancel)? {
        for target in targets(cursor, &here) {
            if !found.iter().any(|it| it.location == target.location) {
                found.push(target);
            }
        }
    }
    if found.is_empty() {
        return Ok(None);
    }
    if !links {
        let locations: Vec<Location> = found.into_iter().map(|it| it.location).collect();
        return Ok(Some(Definition::from(locations).into()));
    }
    let links: Vec<LocationLink> = (found.into_iter())
        .map(|it| LocationLink {
            origin_selection_range: Some(it.origin),
            target_uri: it.location.uri,
            target_range: it.location.range,
            target_selection_range: it.location.range,
        })
        .collect();
    Ok(Some(links.into()))
}

/// Where what stands at `here` leads: a name's declaration, a macro call's
/// `` `define ``, an `` `include ``'s header.
fn targets(cursor: &Cursor, here: &Here) -> Vec<Target> {
    let (design, unit) = (here.design, here.unit);
    let origins = &unit.origins;
    let mut found = Vec::new();
    let mut lead = |from: Span, to: Option<Location>| {
        if let Some(location) = to {
            let origin = cursor.range(origins, from);
            found.push(Target { origin, location });
        }
    };
    if let Some(call) = here.call {
        let expansion = origins.expansion(call);
        let at = expansion.def.map(|it| defined(origins, it));
        lead(
            expansion.name,
            at.and_then(|it| cursor.location(origins, it)),
        );
    }
    // A macro's name in its `define is its declaration.
    if let Some(name) = here.define {
        lead(name, cursor.location(origins, name));
    }
    if let Some(header) = here.include {
        let directive = origins.included_from(header).expect("a header is included");
        let start = Span::point(header, 0);
        lead(directive, cursor.location(origins, start));
    }
    // A positional connection or override leads to its formal only from
    // where no name inside it stands.
    let named = (here.sites.iter()).any(|it| !matches!(it.kind, SiteKind::Positional { .. }));
    let hir = &design.elaboration.design()[unit.file];
    for site in &here.sites {
        let symbol = match site.kind {
            // A name an opaque region spells may mean several things.
            SiteKind::Use(expr, _) if matches!(hir[expr].kind, ExprKind::Loose(_)) => continue,
            SiteKind::Use(_, Resolution::Declared(it)) => Some(it),
            // An implicit net is declared by its first use.
            SiteKind::Use(_, Resolution::Implicit(first)) => {
                lead(site.span, cursor.location(origins, hir[first].span));
                continue;
            }
            SiteKind::Use(..) => None,
            SiteKind::Declaration(_) | SiteKind::Field => {
                lead(site.span, cursor.location(origins, site.span));
                continue;
            }
            SiteKind::Definition { definition, .. } => definition,
            SiteKind::Formal { formal, .. } => formal,
            SiteKind::Positional { formal, .. } if !named => formal,
            SiteKind::Positional { .. } => None,
            SiteKind::Package(it) | SiteKind::Target(_, it) => it,
            SiteKind::Imported(Resolution::Declared(it)) => Some(it),
            SiteKind::Imported(_) => None,
            SiteKind::Member { expr, scope } => {
                let elaboration = &design.elaboration;
                let member = elaboration.member(elaboration.body_at(unit.file, scope), expr);
                if let Some(member) = member
                    && let Some(declared) = design.unit(member.file)
                {
                    lead(site.span, cursor.location(&declared.origins, member.span));
                }
                continue;
            }
        };
        if let Some(symbol) = symbol {
            for location in declared(cursor, design, symbol) {
                lead(site.span, Some(location));
            }
        }
    }
    found
}

/// Where `symbol` is declared: its name, or for a definition or a package
/// that several files declare, each declaration's name.
fn declared(cursor: &Cursor, design: &Analysed, symbol: SymbolRef) -> Vec<Location> {
    let Some(unit) = design.unit(symbol.file) else {
        return Vec::new();
    };
    let declared = design.elaboration.design().symbol(symbol);
    let Some(location) = cursor.location(&unit.origins, declared.name.span) else {
        return Vec::new();
    };
    if !matches!(declared.kind, SymbolKind::Definition { .. }) {
        return vec![location];
    }
    // The index names the place of each declaration as a line and a
    // column, and its group is the symbol's if one of them is its place.
    let name = &*declared.name.text;
    let origins = &unit.origins;
    let span = origins.spelled(declared.name.span);
    let (path, at) = (
        origins.path(span.src_id),
        origins.line_col(span.src_id, span.start),
    );
    let redeclared = design.index.redeclared(&[symbol.file.index()]);
    let group = redeclared.into_iter().find(|group| {
        group.name == name
            && (group.declarations.iter())
                .any(|it| Some(it.location.path.as_path()) == path && it.location.at == at)
    });
    let Some(group) = group else {
        return vec![location];
    };
    (group.declarations.iter())
        .filter_map(|&declaration| {
            let origins = &declaring(design, declaration)?.origins;
            let range = workspace_symbols::range(declaration, origins, cursor.encoding);
            let uri = cursor.uri(&declaration.location.path)?;
            Some(Location { uri, range })
        })
        .collect()
}

/// The unit whose summary in the index holds `declaration`.
fn declaring<'a>(design: &'a Analysed, declaration: &Declaration) -> Option<&'a Unit> {
    // A design's summaries are its units, in the same order.
    let summaries = design.index.summaries().iter().zip(&design.units);
    summaries
        .into_iter()
        .find(|(summary, _)| (summary.declarations.iter()).any(|it| ptr::eq(it, declaration)))
        .map(|(_, unit)| unit)
}
