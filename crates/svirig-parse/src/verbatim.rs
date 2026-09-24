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
use svirig_text::Span;

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
    let mut open: Vec<(SyntaxKind, Option<Span>)> = Vec::new();
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
            open.push((kind, parser.span()));
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
        parser
            .events
            .report(super::diagnostics::unclosed_at_end(opener, closer, at));
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
        kind => kind.keyword_text().unwrap_or("?"),
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

#[cfg(test)]
mod tests {
    //! What one run takes, and where it stops, with nothing around it.

    use super::*;
    use crate::Raw;
    use crate::testing;

    fn one(text: &str, context: Context) -> (String, String) {
        let (taken, rest) = testing::one(text, |parser: &mut Parser<Raw<'_>>| {
            verbatim(parser, context, None);
            true
        });
        (taken.unwrap_or_default(), rest)
    }

    #[test]
    fn a_semicolon_inside_something_does_not_end_it() {
        let (taken, rest) = one(
            "for (int i = 0; i < 4; i++) x = 1; more",
            Context::Terminated,
        );
        assert_eq!(taken, "for (int i = 0; i < 4; i++) x = 1;");
        assert_eq!(rest, " more");
    }

    #[test]
    fn a_block_is_taken_whole() {
        let (taken, rest) = one(
            "always_ff begin\n  a <= 1;\n  b <= 2;\nend\nnext",
            Context::Terminated,
        );
        assert_eq!(taken, "always_ff begin\n  a <= 1;\n  b <= 2;\nend");
        assert_eq!(rest, "\nnext");
    }

    #[test]
    fn a_run_cannot_escape_past_what_encloses_it() {
        // The `end` belongs to a `begin` this run never saw, so it stops rather
        // than swallowing the rest of the file.
        let (taken, rest) = one("a = 1 end more", Context::Terminated);
        assert_eq!(taken, "a = 1");
        assert_eq!(rest, " end more");
    }

    #[test]
    fn a_stray_closer_still_makes_progress() {
        // Otherwise a caller looping until the end would never get past it.
        let (taken, _) = one("endmodule", Context::Terminated);
        assert_eq!(taken, "endmodule");
    }

    #[test]
    fn a_prototype_has_no_body_to_look_for() {
        // Each of these would otherwise push a `function` that no `endfunction`
        // closes, and the run would eat everything after it.
        for text in [
            "extern function void f(); logic after;",
            "pure virtual function int g(); logic after;",
            "import \"DPI-C\" function void h(); logic after;",
            "extern task t(); logic after;",
        ] {
            let (taken, rest) = one(text, Context::Terminated);
            assert!(taken.ends_with(';'), "{text:?} took {taken:?}");
            assert_eq!(rest, " logic after;", "{text:?}");
        }
    }

    #[test]
    fn a_function_with_a_body_is_taken_whole() {
        let (taken, rest) = one(
            "virtual function int g();\n  return 1;\nendfunction\nafter",
            Context::Terminated,
        );
        assert_eq!(taken, "virtual function int g();\n  return 1;\nendfunction");
        assert_eq!(rest, "\nafter");
    }

    #[test]
    fn the_keywords_that_only_sometimes_open_something() {
        // A forward declaration, a type, and an inline assertion: none of the
        // three has an `end...` to find.
        for (text, expected) in [
            ("typedef class C; logic after;", "typedef class C;"),
            (
                "virtual interface axi_if vif; logic after;",
                "virtual interface axi_if vif;",
            ),
            (
                "assert property (@(posedge clk) a |-> b); logic after;",
                "assert property (@(posedge clk) a |-> b);",
            ),
        ] {
            let (taken, rest) = one(text, Context::Terminated);
            assert_eq!(taken, expected);
            assert_eq!(rest, " logic after;", "{text:?}");
        }
    }

    #[test]
    fn an_interface_class_is_closed_by_endclass() {
        let (taken, rest) = one("interface class C; endclass after", Context::Terminated);
        assert_eq!(taken, "interface class C; endclass");
        assert_eq!(rest, " after");
    }

    #[test]
    fn a_closing_keyword_keeps_its_label() {
        let (taken, rest) = one("covergroup g; endgroup : g after", Context::Terminated);
        assert_eq!(taken, "covergroup g; endgroup : g");
        assert_eq!(rest, " after");

        let (taken, _) = one("function new(); endfunction : new", Context::Terminated);
        assert_eq!(taken, "function new(); endfunction : new");

        // And when the closer is a stray one, which is how a covergroup whose
        // header writes `with function sample(…)` ends up leaving its `endgroup`.
        let (taken, rest) = one("endgroup : g after", Context::Terminated);
        assert_eq!(taken, "endgroup : g");
        assert_eq!(rest, " after");
    }

    #[test]
    fn a_list_element_ends_at_the_comma() {
        let (taken, rest) = one("a + b, c", Context::Element);
        assert_eq!(taken, "a + b");
        assert_eq!(rest, ", c");
    }

    #[test]
    fn a_list_element_keeps_its_own_brackets() {
        let (taken, rest) = one("f(x, y), next", Context::Element);
        assert_eq!(taken, "f(x, y)");
        assert_eq!(rest, ", next");
    }
}
