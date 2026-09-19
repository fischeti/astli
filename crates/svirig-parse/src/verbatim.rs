//! The fallback: leave this region's bytes alone.
//!
//! Annex A is 747 productions and full coverage is a multi-year slog. A
//! formatter that degrades to *leave this alone* on what it does not
//! recognise can be genuinely useful at a fraction of that, and every existing
//! tool fails hard instead -- which is the gap worth exploiting. See
//! `docs/plan.md`.
//!
//! So a rule that cannot make progress hands over here, and what comes back is
//! a [`VERBATIM`] node holding a **balanced** run of tokens that the formatter
//! emits untouched and resyncs after.
//!
//! # Balanced, and bounded
//!
//! The run ends at the `;` that terminates it, or before the `,` that
//! separates it from the next list element -- but only at the outermost level,
//! so a `;` inside a `begin` … `end` or inside parentheses does not end it.
//! That is what the delimiter stack is for.
//!
//! The stack holds *what opened*, not a count, and **a closer that does not
//! match the top of it ends the run** rather than being swallowed. That is the
//! safety net: whatever else goes wrong, a run cannot escape past the
//! `endmodule` of the module it started in, because `endmodule` closes nothing
//! it opened.
//!
//! # The keywords that only sometimes open anything
//!
//! `begin` always opens a block and `end` always closes one. The larger
//! constructs are not so tidy -- the same keyword introduces a body in one
//! place and names a type or declares a prototype in another:
//!
//! | written | opens a body | does not |
//! | --- | --- | --- |
//! | `function` | `function f(); … endfunction` | `extern function f();`, `pure virtual function f();`, `import "DPI-C" function f();` |
//! | `class` | `class C; … endclass` | `typedef class C;` |
//! | `interface` | `interface i; … endinterface` | `virtual interface i vif;`, `interface class C; … endclass` |
//! | `property` | `property p; … endproperty` | `assert property (…);` |
//! | `sequence` | `sequence s; … endsequence` | `expect (s);` |
//!
//! Pushing one of those wrongly is what makes a run escape, so each is tested
//! for the shape that means a body. The tests are heuristics and they are
//! written down as such in `docs/limitations.md`; the mismatch rule above is
//! what keeps a wrong guess from costing more than one construct.

use super::event::Completed;
use super::source::{Position, Tokens};
use super::{Parser, preprocessor};
use svirig_syntax::{SyntaxKind, SyntaxKind::*};
use svirig_text::TokenOrigin;

/// What a run is being recovered inside, and so where it may stop.
///
/// Two, not the four an item, a member, a statement and a list element would
/// suggest: the first three all end at their own `;` and at anything that
/// closes what encloses them, which the delimiter stack already knows. A
/// finer context is worth adding when a rule needs one, not before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// An item, a member, or a statement. The run ends at the `;` that
    /// terminates it, and takes that `;` with it.
    Terminated,
    /// One element of a parenthesised or braced list. The run ends *before*
    /// the `,` or the closing bracket, which belong to the list rather than to
    /// the element.
    Element,
}

