//! What drives a signal: nothing. One driven twice is an error `check`
//! reports.
//!
//! Only in a module, interface or program, as for the unused rules, and not
//! of a declaration a macro wrote.

use std::ops::Range;

use astli_sema::{Member, ScopeId, Shape, SymbolId, SymbolKind, SymbolRef, Ty};
use astli_syntax::SyntaxKind::{OUTPUT_KW, SUPPLY0_KW, SUPPLY1_KW, TRI0_KW, TRI1_KW};
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

/// The bits of a variable a driver's writes reach.
#[derive(Default)]
struct Reach {
    /// Whether one is to all of it.
    whole: bool,
    /// The bits its writes to parts reach, where sema can evaluate them.
    parts: Vec<Range<u64>>,
}

impl Reach {
    fn add(&mut self, other: Reach) {
        self.whole |= other.whole;
        self.parts.extend(other.parts);
    }
}
