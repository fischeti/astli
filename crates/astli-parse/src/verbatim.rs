//! Fallback parser for unrecognised syntactic regions.
//!
//! When grammar rules fail to match a construct, the parser falls back to consuming
//! a balanced run of tokens into a [`VERBATIM`] node.
//! This ensures that formatting and tooling operations can proceed without syntax errors,
//! preserving unrecognised regions verbatim while retaining correct parentage and delimiters.
//!
//! ### Delimiter Tracking
//!
//! Verbatim recovery maintains a stack of what is open, by the pairs of
//! [`delimiters`]. Runs terminate at semicolons or commas at depth zero, or
//! when encountering an unmatched closing delimiter.

use super::delimiters::{self, Statement};
use super::event::{Completed, Marker};
use super::source::Tokens;
use super::{Parser, preprocessor};
use astli_syntax::{SyntaxKind, SyntaxKind::*};
use astli_text::Span;

/// Boundary context governing where a verbatim recovery run may terminate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// Statement, item, or class member terminated by a semicolon or matching closer.
    Terminated,
    /// List element bounded by a comma or closing delimiter.
    Element,
}

/// Consumes a balanced run of tokens into a [`VERBATIM`] node.
pub fn verbatim<T: Tokens>(parser: &mut Parser<T>, context: Context) -> Completed {
    let marker = parser.start();
    let from = parser.position();
    // Each open pair, by its index, with the opener and where it stands.
    let mut open: Vec<(usize, SyntaxKind, Option<Span>)> = Vec::new();
    let mut declaring = false;
    let mut previous = EOF;
    let mut taken = 0;

    while !parser.at_end() {
        let kind = parser.kind(0);

        if context == Context::Element && open.is_empty() && taken > 0 && kind == COMMA {
            break;
        }

        // Too deep, a directive is tokens like any other: parsing it would
        // open a region, and the region an item, and the item this again.
        if kind == TICK_IDENT && !parser.too_deep() && preprocessor::any(parser) {
            previous = kind;
            taken += 1;
            continue;
        }

        let pair = delimiters::pair(kind);
        if let Some((pair, false)) = pair {
            if open.last().is_some_and(|&(at, ..)| at == pair) {
                open.pop();
                parser.bump();
                taken += 1;
                if open.is_empty() && delimiters::bracket_depth(kind) == 0 {
                    parser.label();
                    break;
                }
                continue;
            }
            if taken == 0 {
                parser.bump();
                parser.label();
            }
            break;
        }

        let statement = Statement {
            next: [parser.kind(1), parser.kind(2)],
            declaring,
        };
        if let Some((pair, true)) = pair
            && delimiters::opens(kind, previous, statement)
        {
            open.push((pair, kind, parser.span()));
        } else if delimiters::declares(kind) {
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

    // An envelope may hold the closer, decrypted, and so may what lies past
    // a fence.
    if parser.at_eof()
        && !parser.enveloped_from(from)
        && let Some(&(_, opener, Some(at))) = open.last()
        && let Some(&closer) = delimiters::closers(opener).first()
    {
        let (opener, closer) = (spelling(opener), spelling(closer));
        parser
            .events
            .report(super::diagnostics::unclosed_at_end(opener, closer, at));
    }

    parser.complete(marker, VERBATIM)
}

/// Takes the construct at the cursor as [`verbatim`] does, if it is nested
/// too deep to parse, and says whether it did.
pub fn too_deep<T: Tokens>(parser: &mut Parser<T>) -> bool {
    if !parser.too_deep() || parser.at_end() {
        return false;
    }
    report_too_deep(parser);
    verbatim(parser, Context::Terminated);
    true
}

/// Takes the rest of an operand nested too deep to parse, up to a `,` or `;`
/// or a closer it did not open, or `None` if the cursor is at one.
pub fn too_deep_operand<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    let end = |kind| matches!(kind, COMMA | SEMICOLON) || delimiters::is_closer(kind);
    if parser.at_end() || end(parser.kind(0)) {
        return None;
    }
    report_too_deep(parser);
    let marker = parser.start();
    balanced(parser, end);
    Some(parser.complete(marker, VERBATIM))
}

/// Takes what a rule left of a group before the `close` that ends it as
/// [`VERBATIM`], so that it is reported and counted like any other text the
/// grammar did not cover, then the `close`, and completes the group as a
/// `kind`.
pub fn rest_as_verbatim<T: Tokens>(
    parser: &mut Parser<T>,
    group: Marker,
    close: SyntaxKind,
    kind: SyntaxKind,
) -> Completed {
    if !parser.at(close) && !parser.at_end() {
        let rest = parser.start();
        balanced(parser, |kind| kind == close);
        parser.complete(rest, VERBATIM);
    }
    if parser.at(close) {
        parser.bump();
    }
    parser.complete(group, kind)
}

/// Bumps tokens up to the first outside any bracket that `end` holds for,
/// or the end of the stream. Only brackets nest: inside a group the grammar
/// expects an expression, where a keyword opens no body.
fn balanced<T: Tokens>(parser: &mut Parser<T>, end: impl Fn(SyntaxKind) -> bool) {
    let mut depth = 0u32;
    while !parser.at_end() {
        let kind = parser.kind(0);
        if depth == 0 && end(kind) {
            break;
        }
        depth = depth.saturating_add_signed(delimiters::bracket_depth(kind));
        parser.bump();
    }
}

fn report_too_deep<T: Tokens>(parser: &mut Parser<T>) {
    if let Some(at) = parser.span() {
        parser
            .events
            .report(super::diagnostics::nested_too_deep(at, crate::MAX_NESTING));
    }
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

#[cfg(test)]
mod tests {
    //! What one run takes, and where it stops, with nothing around it.

    use super::*;
    use crate::Raw;
    use crate::testing;

    fn one(text: &str, context: Context) -> (String, String) {
        let (taken, rest) = testing::one(text, |parser: &mut Parser<Raw<'_>>| {
            verbatim(parser, context);
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
            "export \"DPI-C\" function f; logic after;",
            "extern module m(input a); logic after;",
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
        // A forward declaration, a type, an inline assertion, a reference
        // to a block declared elsewhere, and a use of the forked processes:
        // none has an `end...` to find.
        for (text, expected) in [
            ("typedef class C; logic after;", "typedef class C;"),
            ("disable fork; logic after;", "disable fork;"),
            ("wait fork; logic after;", "wait fork;"),
            ("default clocking cb; logic after;", "default clocking cb;"),
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
    fn what_a_body_holds_that_opens_nothing_does_not_hold_its_closer() {
        // Each of these would otherwise push something the body's own closer
        // does not close, and the run would stop short at it.
        for text in [
            "module m(interface.mp a, interface b); endmodule",
            "property p(sequence s); s; endproperty",
            "covergroup g with function sample(bit x); endgroup",
            "initial randcase 1: a = 1; endcase",
        ] {
            let (taken, rest) = one(&format!("{text} after"), Context::Terminated);
            assert_eq!(taken, text);
            assert_eq!(rest, " after", "{text:?}");
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

        // And when the closer is a stray one.
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
