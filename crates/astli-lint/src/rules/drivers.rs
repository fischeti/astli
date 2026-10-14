//! What drives a signal: nothing, or more than one thing.
//!
//! Only in a module, interface or program, as for the unused rules, and not
//! of a declaration a macro wrote.

use std::ops::Range;

use astli_sema::{
    Access, Driver, Hir, Member, ScopeId, Shape, SymbolId, SymbolKind, SymbolRef, Ty, TypeKind,
};
use astli_syntax::SyntaxKind::{
    self, ALWAYS_COMB_KW, ALWAYS_FF_KW, ALWAYS_LATCH_KW, OUTPUT_KW, SUPPLY0_KW, SUPPLY1_KW,
    TRI0_KW, TRI1_KW, VAR_KW,
};
use astli_text::{Label, Span};
use rustc_hash::{FxHashMap, FxHashSet};

use super::unused::declared;
use crate::rule::DesignCx;

/// A net or variable something reads and nothing writes, or an output
/// nothing drives; or the bits of one that its writes miss, as its
/// definition's defaults evaluate their selects.
///
/// A connection to a port without a direction, an argument of a subroutine
/// sema does not know and a name in a region it does not lower all count as
/// writes, as does a hierarchical name that does not resolve, `up.u.sig`,
/// which may be a write from afar; one that does, from another file, writes
/// the signal it resolves to. So does a write to a part sema cannot
/// evaluate; a read of one may read any bit, so its signal's bits are not
/// compared.
pub(crate) fn undriven_signal(cx: &mut DesignCx) {
    let mut reads: FxHashMap<SymbolRef, Reach> = FxHashMap::default();
    let mut writes: FxHashMap<SymbolRef, Reach> = FxHashMap::default();
    // What reads a part sema cannot place, which may be any.
    let mut unplaced = FxHashSet::default();
    for access in cx.accesses {
        let part = cx.reach(access).filter(|_| access.certain && !access.whole);
        let placed = access.whole || part.is_some();
        // A write sema cannot place may reach all of it.
        let reach = Reach {
            whole: part.is_none(),
            parts: part.into_iter().collect(),
        };
        if access.read {
            if !placed || !access.certain {
                unplaced.insert(access.symbol);
            }
            let entry: &mut Reach = reads.entry(access.symbol).or_default();
            entry.add(Reach {
                whole: reach.whole,
                parts: reach.parts.clone(),
            });
        }
        if access.write {
            writes.entry(access.symbol).or_default().add(reach);
        }
    }
    let hir = cx.hir();
    let homes: FxHashMap<SymbolId, ScopeId> = (hir.scopes())
        .flat_map(|(id, scope)| {
            scope.members.iter().filter_map(move |member| match member {
                Member::Declare(symbol) => Some((*symbol, id)),
                _ => None,
            })
        })
        .collect();
    for symbol in declared(cx, |kind| match kind {
        // These drive themselves.
        SymbolKind::Net(data) => !matches!(
            data.keyword,
            Some(SUPPLY0_KW | SUPPLY1_KW | TRI0_KW | TRI1_KW)
        ),
        SymbolKind::Variable(_) => true,
        SymbolKind::Port(port) => port.direction == Some(OUTPUT_KW),
        _ => false,
    }) {
        let name = &hir[symbol.symbol].name;
        let output = matches!(hir[symbol.symbol].kind, SymbolKind::Port(_));
        let (read, written) = (reads.get(&symbol), writes.get(&symbol));
        // Another file's hierarchical name reaches all of it.
        let (read_afar, written_afar) = cx.linter.remote(symbol);
        if (!output && read.is_none() && !read_afar)
            || written.is_some_and(|it| it.whole)
            || written_afar
            || excused(cx, symbol)
        {
            continue;
        }
        let Some(written) = written else {
            let message = match output {
                true => format!("output `{}` is never driven", name.text),
                false => format!("signal `{}` is read and never written", name.text),
            };
            cx.report(name.span, message);
            continue;
        };
        // Written in part: what is read, all of an output, that no write
        // reaches.
        if unplaced.contains(&symbol) {
            continue;
        }
        let Some(ty) = (homes.get(&symbol.symbol))
            .and_then(|&scope| definition_body(cx, scope))
            .map(|body| cx.elaboration.type_of(body, symbol.symbol))
        else {
            continue;
        };
        let Some(bits) = ty.bits() else {
            continue;
        };
        let needed = match read {
            Some(read) if !read.whole && !output && !read_afar => read.parts.clone(),
            _ => std::iter::once(0..bits).collect(),
        };
        let missed = missing(needed, &written.parts);
        if missed.is_empty() {
            continue;
        }
        let mut parts = Vec::new();
        for part in &missed {
            named(name.text.to_string(), &ty, part.clone(), &mut parts);
        }
        let what = match parts.split_last() {
            Some((last, [])) => format!("{last} is"),
            Some((last, rest)) if rest.len() < 4 => {
                format!("{} and {last} are", rest.join(", "))
            }
            Some((_, rest)) => format!(
                "{} and {} more parts of it are",
                rest[..3].join(", "),
                parts.len() - 3
            ),
            None => continue,
        };
        let message = match output {
            true => format!("{what} never driven"),
            false => format!("{what} read and never written"),
        };
        cx.report(name.span, message);
    }
}

