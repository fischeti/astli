//! What drives a signal: nothing, or more than one thing.
//!
//! Only in a module, interface or program, as for the unused rules, and not
//! of a declaration a macro wrote.

use astli_sema::{Driver, Hir, Member, ScopeId, SymbolKind, SymbolRef, TypeKind};
use astli_syntax::SyntaxKind::{
    self, ALWAYS_COMB_KW, ALWAYS_FF_KW, ALWAYS_LATCH_KW, OUTPUT_KW, SUPPLY0_KW, SUPPLY1_KW,
    TRI0_KW, TRI1_KW, VAR_KW,
};
use astli_text::{Label, Span};
use rustc_hash::{FxHashMap, FxHashSet};

use super::unused::declared;
use crate::rule::DesignCx;

/// A net or variable something reads and nothing writes, or an output
/// nothing drives.
///
/// A connection to a port without a direction, an argument of a subroutine
/// sema does not know and a name in a region it does not lower all count as
/// writes, as does a name any file reaches as a member, `top.u.sig`, which
/// may be a write from afar.
pub(crate) fn undriven_signal(cx: &mut DesignCx) {
    let (mut read, mut written) = (FxHashSet::default(), FxHashSet::default());
    for access in cx.accesses {
        if access.read {
            read.insert(access.symbol);
        }
        if access.write {
            written.insert(access.symbol);
        }
    }
    let hir = cx.hir();
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
        if written.contains(&symbol) || (!output && !read.contains(&symbol)) || excused(cx, symbol)
        {
            continue;
        }
        let message = match output {
            true => format!("output `{}` is never driven", name.text),
            false => format!("signal `{}` is read and never written", name.text),
        };
        cx.report(name.span, message);
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
/// Two writes collide only if one of them is to all of the variable: which
/// bits a select names is known only once it is evaluated. And only if one
/// driver exists whenever the other does, standing in the same generate
/// arms or in fewer of them: two arms of one `if` never both exist, and two
/// generate constructs may have conditions that exclude each other. A write
/// in a subroutine runs wherever it is called, and one in a region sema does
/// not lower is not placed, so neither counts.
pub(crate) fn multiple_drivers(cx: &mut DesignCx) {
    let hir = cx.hir();
    let mut places = FxHashMap::default();
    place(hir, hir.root(), &mut Vec::new(), &mut places);

    // Each variable's drivers, in the order written, with whether one of its
    // writes is to all of it and where it first writes.
    let mut drivers: FxHashMap<SymbolRef, Vec<(Driver, bool, Span)>> = FxHashMap::default();
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
        match list
            .iter_mut()
            .find(|(driver, ..)| *driver == access.driver)
        {
            Some((_, whole, _)) => *whole |= access.whole,
            None => list.push((access.driver, access.whole, access.at)),
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
                (first.1 || second.1)
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
