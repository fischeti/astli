//! What opens a group or a body, and what closes it.
//!
//! The fallback counts these to find where a run it cannot parse ends, and a
//! conditional region is live when each of its branches balances them. Both
//! read this one table, so that they cannot disagree about where something
//! ends.
//!
//! A bracket always opens. A keyword usually does, but most have a form that
//! opens nothing, `disable fork;` or `extern module m;`, which [`opens`] tells
//! apart by the tokens around the keyword.

use astli_syntax::{SyntaxKind, SyntaxKind::*};

/// What tells whether an opener of a pair opens something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Told {
    /// The opener and the token before it.
    ByToken,
    /// The statement around it, which only the fallback follows; region
    /// liveness counts none of these pairs, closers included, since a wrong
    /// guess there would mark ragged a region whose branches each balance.
    ByStatement,
}

use Told::*;

/// Each pair: the tokens that open it, the tokens that close it, and what
/// tells whether an opener opens.
const PAIRS: [(&[SyntaxKind], &[SyntaxKind], Told); 23] = [
    (&[L_PAREN], &[R_PAREN], ByToken),
    (&[L_BRACK], &[R_BRACK], ByToken),
    // An assignment pattern's `'{` is one token, closed by a plain `}`.
    (&[L_BRACE, APOSTROPHE_L_BRACE], &[R_BRACE], ByToken),
    (&[BEGIN_KW], &[END_KW], ByToken),
    (&[FORK_KW], &[JOIN_KW, JOIN_ANY_KW, JOIN_NONE_KW], ByToken),
    (
        &[CASE_KW, CASEX_KW, CASEZ_KW, RANDCASE_KW],
        &[ENDCASE_KW],
        ByToken,
    ),
    (&[MODULE_KW, MACROMODULE_KW], &[ENDMODULE_KW], ByToken),
    (&[PROGRAM_KW], &[ENDPROGRAM_KW], ByToken),
    (&[PRIMITIVE_KW], &[ENDPRIMITIVE_KW], ByToken),
    (&[PACKAGE_KW], &[ENDPACKAGE_KW], ByToken),
    (&[CHECKER_KW], &[ENDCHECKER_KW], ByToken),
    (&[CONFIG_KW], &[ENDCONFIG_KW], ByToken),
    (&[GENERATE_KW], &[ENDGENERATE_KW], ByToken),
    (&[SPECIFY_KW], &[ENDSPECIFY_KW], ByToken),
    (&[TABLE_KW], &[ENDTABLE_KW], ByToken),
    (&[COVERGROUP_KW], &[ENDGROUP_KW], ByToken),
    (&[FUNCTION_KW], &[ENDFUNCTION_KW], ByStatement),
    (&[TASK_KW], &[ENDTASK_KW], ByStatement),
    (&[CLASS_KW], &[ENDCLASS_KW], ByStatement),
    (&[INTERFACE_KW], &[ENDINTERFACE_KW], ByStatement),
    (&[PROPERTY_KW], &[ENDPROPERTY_KW], ByStatement),
    // A `randsequence` always opens, but shares its closer with `sequence`,
    // which does not.
    (
        &[SEQUENCE_KW, RANDSEQUENCE_KW],
        &[ENDSEQUENCE_KW],
        ByStatement,
    ),
    (&[CLOCKING_KW], &[ENDCLOCKING_KW], ByStatement),
];

/// How many pairs there are.
pub(crate) const COUNT: usize = PAIRS.len();

/// The pair `kind` opens or closes, by its index, and whether it is one of
/// the pair's openers.
pub(crate) fn pair(kind: SyntaxKind) -> Option<(usize, bool)> {
    PAIRS.iter().enumerate().find_map(|(at, (open, close, _))| {
        if open.contains(&kind) {
            Some((at, true))
        } else if close.contains(&kind) {
            Some((at, false))
        } else {
            None
        }
    })
}