/// The body of the definition around `scope`, at its defaults.
fn definition_body(cx: &DesignCx, scope: ScopeId) -> Option<astli_sema::BodyId> {
    let hir = cx.hir();
    let mut at = Some(scope);
    while let Some(scope) = at {
        if let Some(owner) = hir[scope].owner
            && let SymbolKind::Definition { .. } = hir[owner].kind
        {
            return cx.elaboration.body(SymbolRef {
                file: cx.file,
                symbol: owner,
            });
        }
        at = hir[scope].parent;
    }
    None
}

/// The bits of `needed` none of `have` covers, lowest first.
fn missing(mut needed: Vec<Range<u64>>, have: &[Range<u64>]) -> Vec<Range<u64>> {
    let mut have = have.to_vec();
    have.sort_by_key(|it| it.start);
    needed.sort_by_key(|it| it.start);
    let mut missed: Vec<Range<u64>> = Vec::new();
    for part in needed {
        let mut from = part.start;
        for it in &have {
            if it.end <= from || it.start >= part.end {
                continue;
            }
            if it.start > from {
                missed.push(from..it.start);
            }
            from = from.max(it.end);
        }
        if from < part.end {
            missed.push(from..part.end);
        }
    }
    // Merge what touches, as two reads of one bit miss it once.
    missed.sort_by_key(|it| it.start);
    let mut merged: Vec<Range<u64>> = Vec::new();
    for part in missed {
        match merged.last_mut() {
            Some(last) if part.start <= last.end => last.end = last.end.max(part.end),
            _ => merged.push(part),
        }
    }
    merged
}

/// The bits `part` of `name`, of type `ty`, as the selects and members
/// that name them, into `out`: `name[7:4]`, `name.member`, `name[2]`.
fn named(name: String, ty: &Ty, part: Range<u64>, out: &mut Vec<String>) {
    // Each element or member, by its name, its type, and its bits.
    let mut within: Vec<(String, Ty, Range<u64>)> = Vec::new();
    match ty {
        Ty::Packed(packed) => match &packed.shape {
            Shape::Vector { left, right } => {
                out.push(format!("`{name}{}`", indices(*left, *right, &part)));
                return;
            }
            Shape::Array {
                left,
                right,
                element,
            } => {
                let width = u64::from(element.width);
                if part.start.is_multiple_of(width) && part.end.is_multiple_of(width) {
                    let elements = part.start / width..part.end / width;
                    out.push(format!("`{name}{}`", indices(*left, *right, &elements)));
                    return;
                }
                for at in part.start / width..part.end.div_ceil(width) {
                    let index = indices(*left, *right, &(at..at + 1));
                    let bits = at * width..(at + 1) * width;
                    within.push((
                        format!("{name}{index}"),
                        Ty::Packed((**element).clone()),
                        bits,
                    ));
                }
            }
            Shape::Struct(st) => {
                // The first member is the most significant.
                let mut end = u64::from(packed.width);
                for member in &st.members {
                    let width = u64::from(member.ty.width().unwrap_or(0));
                    let start = if st.union { 0 } else { end - width };
                    within.push((
                        format!("{name}.{}", member.name),
                        member.ty.clone(),
                        start..start + width,
                    ));
                    if !st.union {
                        end = start;
                    }
                }
            }
            _ => {}
        },
        Ty::Array(array) => {
            if let Some(width) = array.element.bits().filter(|&it| it > 0) {
                for at in part.start / width..part.end.div_ceil(width) {
                    let index = match array.left <= array.right {
                        true => array.left + at as i64,
                        false => array.left - at as i64,
                    };
                    within.push((
                        format!("{name}[{index}]"),
                        array.element.clone(),
                        at * width..(at + 1) * width,
                    ));
                }
            }
        }
        Ty::Struct(st) if !st.union => {
            let mut start = 0;
            for member in &st.members {
                let Some(width) = member.ty.bits() else {
                    break;
                };
                within.push((
                    format!("{name}.{}", member.name),
                    member.ty.clone(),
                    start..start + width,
                ));
                start += width;
            }
        }
        _ => {}
    }
    let overlapping: Vec<_> = (within.into_iter())
        .filter(|(_, _, bits)| bits.start < part.end && part.start < bits.end)
        .collect();
    if overlapping.is_empty() {
        out.push(format!("part of `{name}`"));
    }
    for (name, ty, bits) in overlapping {
        if part.start <= bits.start && bits.end <= part.end {
            out.push(format!("`{name}`"));
        } else {
            let inner =
                part.start.max(bits.start) - bits.start..part.end.min(bits.end) - bits.start;
            named(name, &ty, inner, out);
        }
    }
}