/// Consumes a balanced run of tokens into a [`VERBATIM`] node.
///
/// Always takes at least one token unless the cursor is already at the end or
/// already at `limit`, so a caller that has checked for those can loop on it
/// without checking for progress.
///
/// `limit` is for a caller parsing a stretch of text whose extent it already
/// knows -- a conditional branch. It matters there because a ragged branch
/// does not balance: without a bound, a run inside one goes looking for the
/// `end` that the *next* branch writes and swallows the rest of the region.
pub fn verbatim<T: Tokens>(
    parser: &mut Parser<T>,
    context: Context,
    limit: Option<Position>,
) -> Completed {
    let marker = parser.start();
    // Each opener with where it was written, so that a run which never closes
    // can point at the thing that is open rather than at the end of the file.
    let mut open: Vec<(SyntaxKind, Option<TokenOrigin>)> = Vec::new();
    // Whether what is being read right now announced itself as a declaration
    // rather than as something with a body.
    let mut declaring = false;
    // The token before the cursor, which is what separates `virtual interface`
    // -- a type -- from an `interface` that opens one.
    let mut previous = EOF;
    let mut taken = 0;

    while !parser.at_end() {
        if limit.is_some_and(|limit| parser.position() >= limit) {
            break;
        }

        let kind = parser.kind(0);

        // A list element ends where the next one begins. The `taken` guard is
        // what keeps an empty element from making no progress at all.
        if context == Context::Element && open.is_empty() && taken > 0 && kind == COMMA {
            break;
        }

        // A `` `name `` is an atom: it stands for a value, a name, a type or a
        // whole declaration, and the run has no way of knowing which. Giving
        // it a node here rather than swallowing its tokens is what puts the
        // preprocessor's structure into the tree everywhere, and not only
        // where the grammar already reaches.
        //
        // It is also what keeps the stack honest. The delimiters inside an
        // argument list or a ragged region are not this run's to balance, and
        // never looking inside means they cannot unbalance it.
        if kind == TICK_IDENT && preprocessor::any(parser) {
            previous = kind;
            taken += 1;
            continue;
        }

        if is_closer(kind) {
            if open
                .last()
                .is_some_and(|&(opener, _)| closers(opener).contains(&kind))
            {
                open.pop();
                parser.bump();
                taken += 1;
                // `end` or `endmodule` closing the last thing open means the
                // construct is over, and whatever follows is not this run's.
                // A bracket is no such boundary: `(a + b) + c` carries on.
                if open.is_empty() && !is_bracket(kind) {
                    label(parser, limit);
                    break;
                }
                continue;
            }
            // It closes something this run did not open -- or something
            // further down the stack, which means the nesting is malformed.
            // Either way it is not ours to take, and leaving it is what stops
            // a run escaping past the end of what encloses it.
            if taken == 0 {
                // Except when it is the very first thing, because a caller
                // looping on a stray closer would never get past it.
                parser.bump();
                label(parser, limit);
            }
            break;
        }

        if opens(parser, kind, declaring, previous) {
            open.push((kind, parser.origin()));
        } else if matches!(kind, EXTERN_KW | PURE_KW | IMPORT_KW | TYPEDEF_KW) {
            declaring = true;
        }

        previous = kind;
        parser.bump();
        taken += 1;

        if kind == SEMICOLON {
            // A declaration reaches exactly as far as its own `;`, wherever
            // that sits: `extern function void f();` is a class member, so
            // asking about the outermost level would never see it.
            declaring = false;
            if open.is_empty() {
                break;
            }
        }
    }

    // The text ran out with delimiters still open, which is a fact about the
    // file rather than about how much of Annex A is written: a construct no
    // rule claims still balances, and its run leaves nothing on the stack.
    // Innermost first, because that is the one whose closer is missing.
    if parser.at_end()
        && let Some(&(opener, Some(at))) = open.last()
        && let Some(&closer) = closers(opener).first()
    {
        let (opener, closer) = (spelling(opener), spelling(closer));
        parser.report(super::diagnostics::unclosed_at_end(opener, closer, at));
    }

    parser.complete(marker, VERBATIM)
}

/// How a delimiter is written, for a message that names it.
///
/// `keyword::text` answers for the words; the brackets are not keywords and
/// are spelled here.
fn spelling(kind: SyntaxKind) -> &'static str {
    match kind {
        L_PAREN => "(",
        R_PAREN => ")",
        L_BRACK => "[",
        R_BRACK => "]",
        L_BRACE | APOSTROPHE_L_BRACE => "{",
        R_BRACE => "}",
        kind => svirig_syntax::keyword::text(kind).unwrap_or("?"),
    }
}

/// Takes the `: name` that a closing keyword may carry.
///
/// The label belongs to the construct that just ended, and leaving it costs
/// more than the two tokens suggest: the next run starts on a `:`, balances
/// nothing, and so reaches for whatever follows -- a whole task or class that
/// would otherwise have parsed. If that one closes with a label too, it
/// happens again, and one construct in the fallback becomes a file's worth.
fn label<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    // `endfunction : new` is written, so the name is not always an `IDENT`.
    if parser.kind(0) != COLON || !matches!(parser.kind(1), IDENT | ESCAPED_IDENT | NEW_KW) {
        return;
    }
    if limit.is_some_and(|limit| parser.ahead(2) > limit) {
        return;
    }
    parser.bump();
    parser.bump();
}

