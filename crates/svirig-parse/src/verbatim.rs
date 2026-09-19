//! Fallback parser for unrecognised syntactic regions.
//!
//! When grammar rules fail to match a construct, the parser falls back to consuming
//! a balanced run of tokens into a [`VERBATIM`] node.
//! This ensures that formatting and tooling operations can proceed without syntax errors,
//! preserving unrecognised regions verbatim while retaining correct parentage and delimiters.
//!
//! ### Delimiter Tracking
//!
//! Verbatim recovery maintains a stack of open delimiter tokens (parentheses, brackets,
//! braces, and block-opening keywords like `begin` or `module`). Runs terminate at semicolons
//! or commas at depth zero, or when encountering an unmatched closing delimiter.

use super::event::Completed;
use super::source::{Position, Tokens};
use super::{Parser, preprocessor};
use svirig_syntax::{SyntaxKind, SyntaxKind::*};
use svirig_text::TokenOrigin;

/// Boundary context governing where a verbatim recovery run may terminate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// Statement, item, or class member terminated by a semicolon or matching closer.
    Terminated,
    /// List element bounded by a comma or closing delimiter.
    Element,
}

/// Consumes a balanced run of tokens into a [`VERBATIM`] node.
///
/// If `limit` is provided, consumption stops before exceeding that token position.
pub fn verbatim<T: Tokens>(
    parser: &mut Parser<T>,
    context: Context,
    limit: Option<Position>,
) -> Completed {
    let marker = parser.start();
    let mut open: Vec<(SyntaxKind, Option<TokenOrigin>)> = Vec::new();
    let mut declaring = false;
    let mut previous = EOF;
    let mut taken = 0;

    while !parser.at_end() {
        if limit.is_some_and(|limit| parser.position() >= limit) {
            break;
        }

        let kind = parser.kind(0);

        if context == Context::Element && open.is_empty() && taken > 0 && kind == COMMA {
            break;
        }

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
                if open.is_empty() && !is_bracket(kind) {
                    label(parser, limit);
                    break;
                }
                continue;
            }
            if taken == 0 {
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
            declaring = false;
            if open.is_empty() {
                break;
            }
        }
    }

    if parser.at_end()
        && let Some(&(opener, Some(at))) = open.last()
        && let Some(&closer) = closers(opener).first()
    {
        let (opener, closer) = (spelling(opener), spelling(closer));
        parser.report(super::diagnostics::unclosed_at_end(opener, closer, at));
    }

    parser.complete(marker, VERBATIM)
}

/// Returns the textual representation of a delimiter or keyword for diagnostics.
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

/// Consumes an optional `: name` label following a closing keyword.
fn label<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    if parser.kind(0) != COLON || !matches!(parser.kind(1), IDENT | ESCAPED_IDENT | NEW_KW) {
        return;
    }
    if limit.is_some_and(|limit| parser.ahead(2) > limit) {
        return;
    }
    parser.bump();
    parser.bump();
}

/// Checks whether `kind` opens a balanced block or delimiter scope.
fn opens<T: Tokens>(
    parser: &Parser<T>,
    kind: SyntaxKind,
    declaring: bool,
    previous: SyntaxKind,
) -> bool {
    match kind {
        L_PAREN | L_BRACK | L_BRACE | APOSTROPHE_L_BRACE | BEGIN_KW | FORK_KW | CASE_KW
        | CASEX_KW | CASEZ_KW | GENERATE_KW | SPECIFY_KW | TABLE_KW | CONFIG_KW | PRIMITIVE_KW
        | PROGRAM_KW | CHECKER_KW | CLOCKING_KW | COVERGROUP_KW | PACKAGE_KW => true,

        FUNCTION_KW | TASK_KW | MODULE_KW | MACROMODULE_KW => !declaring,
        CLASS_KW => !declaring,
        INTERFACE_KW => !declaring && parser.kind(1) != CLASS_KW && previous != VIRTUAL_KW,
        PROPERTY_KW | SEQUENCE_KW => parser.kind(1) == IDENT,

        _ => false,
    }
}

/// Returns the expected closing tokens corresponding to `opener`.
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

/// Returns `true` if `kind` is a bracket, brace, or parenthesis token.
fn is_bracket(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        L_PAREN | L_BRACK | L_BRACE | APOSTROPHE_L_BRACE | R_PAREN | R_BRACK | R_BRACE
    )
}

/// Returns `true` if `kind` closes any delimiter or block construct.
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
