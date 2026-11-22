//! What stands at a place of a file: each name its HIR holds, by where the
//! name is spelled.
//!
//! A cursor is a file and an offset, and sema answers by id. Between them
//! is a table of every name a file's HIR holds, sorted by where it is
//! spelled, so that an offset is a binary search away from what stands
//! there: a use and what it refers to, a declaration, or a name that is no
//! expression, as an instance's definition or the formal a connection
//! names.
//!
//! A name is spelled once however often it is used: a name a macro wrote
//! is spelled in the `` `define ``'s body or in an argument of the call,
//! so one place may stand for a use in each expansion. Each is in the
//! table, at the same span.

use astli_syntax::name_text;
use astli_text::{Origins, SourceId, Span};

use crate::bind;
use crate::design::{BindTarget, Design, FileId, SymbolRef};
use crate::hir::*;
use crate::resolve::Resolution;

/// A name a file spells, and what it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Site {
    /// Where the name is written, [`Origins::spelled`]: in a header the
    /// file includes, in a `` `define ``'s body, in a macro call's
    /// argument. A positional argument's is the whole expression.
    pub span: Span,
    pub kind: SiteKind,
}

/// What a [`Site`] is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SiteKind {
    /// A name used, and what it refers to: an expression
    /// [`Names::expr`](crate::Names::expr) resolves, whose span is the
    /// name's alone, `b` of `a.b` and `p::b`.
    Use(ExprId, Resolution),
    /// A declaration's name.
    Declaration(SymbolRef),
    /// The definition an instance names; `None` where no file declares
    /// one by that name.
    Definition {
        instance: SymbolId,
        definition: Option<SymbolRef>,
    },
    /// The formal of its definition that an instance's override or
    /// connection names, `.name(…)` or `.name`: a parameter or a port.
    /// `None` where no formal has the name, or no definition is known.
    Formal {
        instance: SymbolId,
        formal: Option<SymbolRef>,
    },
    /// The formal a positional override or connection is given to, the
    /// site the expression given. `None` past the last formal, or where no
    /// definition is known.
    Positional {
        instance: SymbolId,
        formal: Option<SymbolRef>,
    },
    /// The package an `import` or `export` names; `None` where no file
    /// declares one.
    Package(Option<SymbolRef>),
    /// The item an `import` or `export` names, as its package declares or
    /// exports it.
    Imported(Resolution),
    /// The definition a `bind` binds into; `None` where its name is an
    /// instance's, which a [`Use`](SiteKind::Use) at the same span
    /// resolves, or nothing's.
    Target(BindId, Option<SymbolRef>),
    /// `b` of `a.b` where `b` is no symbol: a struct's or union's member, or
    /// a value's method. Which depends on the type of `a`, so on a body:
    /// [`Elaboration::member`](crate::Elaboration::member) finds it in one
    /// [`Elaboration::body_at`](crate::Elaboration::body_at) gives for the
    /// scope it stands in.
    Member { expr: ExprId, scope: ScopeId },
    /// A struct's or union's member, declared: no symbol, but where a
    /// [`MemberTy`](crate::MemberTy) of its type is declared.
    Field,
}

/// Every name one file's HIR holds, by where it is spelled.
#[derive(Debug, Clone, Default)]
pub struct Sites {
    /// Each a name: two are at the same span or apart.
    names: Vec<Site>,
    /// Each positional argument of an instance, which holds the names
    /// written in it: two are at the same span or apart.
    arguments: Vec<Site>,
}

impl Sites {
    /// Each name `file` of `design` spells, by where it is spelled.
    /// `origins` is what the file's spans resolve against, that of the
    /// session it was lowered in, which the design does not hold; the
    /// caller keeps the table beside them.
    pub fn new(design: &Design, file: FileId, origins: &Origins) -> Sites {
        sites(design, file, origins)
    }

    /// What stands at `offset` of `file`, a file or header as written in
    /// the session the HIR was lowered in: each site whose span holds it,
    /// a cursor just past a name being on it. Several where a macro wrote
    /// the name more than once, or where a name is a positional argument,
    /// or `.name` connects a name to a port of that name.
    pub fn at(&self, file: SourceId, offset: u32) -> impl Iterator<Item = &Site> {
        (holding(&self.names, file, offset).iter())
            .chain(holding(&self.arguments, file, offset))
            .filter(move |site| site.span.start <= offset && offset <= site.span.end)
    }

    /// Every site, names in the order spelled, then positional arguments.
    pub fn iter(&self) -> impl Iterator<Item = &Site> {
        self.names.iter().chain(&self.arguments)
    }
}

