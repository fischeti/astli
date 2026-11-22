//! `textDocument/inlayHint`: what an instantiation leaves unsaid, drawn
//! into the text of a range of a file.
//!
//! Each kind of hint reads what hover reads: the formal a positional
//! argument lands on, the ports a `.*` connects, the value an override or
//! a parameter evaluates to, a named connection's port. A hint is placed
//! only at text the file writes and uses where it is written, never in
//! what a macro wrote, whose one place stands for every expansion. A
//! value depends on the body it is evaluated in, so it is hinted only
//! where every body agrees on it ([`across`]).
//!
//! An editor switches hints on and off as a whole, and the server has no
//! settings, so [`KINDS`] says which kinds it answers.

use astli_sema::{
    Arg, BodyId, Bound, ExprId, ExprKind, FileId, Hir, SiteKind, SymbolId, SymbolKind, SymbolRef,
    Ty, Value,
};
use astli_text::{SourceId, Span};
use lsp_server::{RequestId, ResponseError};
use lsp_types::{InlayHint, InlayHintKind, InlayHintParams, Position};
use serde::Deserialize;
use serde_json::Value as Json;

use crate::cursor::Cursor;
use crate::hover::{around, value};
use crate::server::Server;
use crate::snapshot::{Analysed, Unit};
use crate::tasks::Cancel;

/// What a hint says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The formal a positional connection or override lands on, before it.
    Formal,
    /// The ports a `.*` connects, after it.
    Wildcard,
    /// What an override, or a parameter's declared value, evaluates to,
    /// after it, where it is not a literal.
    Value,
    /// The direction and type of the port a named connection names, before
    /// it, as a declaration of the port would write them.
    Port,
}

/// Each kind, and whether the server answers it. A port's type is left
/// out: real code connects by name, so it would stand at nearly every
/// connection, six to eleven hints in a hundred lines, where the others
/// are a few in a thousand.
const KINDS: [(Kind, bool); 4] = [
    (Kind::Formal, true),
    (Kind::Wildcard, true),
    (Kind::Value, true),
    (Kind::Port, false),
];

/// The kinds the server answers.
pub fn answered() -> Vec<Kind> {
    (KINDS.iter())
        .filter(|(_, on)| *on)
        .map(|(kind, _)| *kind)
        .collect()
}

/// Every kind, answered or not.
pub fn every() -> Vec<Kind> {
    KINDS.iter().map(|(kind, _)| *kind).collect()
}

/// At most this many characters of a hint's label; the rest is cut to
/// `…`, and the whole is its tooltip.
const LENGTH: usize = 32;

impl Server {
    /// Hands the request to the pool, with a cursor at the start of its
    /// range as things are now.
    pub fn inlay_hints(&mut self, id: RequestId, params: Json) {
        let asked = InlayHintParams::deserialize(&params).ok();
        let cursor = asked.and_then(|it| self.cursor_at(&it.text_document.uri, it.range.start));
        let kinds = self.hints.clone();
        let method = "textDocument/inlayHint";
        self.spawn(id, method, params, move |asked: InlayHintParams, cancel| {
            let Some(cursor) = cursor else {
                return Ok(None);
            };
            hints(&cursor, asked.range.end, &kinds, cancel).map(Some)
        });
    }
}

/// A hint, at a byte of the file as its unit holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Hint {
    offset: u32,
    kind: Kind,
    label: String,
    tooltip: Option<String>,
}

