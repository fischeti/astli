//! Lexer behaviour, and the invariant the whole crate rests on.
//!
//! Token *kinds* have no external oracle yet — round-tripping proves only that
//! every byte lands in exactly one token, not that it was labelled correctly.
//! What stands in for one is [`audits`], a set of realistic snippets with their
//! full kind sequences written out, chosen to cover the places where a wrong
//! label is plausible. See `docs/limitations.md`.

use svirig_syntax::{SyntaxKind, SyntaxKind::*, Token, tokenize};

mod corpus;

/// Non-trivia kinds, which is what the audits are written against.
fn kinds(source: &str) -> Vec<SyntaxKind> {
    tokenize(source)
        .into_iter()
        .map(|t| t.kind)
        .filter(|k| !k.is_trivia() && *k != EOF)
        .collect()
}

/// Every byte in exactly one token, in order, nothing empty but the final EOF.
fn assert_gapless(source: &str, tokens: &[Token]) {
    let (last, body) = tokens.split_last().expect("always at least EOF");
    assert_eq!(last.kind, EOF);
    assert!(last.is_empty());

    let mut at = 0u32;
    for token in body {
        assert_eq!(token.start, at, "gap or overlap before {token:?}");
        assert!(token.end > token.start, "empty token {token:?}");
        at = token.end;
    }
    assert_eq!(at as usize, source.len(), "input not fully covered");

    let rebuilt: String = body.iter().map(|t| t.text(source)).collect();
    assert_eq!(rebuilt, source, "token texts do not reassemble the source");
}

#[test]
fn empty_input_is_just_eof() {
    let tokens = tokenize("");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].kind, EOF);
    assert_gapless("", &tokens);
}

#[test]
fn identifiers_become_keywords() {
    assert_eq!(
        kinds("module foo; endmodule"),
        [MODULE_KW, IDENT, SEMICOLON, ENDMODULE_KW]
    );
    // An escaped identifier never reaches the lookup, which is the entire
    // point of escaping it.
    assert_eq!(kinds("\\module "), [ESCAPED_IDENT]);
    // Nor does anything that merely contains a keyword.
    assert_eq!(kinds("module_a modulea amodule"), [IDENT, IDENT, IDENT]);
}

#[test]
fn unlexable_bytes_become_one_error_token() {
    // A run of bytes no rule matches is reported once, not per byte.
    assert_eq!(kinds("a €€€ b"), [IDENT, LEX_ERROR, IDENT]);
    let source = "a €€€ b";
    assert_gapless(source, &tokenize(source));
}

/// Realistic snippets with their kind sequences spelled out, standing in for an
/// external oracle. Each is here because a wrong label is plausible, not
/// because the construct is common.
///
/// Hand-formatted: a kind sequence is only readable if it is grouped the way
/// the source is, and rustfmt would put each of the ~30 kinds on its own line.
#[rustfmt::skip]
mod audits {
    use crate::kinds;
    use svirig_syntax::SyntaxKind::*;

    #[test]
    fn module_header() {
        assert_eq!(
            kinds("module foo #(parameter int W = 8) (input logic clk_i, output logic [W-1:0] d_o);"),
            [
                MODULE_KW, IDENT,
                HASH, L_PAREN, PARAMETER_KW, INT_KW, IDENT, EQ, INT_LITERAL, R_PAREN,
                L_PAREN,
                    INPUT_KW, LOGIC_KW, IDENT, COMMA,
                    OUTPUT_KW, LOGIC_KW, L_BRACK, IDENT, MINUS, INT_LITERAL, COLON, INT_LITERAL,
                        R_BRACK, IDENT,
                R_PAREN, SEMICOLON,
            ]
        );
    }

    #[test]
    fn nonblocking_assignment_is_not_a_comparison() {
        // `<=` is one glyph doing two jobs; the lexer commits to neither.
        assert_eq!(
            kinds("always_ff @(posedge clk_i) q <= d;"),
            [
                ALWAYS_FF_KW, AT, L_PAREN, POSEDGE_KW, IDENT, R_PAREN,
                IDENT, LT_EQ, IDENT, SEMICOLON,
            ]
        );
        assert_eq!(
            kinds("if (a <= b) x = 1;"),
            [
                IF_KW, L_PAREN, IDENT, LT_EQ, IDENT, R_PAREN,
                IDENT, EQ, INT_LITERAL, SEMICOLON,
            ]
        );
    }

