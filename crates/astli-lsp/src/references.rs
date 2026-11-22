//! `textDocument/references` and `textDocument/documentHighlight`: every
//! place that names what stands at the cursor, over the whole design or in
//! the cursor's file.
//!
//! Sema goes from a use to its declaration and not back. The way back is
//! a scan of each unit's sites, which are made once a snapshot and kept
//! with the unit, for those that mean what a site at the cursor means. A
//! unit whose text does not spell the name holds no use of it, but for a
//! positional connection or override, whose instance spells its
//! definition, so only the units that spell one are resolved.

use std::path::Path;
use std::ptr;

use astli_sema::{Design, ExprId, ExprKind, FileId, Resolution, ScopeKind, Site, SiteKind};
use astli_sema::{SymbolKind, SymbolRef};
use astli_text::{Origins, Span};
use lsp_server::{RequestId, ResponseError};
use lsp_types::{DocumentHighlight, DocumentHighlightParams, Location, ReferenceParams};
use rayon::prelude::*;
use serde_json::Value;

use crate::cursor::{Cursor, Here, defined, holds_path};
use crate::server::Server;
use crate::tasks::Cancel;
use crate::{Analysed, Unit};

impl Server {
    /// Hands the request to the pool, with its cursor as things are now.
    pub fn references(&mut self, id: RequestId, params: Value) {
        let cursor = self.cursor(&params);
        let method = "textDocument/references";
        self.spawn(
            id,
            method,
            params,
            move |params: ReferenceParams, cancel| {
                let Some(cursor) = cursor else {
                    return Ok(None);
                };
                let declarations = params.context.include_declaration;
                let locations: Vec<Location> = (references(&cursor, false, cancel)?.into_iter())
                    .filter(|it| declarations || !it.declaration)
                    .map(|it| it.location)
                    .collect();
                Ok((!locations.is_empty()).then_some(locations))
            },
        );
    }

    /// Hands the request to the pool, with its cursor as things are now.
    pub fn highlights(&mut self, id: RequestId, params: Value) {
        let cursor = self.cursor(&params);
        let method = "textDocument/documentHighlight";
        self.spawn(
            id,
            method,
            params,
            move |_: DocumentHighlightParams, cancel| {
                let Some(cursor) = cursor else {
                    return Ok(None);
                };
                // The sites do not tell a write from a read, so neither is
                // marked.
                let highlights: Vec<DocumentHighlight> = (references(&cursor, true, cancel)?)
                    .into_iter()
                    .map(|it| DocumentHighlight::new(it.location.range, None))
                    .collect();
                Ok((!highlights.is_empty()).then_some(highlights))
            },
        );
    }
}

/// A place that names what the cursor is on.
struct Reference {
    location: Location,
    /// Whether it is where that is declared.
    declaration: bool,
}