/// The hints of `kinds` from the cursor to `end`, from each unit that
/// holds the file; where two units hint otherwise at one place, as a
/// header's instance may mean something else in each, neither.
fn hints(
    cursor: &Cursor,
    end: Position,
    kinds: &[Kind],
    cancel: &Cancel,
) -> Result<Vec<InlayHint>, ResponseError> {
    let mut found: Vec<Hint> = Vec::new();
    let mut place = None;
    for here in cursor.lookup(cancel)? {
        cancel.check()?;
        let origins = &here.unit.origins;
        let (text, lines) = (origins.text(here.source), origins.lines(here.source));
        let end = cursor.encoding.offset(text, lines, end);
        let mut file = File {
            analysed: here.design,
            unit: here.unit,
            source: here.source,
            range: (here.offset, end),
            kinds,
            out: Vec::new(),
        };
        file.instances(cancel)?;
        file.parameters();
        found.extend(file.out);
        place.get_or_insert((here.unit, here.source));
    }
    let Some((unit, source)) = place else {
        return Ok(Vec::new());
    };
    found.sort_by_key(|it| it.offset);
    let mut kept: Vec<Hint> = Vec::new();
    for hint in found {
        let same = |it: &Hint| it.offset == hint.offset && it.kind == hint.kind;
        match kept.iter().position(same) {
            Some(at) if kept[at] != hint => kept[at].label.clear(),
            Some(_) => {}
            None => kept.push(hint),
        }
    }
    let origins = &unit.origins;
    let (text, lines) = (origins.text(source), origins.lines(source));
    let hints = (kept.into_iter().filter(|it| !it.label.is_empty())).map(|hint| {
        // A hint before the text it is about is padded after, one after it
        // before.
        let before = matches!(hint.kind, Kind::Formal | Kind::Port);
        InlayHint {
            position: cursor.encoding.position(text, lines, hint.offset),
            label: hint.label.into(),
            kind: match hint.kind {
                Kind::Formal | Kind::Wildcard => Some(InlayHintKind::Parameter),
                Kind::Port => Some(InlayHintKind::Type),
                Kind::Value => None,
            },
            text_edits: None,
            tooltip: hint.tooltip.map(Into::into),
            padding_left: (!before).then_some(true),
            padding_right: before.then_some(true),
            data: None,
        }
    });
    Ok(hints.collect())
}

/// What one unit hints in one file it holds.
struct File<'a> {
    analysed: &'a Analysed,
    unit: &'a Unit,
    /// The file, as the unit's origins hold it as written.
    source: SourceId,
    /// The bytes asked about, both ends included.
    range: (u32, u32),
    kinds: &'a [Kind],
    out: Vec<Hint>,
}