/// Whether `kind` opens something this run has to see the end of.
fn opens<T: Tokens>(
    parser: &Parser<T>,
    kind: SyntaxKind,
    declaring: bool,
    previous: SyntaxKind,
) -> bool {
    match kind {
        // Unambiguous: these introduce a body wherever they appear.
        L_PAREN | L_BRACK | L_BRACE | APOSTROPHE_L_BRACE | BEGIN_KW | FORK_KW | CASE_KW
        | CASEX_KW | CASEZ_KW | GENERATE_KW | SPECIFY_KW | TABLE_KW | CONFIG_KW | PRIMITIVE_KW
        | PROGRAM_KW | CHECKER_KW | CLOCKING_KW | COVERGROUP_KW | PACKAGE_KW => true,

        // A prototype has no body: `extern function f();`, and the DPI and
        // pure virtual forms.
        FUNCTION_KW | TASK_KW | MODULE_KW | MACROMODULE_KW => !declaring,

        // `typedef class C;` names a class that is declared elsewhere.
        CLASS_KW => !declaring,

        // `virtual interface` is a type -- the commonest use of the word in
        // verification code -- and `interface class` is a class, which `class`
        // opens for itself and `endclass` closes.
        INTERFACE_KW => !declaring && parser.kind(1) != CLASS_KW && previous != VIRTUAL_KW,

        // A declaration names what it declares; the inline forms --
        // `assert property (…)` -- are followed by their parenthesis.
        PROPERTY_KW | SEQUENCE_KW => parser.kind(1) == IDENT,

        _ => false,
    }
}

/// What closes `opener`.
fn closers(opener: SyntaxKind) -> &'static [SyntaxKind] {
    match opener {
        L_PAREN => &[R_PAREN],
        L_BRACK => &[R_BRACK],
        L_BRACE | APOSTROPHE_L_BRACE => &[R_BRACE],
        BEGIN_KW => &[END_KW],
        FORK_KW => &[JOIN_KW, JOIN_ANY_KW, JOIN_NONE_KW],
        CASE_KW | CASEX_KW | CASEZ_KW => &[ENDCASE_KW],
        MODULE_KW | MACROMODULE_KW => &[ENDMODULE_KW],
        FUNCTION_KW => &[ENDFUNCTION_KW],
        TASK_KW => &[ENDTASK_KW],
        GENERATE_KW => &[ENDGENERATE_KW],
        CLASS_KW => &[ENDCLASS_KW],
        PACKAGE_KW => &[ENDPACKAGE_KW],
        INTERFACE_KW => &[ENDINTERFACE_KW],
        PROGRAM_KW => &[ENDPROGRAM_KW],
        CHECKER_KW => &[ENDCHECKER_KW],
        CONFIG_KW => &[ENDCONFIG_KW],
        PRIMITIVE_KW => &[ENDPRIMITIVE_KW],
        SPECIFY_KW => &[ENDSPECIFY_KW],
        TABLE_KW => &[ENDTABLE_KW],
        SEQUENCE_KW => &[ENDSEQUENCE_KW],
        PROPERTY_KW => &[ENDPROPERTY_KW],
        COVERGROUP_KW => &[ENDGROUP_KW],
        CLOCKING_KW => &[ENDCLOCKING_KW],
        _ => &[],
    }
}

/// Whether `kind` is one of the bracket pairs rather than a keyword.
///
/// A bracket closing is not the end of anything a run has to stop at; a
/// keyword closing is.
fn is_bracket(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        L_PAREN | L_BRACK | L_BRACE | APOSTROPHE_L_BRACE | R_PAREN | R_BRACK | R_BRACE
    )
}

/// Whether `kind` closes anything at all.
fn is_closer(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        R_PAREN
            | R_BRACK
            | R_BRACE
            | END_KW
            | JOIN_KW
            | JOIN_ANY_KW
            | JOIN_NONE_KW
            | ENDCASE_KW
            | ENDMODULE_KW
            | ENDFUNCTION_KW
            | ENDTASK_KW
            | ENDGENERATE_KW
            | ENDCLASS_KW
            | ENDPACKAGE_KW
            | ENDINTERFACE_KW
            | ENDPROGRAM_KW
            | ENDCHECKER_KW
            | ENDCONFIG_KW
            | ENDPRIMITIVE_KW
            | ENDSPECIFY_KW
            | ENDTABLE_KW
            | ENDSEQUENCE_KW
            | ENDPROPERTY_KW
            | ENDGROUP_KW
            | ENDCLOCKING_KW
    )
}
