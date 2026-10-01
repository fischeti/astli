//! What drives a signal: nothing, or more than one thing.
//!
//! Only in a module, interface or program, as for the unused rules, and not
//! of a declaration a macro wrote.

use astli_sema::{SymbolKind, SymbolRef};
use astli_syntax::SyntaxKind::{OUTPUT_KW, SUPPLY0_KW, SUPPLY1_KW, TRI0_KW, TRI1_KW};
use rustc_hash::FxHashSet;

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