impl File<'_> {
    /// Whether `span` is written in the file and used where it is written,
    /// not placed by a macro, and so may be hinted at.
    fn written(&self, span: Span) -> bool {
        span.src_id == self.source
    }

    /// Adds a hint of `kind` at `offset`, if it is in the range and of a
    /// kind asked for; `label` is asked only then.
    fn hint(&mut self, kind: Kind, offset: u32, label: impl FnOnce() -> Option<(String, String)>) {
        let (start, end) = self.range;
        if !(start..=end).contains(&offset) || !self.kinds.contains(&kind) {
            return;
        }
        if let Some((label, tooltip)) = label() {
            let tooltip = (label != tooltip).then_some(tooltip);
            self.out.push(Hint {
                offset,
                kind,
                label,
                tooltip,
            });
        }
    }

    /// The hints of each instance the file declares of a definition some
    /// file declares.
    fn instances(&mut self, cancel: &Cancel) -> Result<(), ResponseError> {
        let design = self.analysed.elaboration.design();
        let file = self.unit.file;
        for site in self.unit.sites(design).iter() {
            let SiteKind::Definition {
                instance,
                definition: Some(definition),
            } = site.kind
            else {
                continue;
            };
            cancel.check()?;
            let at = SymbolRef {
                file,
                symbol: instance,
            };
            let SymbolKind::Instance(it) = &design.symbol(at).kind else {
                continue;
            };
            let overrides = self.analysed.elaboration.overrides(at);
            for arg in &it.parameters {
                if let Arg::Positional(Some(expr)) = arg
                    && let Some(&(formal, _)) = overrides.iter().find(|(_, it)| it == expr)
                {
                    self.formal(definition.file, formal, *expr);
                }
            }
            for &(parameter, expr) in &overrides {
                self.overridden(at, definition.file, parameter, expr);
            }
            let connections = self.analysed.elaboration.connections(at);
            for arg in &it.connections {
                match arg {
                    Arg::Positional(Some(expr)) => {
                        let port = connections.iter().find_map(|(port, bound)| match bound {
                            Bound::Expr(it) if it == expr => Some(*port),
                            _ => None,
                        });
                        if let Some(port) = port {
                            self.formal(definition.file, port, *expr);
                        }
                    }
                    Arg::Named { name, .. } | Arg::Implicit { name, .. } => {
                        self.port(at, definition, &name.text, name.span);
                    }
                    Arg::Wildcard(span) => self.wildcard(*span),
                    Arg::Positional(None) => {}
                }
            }
        }
        Ok(())
    }

    /// The name of `formal`, of the definition `file` declares, before the
    /// positional argument `expr`, unless the argument is that name.
    fn formal(&mut self, file: FileId, formal: SymbolId, expr: ExprId) {
        let design = self.analysed.elaboration.design();
        let span = design[self.unit.file][expr].span;
        if !self.written(span) {
            return;
        }
        let name = &*design[file][formal].name.text;
        if self.unit.origins.slice(span).trim() == name {
            return;
        }
        self.hint(Kind::Formal, span.start, || Some(cut(&format!("{name}:"))));
    }

    /// The ports the `.*` at `span` connects, after it.
    fn wildcard(&mut self, span: Span) {
        if !self.written(span) {
            return;
        }
        let design = self.analysed.elaboration.design();
        let connected = design.names(self.unit.file).wildcard(span);
        self.hint(Kind::Wildcard, span.end, || {
            let names: Vec<&str> = (connected.iter())
                .map(|(port, _)| &*design.symbol(*port).name.text)
                .collect();
            (!names.is_empty()).then(|| listed(&names))
        });
    }

    /// The direction and type of the port named `name` at `span` of the
    /// definition the instance `at` is of, before the `.` that names it.
    fn port(&mut self, at: SymbolRef, definition: SymbolRef, name: &str, span: Span) {
        if !self.written(span) {
            return;
        }
        let elaboration = &self.analysed.elaboration;
        let design = elaboration.design();
        let SymbolKind::Definition { ports, .. } = &design.symbol(definition).kind else {
            return;
        };
        let hir = &design[definition.file];
        let Some(&port) = ports.iter().find(|&&it| &*hir[it].name.text == name) else {
            return;
        };
        let SymbolKind::Port(declared) = &hir[port].kind else {
            return;
        };
        let port = SymbolRef {
            file: definition.file,
            symbol: port,
        };
        let direction = declared.direction.and_then(|it| it.keyword_text());
        let analysed = self.analysed;
        let before = &self.unit.origins.text(self.source)[..span.start as usize];
        let dot = before
            .trim_end()
            .strip_suffix('.')
            .map_or(span.start, |it| it.len() as u32);
        self.hint(Kind::Port, dot, || {
            let ty = agreed(analysed, at, |built| {
                match elaboration.symbol_type(built, port) {
                    Ty::Unknown => None,
                    ty => Some(value::shown(&ty)),
                }
            });
            let said = match (direction, ty) {
                (Some(direction), Some(ty)) => format!("{direction} {ty}"),
                (Some(direction), None) => direction.to_string(),
                (None, Some(ty)) => ty,
                (None, None) => return None,
            };
            Some(cut(&said))
        });
    }

    /// What the override `expr` of the instance `at` gives `parameter`, of
    /// the definition `file` declares, after it: in the body the instance
    /// builds, from each body of the definition around it.
    fn overridden(&mut self, at: SymbolRef, file: FileId, parameter: SymbolId, expr: ExprId) {
        let elaboration = &self.analysed.elaboration;
        let design = elaboration.design();
        let span = design[self.unit.file][expr].span;
        if !self.written(span) || literal(&design[self.unit.file], expr) {
            return;
        }
        if let SymbolKind::Parameter(it) = &design[file][parameter].kind
            && it.is_type
        {
            return;
        }
        let written = self.unit.origins.slice(span);
        let analysed = self.analysed;
        self.hint(Kind::Value, span.end, || {
            let said = agreed(analysed, at, |built| said(analysed, built, parameter))?;
            valued(written, &said)
        });
    }

    /// The value of each parameter the file declares with a value that is
    /// not a literal, after it, where every body of the definition around
    /// it agrees on it.
    fn parameters(&mut self) {
        let elaboration = &self.analysed.elaboration;
        let design = elaboration.design();
        let file = self.unit.file;
        let hir = &design[file];
        for (id, symbol) in hir.symbols() {
            let SymbolKind::Parameter(parameter) = &symbol.kind else {
                continue;
            };
            let Some(expr) = parameter.value.filter(|_| !parameter.is_type) else {
                continue;
            };
            let span = hir[expr].span;
            if !self.written(span) || literal(hir, expr) {
                continue;
            }
            let written = self.unit.origins.slice(span);
            let analysed = self.analysed;
            let defaults = elaboration.body_at(file, symbol.scope);
            let around = around(design, file, symbol.scope);
            self.hint(Kind::Value, span.end, || {
                let said = across(analysed, around, defaults, |body| said(analysed, body, id))?;
                valued(written, &said)
            });
        }
    }
}