/// The sites of `sorted` that start where the last one starting at or
/// before `offset` of `file` does, which alone may hold it, as two are at
/// the same span or apart.
fn holding(sorted: &[Site], file: SourceId, offset: u32) -> &[Site] {
    let key = |site: &Site| (site.span.src_id, site.span.start);
    let end = sorted.partition_point(|site| key(site) <= (file, offset));
    let Some(last) = end.checked_sub(1).map(|at| &sorted[at]) else {
        return &[];
    };
    if last.span.src_id != file {
        return &[];
    }
    let start = sorted[..end].partition_point(|site| key(site) < key(last));
    &sorted[start..end]
}

/// The sites of `file` of `design`, whose spans `origins` resolves.
fn sites(design: &Design, file: FileId, origins: &Origins) -> Sites {
    let hir = &design[file];
    let names = design.names(file);
    let mut sites = Sites::default();
    // A name is a site only where its span spells it: a declaration
    // lowering makes up, as an unnamed instance's, stands nowhere.
    let mut name = |span: Span, text: &str, kind: SiteKind| {
        let span = origins.spelled(span);
        if name_text(origins.slice(span)) == text {
            sites.names.push(Site { span, kind });
        }
    };

    for (id, expr) in hir.exprs() {
        let Some(found) = names.expr(id) else {
            continue;
        };
        let (span, text) = match &expr.kind {
            ExprKind::Name(text) | ExprKind::Loose(text) => (expr.span, &**text),
            ExprKind::Member { name, .. } | ExprKind::Scoped { name, .. } => {
                (name.span, &*name.text)
            }
            _ => continue,
        };
        name(span, text, SiteKind::Use(id, found));
    }
    for &(expr, scope) in names.members() {
        if let ExprKind::Member { name: member, .. } = &hir[expr].kind {
            name(member.span, &member.text, SiteKind::Member { expr, scope });
        }
    }

    let mut positional = Vec::new();
    for (id, symbol) in hir.symbols() {
        let at = SymbolRef { file, symbol: id };
        name(
            symbol.name.span,
            &symbol.name.text,
            SiteKind::Declaration(at),
        );
        let SymbolKind::Instance(instance) = &symbol.kind else {
            continue;
        };
        if instance.gate.is_some() {
            continue;
        }
        let definition = design.instantiated(at);
        let site = SiteKind::Definition {
            instance: id,
            definition,
        };
        name(instance.definition.span, &instance.definition.text, site);
        let formals = definition.and_then(|it| match &design.symbol(it).kind {
            SymbolKind::Definition {
                ports, overridable, ..
            } => Some((it, [overridable, ports])),
            _ => None,
        });
        for (side, args) in [&instance.parameters, &instance.connections]
            .into_iter()
            .enumerate()
        {
            let bound = match formals {
                Some((at, formals)) => bind::formals(&design[at.file], formals[side], args),
                None => vec![None; args.len()],
            };
            for (arg, formal) in args.iter().zip(bound) {
                let formal = formal.map(|symbol| SymbolRef {
                    file: formals.map_or(file, |(at, _)| at.file),
                    symbol,
                });
                match arg {
                    Arg::Positional(Some(expr)) => positional.push(Site {
                        span: origins.spelled(hir[*expr].span),
                        kind: SiteKind::Positional {
                            instance: id,
                            formal,
                        },
                    }),
                    Arg::Named { name: it, .. } | Arg::Implicit { name: it, .. } => {
                        let site = SiteKind::Formal {
                            instance: id,
                            formal,
                        };
                        name(it.span, &it.text, site);
                    }
                    Arg::Positional(None) | Arg::Wildcard(_) => {}
                }
            }
        }
    }

    for (_, scope) in hir.scopes() {
        for member in &scope.members {
            let Member::Import(import) = member else {
                continue;
            };
            let package = &import.package;
            if &*package.text != "*" {
                let found = design.package(&package.text);
                name(package.span, &package.text, SiteKind::Package(found));
            }
            if let Some(item) = &import.item {
                let found = design.member_of(&package.text, &item.text, 0);
                name(item.span, &item.text, SiteKind::Imported(found));
            }
        }
    }

    for (index, bind) in hir.binds.iter().enumerate() {
        let id = BindId(index as u32);
        let Some(target) = &bind.target else {
            continue;
        };
        let into = match design.target(file, id) {
            BindTarget::Definition(it)
            | BindTarget::Instances {
                definition: Some(it),
            } => Some(it),
            BindTarget::Instances { definition: None } | BindTarget::Undeclared => None,
        };
        name(target.span, &target.text, SiteKind::Target(id, into));
    }

    for ty in hir.types() {
        if let TypeKind::Struct { fields, .. } = &ty.kind {
            for field in fields {
                name(field.name.span, &field.name.text, SiteKind::Field);
            }
        }
    }

    let key = |site: &Site| (site.span.src_id, site.span.start, site.span.end);
    // A type is copied into each declarator it is written for, and its
    // members with it.
    sites.names.sort_by_key(key);
    sites.names.dedup();
    positional.sort_by_key(key);
    sites.arguments = positional;
    sites
}