/// The select of a dimension indexed `[left:right]` that its entries
/// `part`, counted from the one at `right`, are.
fn indices(left: i64, right: i64, part: &Range<u64>) -> String {
    let (low, high) = (part.start as i64, part.end as i64 - 1);
    let (a, b) = match left >= right {
        true => (right + high, right + low),
        false => (right - high, right - low),
    };
    match a == b {
        true => format!("[{a}]"),
        false => format!("[{a}:{b}]"),
    }
}

/// Whether `symbol` may be written where resolution does not reach, or is
/// written by a macro.
fn excused(cx: &DesignCx, symbol: SymbolRef) -> bool {
    let name = &cx.hir()[symbol.symbol].name;
    cx.written_by_macro(name.span) || cx.linter.maybe_used(cx.file, &name.text)
}

/// A variable two things drive where the standard allows only one: an
/// `always_comb`, `always_ff` or `always_latch` block, a continuous
/// assignment or an instance's output, and anything else.
///
/// Two writes collide only if one of them is to all of the variable, or the
/// bits their selects reach at the definition's defaults overlap; a select
/// sema cannot evaluate, as one by a generate loop's genvar, collides only
/// with a write to all of it. And only if one driver exists whenever the
/// other does, standing in the same generate
/// arms or in fewer of them: two arms of one `if` never both exist, and two
/// generate constructs may have conditions that exclude each other. A write
/// in a subroutine runs wherever it is called, and one in a region sema does
/// not lower is not placed, so neither counts.
pub(crate) fn multiple_drivers(cx: &mut DesignCx) {
    let hir = cx.hir();
    let mut places = FxHashMap::default();
    place(hir, hir.root(), &mut Vec::new(), &mut places);

    // Each variable's drivers, in the order written, with the bits its
    // writes reach and where it first writes.
    let mut drivers: FxHashMap<SymbolRef, Vec<(Driver, Reach, Span)>> = FxHashMap::default();
    let mut order = Vec::new();
    for access in cx.accesses {
        let placed = matches!(
            access.driver,
            Driver::Process(_) | Driver::Assign(_) | Driver::Instance(_)
        );
        if !access.write
            || !access.certain
            || !placed
            || access.symbol.file != cx.file
            || !variable(hir, access.symbol)
        {
            continue;
        }
        let list = drivers.entry(access.symbol).or_insert_with(|| {
            order.push(access.symbol);
            Vec::new()
        });
        let reach = Reach::of(cx, access);
        match list
            .iter_mut()
            .find(|(driver, ..)| *driver == access.driver)
        {
            Some((_, it, _)) => it.add(reach),
            None => list.push((access.driver, reach, access.at)),
        }
    }

    for symbol in order {
        let list = &drivers[&symbol];
        let name = &hir[symbol.symbol].name;
        if cx.written_by_macro(name.span) {
            continue;
        }
        let clash = list.iter().enumerate().find_map(|(at, second)| {
            let first = list[..at].iter().find(|first| {
                first.1.overlaps(&second.1)
                    && (sole(&places, first.0) || sole(&places, second.0))
                    && together(&places, first.0, second.0)
            })?;
            Some((first, second))
        });
        let Some((first, second)) = clash else {
            continue;
        };
        let message = format!(
            "`{}` is driven by {} and by {}",
            name.text,
            what(hir, &places, first.0),
            what(hir, &places, second.0)
        );
        cx.report(second.2, message).labels.push(Label {
            at: first.2,
            message: "driven here first".to_string(),
        });
    }
}