/// What a site means, as two sites that name the same thing compare.
#[derive(Clone, Copy, PartialEq)]
enum Meaning<'a> {
    /// A symbol or a struct's member, by where its name is spelled: a
    /// header declares a symbol in each unit that includes it, and to a
    /// reader they are one.
    Declared { offset: u32, path: &'a Path },
    /// A definition or a package, by name: several files may declare one,
    /// and each use names the one sema chose.
    Definition(&'a str),
    /// An implicit net, by its first use.
    Implicit(FileId, ExprId),
    /// A macro, by a byte of its `` `define `` in the file at the path.
    Macro { offset: u32, path: &'a Path },
}

/// Every place what stands at `cursor` is named, each once, sorted: over
/// every unit of the design, or with `here` only those in the cursor's
/// file.
fn references(
    cursor: &Cursor,
    here: bool,
    cancel: &Cancel,
) -> Result<Vec<Reference>, ResponseError> {
    let heres = cursor.lookup(cancel)?;
    let mut found = Vec::new();
    for design in &cursor.snapshot.designs {
        let (mut meanings, mut spelled) = (Vec::new(), Vec::new());
        for at in heres.iter().filter(|it| ptr::eq(it.design, design)) {
            for meaning in meanings_at(cursor, at, &mut found, &mut spelled) {
                if !meanings.contains(&meaning) {
                    meanings.push(meaning);
                }
            }
        }
        if meanings.is_empty() {
            continue;
        }
        let units = (design.units.par_iter())
            .filter(|unit| !here || holds_path(&unit.origins, &cursor.path))
            .map(|unit| {
                cancel.check()?;
                Ok(in_unit(cursor, design, unit, &meanings, &spelled))
            })
            .collect::<Result<Vec<_>, ResponseError>>()?;
        found.extend(units.into_iter().flatten());
    }
    if here {
        let uri = cursor.uri(&cursor.path);
        found.retain(|it| Some(&it.location.uri) == uri.as_ref());
    }
    found.sort_by(|a, b| key(&a.location).cmp(&key(&b.location)));
    found.dedup_by(|later, kept| {
        let same = later.location == kept.location;
        kept.declaration |= same && later.declaration;
        same
    });
    Ok(found)
}

/// What a location sorts by: its file, then its place.
fn key(location: &Location) -> (&str, [u32; 4]) {
    let (start, end) = (location.range.start, location.range.end);
    let at = [start.line, start.character, end.line, end.character];
    (location.uri.as_str(), at)
}

/// What each name at `here` means. A macro's `` `define ``, its
/// declaration, joins `found`, since no site holds it, and the names a
/// unit's text has to spell to hold a use join `spelled`.
fn meanings_at<'a>(
    cursor: &Cursor,
    here: &Here<'a>,
    found: &mut Vec<Reference>,
    spelled: &mut Vec<&'a str>,
) -> Vec<Meaning<'a>> {
    let (design, unit) = (here.design, here.unit);
    let origins: &'a Origins = &unit.origins;
    let mut meanings = Vec::new();
    let called = (here.call)
        .and_then(|call| origins.expansion(call).def)
        .map(|def| defined(origins, def));
    if let Some(name) = called.or(here.define) {
        let name = origins.spelled(name);
        if let (Some(path), Some(location)) =
            (origins.path(name.src_id), cursor.location(origins, name))
        {
            let offset = name.start;
            meanings.push(Meaning::Macro { offset, path });
            let declaration = true;
            found.push(Reference {
                location,
                declaration,
            });
        }
    }
    let sema = design.elaboration.design();
    // A positional connection or override means its formal only where no
    // name inside it stands, as definition has it.
    let named = (here.sites.iter()).any(|it| !matches!(it.kind, SiteKind::Positional { .. }));
    for site in &here.sites {
        if named && matches!(site.kind, SiteKind::Positional { .. }) {
            continue;
        }
        let Some(meaning) = meaning(design, unit, site) else {
            continue;
        };
        meanings.push(meaning);
        match symbol(sema, unit.file, site) {
            Some(symbol) => spelled.extend(spellings(sema, symbol)),
            None => spelled.push(origins.slice(site.span)),
        }
    }
    meanings
}

/// The symbol `site` of `file` names, where it names one.
fn symbol(sema: &Design, file: FileId, site: &Site) -> Option<SymbolRef> {
    match site.kind {
        // A name an opaque region spells may mean several things, and the
        // answer has no way to say a place is uncertain.
        SiteKind::Use(expr, _) if matches!(sema[file][expr].kind, ExprKind::Loose(_)) => None,
        SiteKind::Use(_, Resolution::Declared(it))
        | SiteKind::Declaration(it)
        | SiteKind::Imported(Resolution::Declared(it))
        | SiteKind::Definition {
            definition: Some(it),
            ..
        }
        | SiteKind::Formal {
            formal: Some(it), ..
        }
        | SiteKind::Positional {
            formal: Some(it), ..
        }
        | SiteKind::Package(Some(it))
        | SiteKind::Target(_, Some(it)) => Some(it),
        _ => None,
    }
}

