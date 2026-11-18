//! Which formal each argument is given to: a call's arguments to its
//! subroutine's formals, an instance's overrides to its definition's
//! parameters, its connections to its ports.
//!
//! The `n`th positional argument is given to the `n`th formal, counted among
//! the positional arguments alone. A call may give positional arguments
//! before named ones, and an instance gives one kind or the other, so where
//! the standard allows a mix the count is also the argument's index; where a
//! named argument comes first, as nothing allows, the positional ones still
//! start at the first formal.

use astli_text::Span;

use crate::design::{Design, SymbolRef};
use crate::hir::*;
use crate::resolve::{Names, Resolution};

/// What an argument gives the formal it is bound to.
#[derive(Debug, Clone, Copy)]
pub enum Bound {
    /// An expression, by position or by name.
    Expr(ExprId),
    /// What the name of a `.name` or a `.*` refers to where the argument is
    /// written, and where it is written: the name, or the `.*`.
    Name(Resolution, Span),
}

/// The formal of `formals`, declared in `hir`, that each of `args` is bound
/// to, by position or by name: `None` for a `.*`, a positional argument past
/// the last formal and a name no formal has. One that gives nothing,
/// `.name()` or a position left empty, is bound all the same.
pub(crate) fn formals(hir: &Hir, formals: &[SymbolId], args: &[Arg]) -> Vec<Option<SymbolId>> {
    let named = |name: &Name| (formals.iter().copied()).find(|&it| hir[it].name.text == name.text);
    let mut position = 0;
    (args.iter())
        .map(|arg| match arg {
            Arg::Positional(_) => {
                position += 1;
                formals.get(position - 1).copied()
            }
            Arg::Named { name, .. } | Arg::Implicit(name) => named(name),
            Arg::Wildcard(_) => None,
        })
        .collect()
}

/// Each formal of `formals`, declared in `hir`, that `args` give something,
/// and what: in the order written, then what each `.*` connects. `names`
/// resolves the file `args` are written in. A formal given twice is listed
/// twice; one left without a value, by `.name()`, an empty position or no
/// argument at all, is not listed.
pub fn bind(
    hir: &Hir,
    formals: &[SymbolId],
    args: &[Arg],
    names: &Names,
) -> Vec<(SymbolId, Bound)> {
    let mut bound = Vec::new();
    for (arg, formal) in args.iter().zip(self::formals(hir, formals, args)) {
        let given = match arg {
            Arg::Positional(value) | Arg::Named { value, .. } => value.map(Bound::Expr),
            Arg::Implicit(name) => (names.loose_at(name.span)).map(|it| Bound::Name(it, name.span)),
            Arg::Wildcard(_) => None,
        };
        if let (Some(formal), Some(given)) = (formal, given) {
            bound.push((formal, given));
        }
    }
    for arg in args {
        if let Arg::Wildcard(at) = arg {
            for &(port, found) in names.wildcard(*at) {
                bound.push((port.symbol, Bound::Name(found, *at)));
            }
        }
    }
    bound
}

/// What each port of its definition the instance `at` connects is
/// connected to, as [`bind`] lists it; the port is the definition's symbol,
/// what connects it the instance's.
pub(crate) fn connections(design: &Design, at: SymbolRef) -> Vec<(SymbolId, Bound)> {
    let Some((instance, definition)) = instance(design, at) else {
        return Vec::new();
    };
    let SymbolKind::Definition { ports, .. } = &design.symbol(definition).kind else {
        unreachable!("a definition's name names a definition");
    };
    let names = design.names(at.file);
    bind(
        &design[definition.file],
        ports,
        &instance.connections,
        names,
    )
}

/// The value each parameter of its definition the instance `at` overrides
/// is given, in the order written. A name alone gives a parameter nothing.
pub(crate) fn overrides(design: &Design, at: SymbolRef) -> Vec<(SymbolId, ExprId)> {
    let Some((instance, definition)) = instance(design, at) else {
        return Vec::new();
    };
    let SymbolKind::Definition { overridable, .. } = &design.symbol(definition).kind else {
        unreachable!("a definition's name names a definition");
    };
    let hir = &design[definition.file];
    let names = design.names(at.file);
    (bind(hir, overridable, &instance.parameters, names).into_iter())
        .filter_map(|(parameter, given)| match given {
            Bound::Expr(expr) => Some((parameter, expr)),
            Bound::Name(..) => None,
        })
        .collect()
}

/// The instance `at` and the definition it instantiates.
fn instance(design: &Design, at: SymbolRef) -> Option<(&Instance, SymbolRef)> {
    let SymbolKind::Instance(instance) = &design.symbol(at).kind else {
        return None;
    };
    Some((instance, design.instantiated(at)?))
}