/// The tokens that close what `opener` opens; none if it is not an opener.
pub(crate) fn closers(opener: SyntaxKind) -> &'static [SyntaxKind] {
    match pair(opener) {
        Some((at, true)) => PAIRS[at].1,
        _ => &[],
    }
}

/// Returns `true` if `kind` closes a group or a body.
pub(crate) fn is_closer(kind: SyntaxKind) -> bool {
    matches!(pair(kind), Some((_, false)))
}

/// How a bracket changes the depth of brackets: `1` for one that opens,
/// `-1` for one that closes, and `0` for any other token.
pub(crate) fn bracket_depth(kind: SyntaxKind) -> i32 {
    match pair(kind) {
        Some((_, opens)) if kind.keyword_text().is_none() => {
            if opens {
                1
            } else {
                -1
            }
        }
        _ => 0,
    }
}

/// Returns `true` if `kind` is a keyword that makes the statement it starts
/// a prototype or a type, whose `function`, `task` or `class` has no body.
pub(crate) fn declares(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        EXTERN_KW | PURE_KW | IMPORT_KW | EXPORT_KW | TYPEDEF_KW
    )
}

/// What the fallback knows of the statement around a keyword.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Statement {
    /// The two tokens after the keyword.
    pub next: [SyntaxKind; 2],
    /// Whether a keyword before it in the statement [`declares`].
    pub declaring: bool,
}

/// Whether `kind` opens a group or a body here, after `previous`, in the
/// `statement`.
pub(crate) fn opens(kind: SyntaxKind, previous: SyntaxKind, statement: Statement) -> bool {
    if !matches!(pair(kind), Some((_, true))) {
        return false;
    }
    let Statement { next, declaring } = statement;
    match kind {
        // `with function sample(…)` in a covergroup's header names the
        // method that samples it.
        FUNCTION_KW | TASK_KW => !declaring && previous != WITH_KW,
        CLASS_KW => !declaring,
        // `virtual interface` types a handle, `interface class` is a class,
        // and `interface.mp` or `(interface i` is a port of any interface.
        INTERFACE_KW => {
            !declaring
                && !matches!(next[0], CLASS_KW | DOT)
                && !matches!(previous, VIRTUAL_KW | L_PAREN | COMMA)
        }
        // `assert property (…)` and `cover sequence (…)` write one inline,
        // and `property p(sequence s)` names a formal's type.
        PROPERTY_KW | SEQUENCE_KW => {
            matches!(next[0], IDENT | ESCAPED_IDENT) && !matches!(previous, L_PAREN | COMMA)
        }
        // `default clocking cb;` and a modport's `clocking cb` name a block
        // declared elsewhere; a declaration has its event before the `;`.
        CLOCKING_KW => next[0] == AT || next[1] == AT,
        _ => by_token(kind, previous),
    }
}

/// How `kind` after `previous` changes the count of the pair it opens or
/// closes, by the pair's index, for a reader that follows no statement:
/// `None` for a token of a pair only the statement tells about.
pub(crate) fn balance(kind: SyntaxKind, previous: SyntaxKind) -> Option<(usize, i32)> {
    let (at, opener) = pair(kind)?;
    match (PAIRS[at].2, opener) {
        (ByStatement, _) => None,
        (ByToken, true) => by_token(kind, previous).then_some((at, 1)),
        (ByToken, false) => Some((at, -1)),
    }
}

/// Whether an opener of a pair the token before tells about opens.
fn by_token(kind: SyntaxKind, previous: SyntaxKind) -> bool {
    match kind {
        // `disable fork;` and `wait fork;` name the processes a block forked.
        FORK_KW => !matches!(previous, DISABLE_KW | WAIT_KW),
        // An `extern` one is its header alone, its body written elsewhere.
        MODULE_KW | MACROMODULE_KW | PROGRAM_KW | PRIMITIVE_KW => previous != EXTERN_KW,
        _ => true,
    }
}