    #[test]
    fn concurrent_assertion() {
        assert_eq!(
            kinds("assert property (@(posedge clk) a |-> ##1 b);"),
            [
                ASSERT_KW, PROPERTY_KW, L_PAREN,
                    AT, L_PAREN, POSEDGE_KW, IDENT, R_PAREN,
                    IDENT, PIPE_MINUS_GT, HASH_HASH, INT_LITERAL, IDENT,
                R_PAREN, SEMICOLON,
            ]
        );
    }

    #[test]
    fn literals_casts_and_patterns() {
        assert_eq!(
            kinds("logic [7:0] x = 8'hFF; t y = '{default: 0}; int z = int'(w);"),
            [
                LOGIC_KW, L_BRACK, INT_LITERAL, COLON, INT_LITERAL, R_BRACK, IDENT,
                    EQ, INT_LITERAL, BASED_LITERAL, SEMICOLON,
                IDENT, IDENT,
                    EQ, APOSTROPHE_L_BRACE, DEFAULT_KW, COLON, INT_LITERAL, R_BRACE, SEMICOLON,
                INT_KW, IDENT,
                    EQ, INT_KW, APOSTROPHE, L_PAREN, IDENT, R_PAREN, SEMICOLON,
            ]
        );
    }

    #[test]
    fn ranges_and_streaming() {
        assert_eq!(
            kinds("q[1:$]; d[i +: 8]; e = {<<8{f}};"),
            [
                IDENT, L_BRACK, INT_LITERAL, COLON, DOLLAR, R_BRACK, SEMICOLON,
                IDENT, L_BRACK, IDENT, PLUS_COLON, INT_LITERAL, R_BRACK, SEMICOLON,
                IDENT, EQ, L_BRACE, LT_LT, INT_LITERAL, L_BRACE, IDENT, R_BRACE, R_BRACE, SEMICOLON,
            ]
        );
    }

    #[test]
    fn macro_call_with_arguments() {
        // The introducer is one token and the arguments are ordinary tokens,
        // which is what lets the preprocessor see inside the call.
        assert_eq!(
            kinds(r#"`uvm_info("T", $sformatf("%0d", x), UVM_LOW)"#),
            [
                DIRECTIVE, L_PAREN,
                    STRING_LITERAL, COMMA,
                    SYSTEM_IDENT, L_PAREN, STRING_LITERAL, COMMA, IDENT, R_PAREN, COMMA,
                    IDENT,
                R_PAREN,
            ]
        );
    }

    #[test]
    fn conditional_directives() {
        assert_eq!(
            kinds("`ifdef A\nwire w;\n`else\nreg r;\n`endif"),
            [
                DIRECTIVE, IDENT,
                WIRE_KW, IDENT, SEMICOLON,
                DIRECTIVE,
                REG_KW, IDENT, SEMICOLON,
                DIRECTIVE,
            ]
        );
    }

    #[test]
    fn delays_and_timing() {
        assert_eq!(
            kinds("#10ns; #1step; ##[1:$] a;"),
            [
                HASH, TIME_LITERAL, SEMICOLON,
                HASH, ONE_STEP_KW, SEMICOLON,
                HASH_HASH, L_BRACK, INT_LITERAL, COLON, DOLLAR, R_BRACK, IDENT, SEMICOLON,
            ]
        );
    }
}

/// `` `define ``, the one place the lexer stops reading ordinary SystemVerilog.
///
/// These are written against every token rather than the non-trivia ones,
/// because the rule under test moves a byte from one trivia token to another.
#[rustfmt::skip]
mod define_bodies {
    use svirig_syntax::{SyntaxKind, tokenize, SyntaxKind::*};

    use super::assert_gapless;

    fn spans(source: &str) -> Vec<(SyntaxKind, &str)> {
        let tokens = tokenize(source);
        assert_gapless(source, &tokens);
        tokens
            .iter()
            .filter(|t| t.kind != EOF)
            .map(|t| (t.kind, t.text(source)))
            .collect()
    }

    #[test]
    fn a_continuation_survives_a_line_comment() {
        // Commenting the lines of a long macro is ordinary practice, and the
        // `//` rule would otherwise swallow the `\` and end the definition on
        // the comment's line.
        assert_eq!(
            spans("`define A \\\n  // why \\\n  b\n"),
            [
                (DIRECTIVE, "`define"), (WHITESPACE, " "), (IDENT, "A"),
                (WHITESPACE, " "), (LINE_CONTINUATION, "\\\n"),
                (WHITESPACE, "  "), (LINE_COMMENT, "// why "), (LINE_CONTINUATION, "\\\n"),
                (WHITESPACE, "  "), (IDENT, "b"), (WHITESPACE, "\n"),
            ]
        );
    }