/// The bits of a variable a driver's writes reach.
#[derive(Default)]
pub(super) struct Reach {
    /// Whether one is to all of it.
    pub whole: bool,
    /// The bits its writes to parts reach, where sema can evaluate them.
    pub parts: Vec<Range<u64>>,
}

impl Reach {
    pub fn of(cx: &DesignCx, access: &Access) -> Reach {
        Reach {
            whole: access.whole,
            parts: cx.reach(access).into_iter().collect(),
        }
    }

    pub fn add(&mut self, other: Reach) {
        self.whole |= other.whole;
        self.parts.extend(other.parts);
    }

    /// Whether some bit is in both.
    pub fn overlaps(&self, other: &Reach) -> bool {
        self.whole
            || other.whole
            || (self.parts.iter())
                .any(|a| (other.parts.iter()).any(|b| a.start < b.end && b.start < a.end))
    }
}

/// Where a driver stands: the generate arms around it, each as the scope
/// and member of its `if` or `case` and the arm, and for a process its
/// keyword.
type Places = FxHashMap<Driver, (Vec<(ScopeId, usize, usize)>, Option<SyntaxKind>)>;

/// Records where each driver in `scope` stands, inside `arms`.
fn place(hir: &Hir, scope: ScopeId, arms: &mut Vec<(ScopeId, usize, usize)>, places: &mut Places) {
    for (index, member) in hir[scope].members.iter().enumerate() {
        match member {
            Member::Process(process) => {
                places.insert(
                    Driver::Process(process.body),
                    (arms.clone(), Some(process.kind)),
                );
            }
            Member::Assign(assignments) => {
                for &assignment in assignments {
                    places.insert(Driver::Assign(assignment), (arms.clone(), None));
                }
            }
            Member::Declare(symbol) => match &hir[*symbol].kind {
                SymbolKind::Instance(_) => {
                    places.insert(Driver::Instance(*symbol), (arms.clone(), None));
                }
                SymbolKind::Definition { scope, .. } => place(hir, *scope, arms, places),
                _ => {}
            },
            Member::GenerateIf(choices) | Member::GenerateCase { arms: choices, .. } => {
                for (arm, choice) in choices.iter().enumerate() {
                    arms.push((scope, index, arm));
                    place(hir, choice.body, arms, places);
                    arms.pop();
                }
            }
            Member::GenerateFor { body, .. } | Member::Generate(body) => {
                place(hir, *body, arms, places);
            }
            Member::Import(_) | Member::Opaque(_) => {}
        }
    }
}

/// Whether `driver` must be a variable's only one: a continuous assignment,
/// an instance's output, or an `always_comb`, `always_ff` or `always_latch`.
fn sole(places: &Places, driver: Driver) -> bool {
    match driver {
        Driver::Process(_) => matches!(
            places.get(&driver).and_then(|(_, kind)| *kind),
            Some(ALWAYS_COMB_KW | ALWAYS_FF_KW | ALWAYS_LATCH_KW)
        ),
        _ => true,
    }
}

/// Whether one of `a` and `b` exists whenever the other does: the generate
/// arms one stands in are the first of those the other stands in.
fn together(places: &Places, a: Driver, b: Driver) -> bool {
    let (Some((a, _)), Some((b, _))) = (places.get(&a), places.get(&b)) else {
        return false;
    };
    a.starts_with(b) || b.starts_with(a)
}

/// A driver, as a message names it.
fn what(hir: &Hir, places: &Places, driver: Driver) -> String {
    match driver {
        Driver::Process(_) => match places.get(&driver).and_then(|(_, kind)| *kind) {
            Some(kind) => format!("an `{}`", kind.keyword_text().unwrap_or("always")),
            None => "a process".to_string(),
        },
        Driver::Assign(_) => "a continuous assignment".to_string(),
        Driver::Instance(symbol) => format!("instance `{}`", hir[symbol].name.text),
        _ => "something else".to_string(),
    }
}

/// Whether `symbol` is a variable, of which the standard allows one driver
/// of these kinds; a net may have many.
fn variable(hir: &Hir, symbol: SymbolRef) -> bool {
    match &hir[symbol.symbol].kind {
        SymbolKind::Variable(_) => true,
        // An output with a data type and no net type is a variable.
        SymbolKind::Port(port) => {
            port.direction == Some(OUTPUT_KW)
                && matches!(port.keyword, None | Some(VAR_KW))
                && !matches!(port.ty.kind, TypeKind::Implicit { .. })
        }
        _ => false,
    }
}