/// What `site` of `unit` means.
fn meaning<'a>(design: &'a Analysed, unit: &Unit, site: &Site) -> Option<Meaning<'a>> {
    let sema = design.elaboration.design();
    if let Some(symbol) = symbol(sema, unit.file, site) {
        let declared = sema.symbol(symbol);
        if let SymbolKind::Definition { .. } = declared.kind
            && declared.scope == sema[symbol.file].root()
        {
            return Some(Meaning::Definition(&declared.name.text));
        }
        return place(design, symbol.file, declared.name.span);
    }
    match site.kind {
        SiteKind::Use(expr, Resolution::Implicit(first))
            if !matches!(sema[unit.file][expr].kind, ExprKind::Loose(_)) =>
        {
            Some(Meaning::Implicit(unit.file, first))
        }
        // A member is found in the body of the definition around it at its
        // defaults, as definition finds it.
        SiteKind::Member { expr, scope } => {
            let elaboration = &design.elaboration;
            let member = elaboration.member(elaboration.body_at(unit.file, scope), expr)?;
            place(design, member.file, member.span)
        }
        SiteKind::Field => place(design, unit.file, site.span),
        _ => None,
    }
}

/// What a name declared at `span` of `file` means, by where it is spelled.
fn place(design: &Analysed, file: FileId, span: Span) -> Option<Meaning<'_>> {
    let origins = &design.unit(file)?.origins;
    let span = origins.spelled(span);
    let path = origins.path(span.src_id)?;
    let offset = span.start;
    Some(Meaning::Declared { offset, path })
}

/// Whether `site` is where what it means is declared: a declaration's
/// name, a member's in its struct, or an implicit net's first use.
fn declares(site: &Site) -> bool {
    match site.kind {
        SiteKind::Declaration(_) | SiteKind::Field => true,
        SiteKind::Use(expr, Resolution::Implicit(first)) => expr == first,
        _ => false,
    }
}

/// The names a unit's text has to spell to hold a site that names
/// `symbol`: its own, and a formal's definition's, which a positional
/// connection or override spells in its place.
fn spellings(sema: &Design, symbol: SymbolRef) -> impl Iterator<Item = &str> {
    let declared = sema.symbol(symbol);
    let hir = &sema[symbol.file];
    let formal = matches!(
        declared.kind,
        SymbolKind::Port(_) | SymbolKind::Parameter(_)
    );
    let definition = (hir.enclosing(declared.scope, ScopeKind::Definition))
        .and_then(|scope| hir[scope].owner)
        .filter(|_| formal);
    std::iter::once(&*declared.name.text).chain(definition.map(|it| &*hir[it].name.text))
}

/// The places of `unit` that mean one of `meanings`: its sites, where its
/// text spells one of `spelled`, and the macro calls it expands.
fn in_unit(
    cursor: &Cursor,
    design: &Analysed,
    unit: &Unit,
    meanings: &[Meaning],
    spelled: &[&str],
) -> Vec<Reference> {
    let origins = &unit.origins;
    let mut found = Vec::new();
    let mut push = |span: Span, declaration: bool| {
        if let Some(location) = cursor.location(origins, span) {
            found.push(Reference {
                location,
                declaration,
            });
        }
    };

    let macros: Vec<(u32, &Path)> = (meanings.iter())
        .filter_map(|it| match *it {
            Meaning::Macro { offset, path } => Some((offset, path)),
            _ => None,
        })
        .filter(|(_, path)| holds_path(origins, path))
        .collect();
    if !macros.is_empty() {
        // Every expansion, so that a call another macro's body writes is
        // found where that body spells it.
        for (_, expansion) in origins.expansions() {
            let Some(def) = expansion.def.map(|it| origins.spelled(it)) else {
                continue;
            };
            let path = origins.path(def.src_id);
            let reads = (macros.iter())
                .any(|&(offset, at)| def.start <= offset && offset < def.end && path == Some(at));
            if reads {
                push(origins.spelled(expansion.name), false);
            }
        }
    }

    let names = meanings
        .iter()
        .any(|it| !matches!(it, Meaning::Macro { .. }));
    let spells = (origins.files())
        .any(|file| (spelled.iter()).any(|name| origins.text(file).contains(name)));
    if !names || !spells {
        return found;
    }
    for site in unit.sites(design.elaboration.design()).iter() {
        // Resolving a member types the expression it is a member of, so
        // only a member spelled as a name asked about is.
        if let SiteKind::Member { .. } = site.kind
            && !spelled.contains(&origins.slice(site.span))
        {
            continue;
        }
        if meaning(design, unit, site).is_some_and(|it| meanings.contains(&it)) {
            push(site.span, declares(site));
        }
    }
    found
}