    #[test]
    fn and_survives_it_with_crlf() {
        assert_eq!(
            spans("`define A // c \\\r\nb\r\n"),
            [
                (DIRECTIVE, "`define"), (WHITESPACE, " "), (IDENT, "A"), (WHITESPACE, " "),
                (LINE_COMMENT, "// c "), (LINE_CONTINUATION, "\\\r\n"),
                (IDENT, "b"), (WHITESPACE, "\r\n"),
            ]
        );
    }

    #[test]
    fn an_ordinary_comment_keeps_its_backslash() {
        // Outside a definition a trailing `\` continues nothing, so taking it
        // out of the comment would invent a token.
        assert_eq!(
            spans("wire w; // trailing \\\nwire x;"),
            [
                (WIRE_KW, "wire"), (WHITESPACE, " "), (IDENT, "w"), (SEMICOLON, ";"),
                (WHITESPACE, " "), (LINE_COMMENT, "// trailing \\"), (WHITESPACE, "\n"),
                (WIRE_KW, "wire"), (WHITESPACE, " "), (IDENT, "x"), (SEMICOLON, ";"),
            ]
        );
    }

    #[test]
    fn a_definition_ends_at_an_uncontinued_newline() {
        // The comment on the following line is no longer part of anything.
        assert_eq!(
            spans("`define A 1\n// c \\\nx"),
            [
                (DIRECTIVE, "`define"), (WHITESPACE, " "), (IDENT, "A"),
                (WHITESPACE, " "), (INT_LITERAL, "1"), (WHITESPACE, "\n"),
                (LINE_COMMENT, "// c \\"), (WHITESPACE, "\n"), (IDENT, "x"),
            ]
        );
    }

    #[test]
    fn a_commented_out_define_starts_nothing() {
        // The directive never becomes a token, so there is no definition to be
        // inside of.
        assert_eq!(
            spans("// `define A \\\nx"),
            [(LINE_COMMENT, "// `define A \\"), (WHITESPACE, "\n"), (IDENT, "x")]
        );
    }

    #[test]
    fn a_trailing_backslash_at_eof_continues_nothing() {
        assert_eq!(
            spans("`define A // c \\"),
            [
                (DIRECTIVE, "`define"), (WHITESPACE, " "), (IDENT, "A"), (WHITESPACE, " "),
                (LINE_COMMENT, "// c \\"),
            ]
        );
    }

    #[test]
    fn stringification_and_pasting_are_tokens_in_a_body() {
        // A body is lexed rather than held as text, and these are the pieces
        // expansion has to act on.
        assert_eq!(
            spans("`define S(x) `\"x`\"")
                .into_iter()
                .filter(|(k, _)| !k.is_trivia())
                .collect::<Vec<_>>(),
            [
                (DIRECTIVE, "`define"), (IDENT, "S"),
                (L_PAREN, "("), (IDENT, "x"), (R_PAREN, ")"),
                (MACRO_QUOTE, "`\""), (IDENT, "x"), (MACRO_QUOTE, "`\""),
            ]
        );
        assert_eq!(
            spans("`define C(a, b) a``b")
                .into_iter()
                .filter(|(k, _)| !k.is_trivia())
                .collect::<Vec<_>>(),
            [
                (DIRECTIVE, "`define"), (IDENT, "C"),
                (L_PAREN, "("), (IDENT, "a"), (COMMA, ","), (IDENT, "b"), (R_PAREN, ")"),
                (IDENT, "a"), (MACRO_PASTE, "``"), (IDENT, "b"),
            ]
        );
    }
}

/// The corpus round-trip. Skipped, loudly, when `corpus/` has not been fetched.
#[test]
fn corpus_round_trips() {
    let Some(files) = corpus::files() else {
        return;
    };

    let mut total_tokens = 0usize;
    let mut errors = Vec::new();

    for path in &files {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue; // not UTF-8; not ours to lex
        };
        let tokens = tokenize(&source);
        total_tokens += tokens.len();
        assert_gapless(&source, &tokens);

        for token in &tokens {
            if token.kind == LEX_ERROR {
                errors.push(format!(
                    "{}: {:?}",
                    path.display(),
                    token.text(&source).chars().take(20).collect::<String>()
                ));
            }
        }
    }

    eprintln!("{} files, {total_tokens} tokens", files.len());
    assert!(
        errors.is_empty(),
        "{} unlexable spans, first few:\n{}",
        errors.len(),
        errors
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