/// What `of` says of the body the instance `at` builds, in each body of
/// the definition around it: one thing, or `None` where they differ.
fn agreed(
    analysed: &Analysed,
    at: SymbolRef,
    of: impl Fn(BodyId) -> Option<String>,
) -> Option<String> {
    let elaboration = &analysed.elaboration;
    let design = elaboration.design();
    let scope = design.symbol(at).scope;
    let holder = elaboration.body_at(at.file, scope);
    let around = around(design, at.file, scope);
    across(analysed, around, holder, |body| {
        let (_, built) = elaboration.child(body, at)?;
        of(built)
    })
}

/// What `of` says at `defaults`, the body of `around` at its defaults, if
/// the tops build `around` and every body they build of it says the same;
/// `None` where one says otherwise or nothing, since which a reader means
/// is not known. Without an `around`, where there is one body, what it
/// says. A body at its defaults alone says nothing for sure: a hint cannot
/// say that it is the defaults', as hover does, and a definition's
/// instances seldom all keep them.
fn across(
    analysed: &Analysed,
    around: Option<SymbolRef>,
    defaults: BodyId,
    of: impl Fn(BodyId) -> Option<String>,
) -> Option<String> {
    let said = of(defaults)?;
    let Some(around) = around else {
        return Some(said);
    };
    let design = analysed.elaboration.design();
    let mut bodies = analysed.built()?.bodies(design, around).peekable();
    bodies.peek()?;
    for frame in bodies {
        if of(frame.body)? != said {
            return None;
        }
    }
    Some(said)
}

/// The value of `parameter` in `body`, as hover writes it; `None` where it
/// is not known.
fn said(analysed: &Analysed, body: BodyId, parameter: SymbolId) -> Option<String> {
    let elaboration = &analysed.elaboration;
    match elaboration.value(body, parameter) {
        Value::Unknown => None,
        it => {
            let ty = elaboration.type_of(body, parameter);
            Some(value::said(elaboration, body, &ty, &it))
        }
    }
}

/// The label and tooltip of a value `said` of an expression `written`:
/// none where the text says it already.
fn valued(written: &str, said: &str) -> Option<(String, String)> {
    let spaced = |it: &str| it.split_whitespace().collect::<String>();
    (spaced(written) != spaced(said)).then(|| cut(&format!("= {said}")))
}

/// Whether `expr` of `hir` says its value as written: a literal, or a
/// concatenation or an untyped pattern of literals, as a table of widths
/// is written.
fn literal(hir: &Hir, expr: ExprId) -> bool {
    match &hir[expr].kind {
        ExprKind::Literal { .. } | ExprKind::Time { .. } => true,
        ExprKind::Concat(items) => items.iter().all(|&it| literal(hir, it)),
        ExprKind::Pattern { ty: None, items } => items.iter().all(|it| literal(hir, it.value)),
        _ => false,
    }
}

/// `text` as a label of [`LENGTH`] characters at most, and its tooltip,
/// the whole of it.
fn cut(text: &str) -> (String, String) {
    let label = match text.chars().count() > LENGTH {
        true => text.chars().take(LENGTH - 1).chain(['…']).collect(),
        false => text.to_string(),
    };
    (label, text.to_string())
}

/// `names` as a label, as many as fit in [`LENGTH`] characters and `…`
/// for the rest, and its tooltip, all of them.
fn listed(names: &[&str]) -> (String, String) {
    let all = names.join(", ");
    let mut label = String::new();
    for name in names {
        let next = match label.is_empty() {
            true => name.to_string(),
            false => format!("{label}, {name}"),
        };
        if next.chars().count() > LENGTH {
            label = match label.is_empty() {
                true => "…".to_string(),
                false => format!("{label}, …"),
            };
            return (label, all);
        }
        label = next;
    }
    (label, all)
}
