//! Properties of the token inventory that the rest of the crate assumes.

use logos::Logos;
use svirig_syntax::SyntaxKind;
use svirig_syntax::keyword::{self, KEYWORDS_1800_2023 as KEYWORDS, KeywordVersion};

fn lex(src: &str) -> Vec<(SyntaxKind, &str)> {
    let mut lexer = SyntaxKind::lexer(src);
    let mut out = Vec::new();
    while let Some(result) = lexer.next() {
        out.push((result.unwrap_or(SyntaxKind::LEX_ERROR), lexer.slice()));
    }
    out
}

#[test]
fn keyword_table_is_sorted() {
    for pair in KEYWORDS.windows(2) {
        assert!(pair[0].0 < pair[1].0, "{} then {}", pair[0].0, pair[1].0);
    }
}

#[test]
fn keyword_table_round_trips() {
    for &(text, kind) in KEYWORDS {
        assert_eq!(keyword::lookup(text, KeywordVersion::default()), Some(kind));
        assert_eq!(keyword::text(kind), Some(text));
        assert!(kind.is_keyword(), "{text}");
    }
}

#[test]
fn keyword_block_is_contiguous() {
    // `is_keyword` is a range check over the enum, which is only sound while
    // the keyword variants stay in one run. Inserting a non-keyword among them
    // fails here rather than silently.
    let first = KEYWORDS.iter().map(|&(_, k)| k as u16).min().unwrap();
    let last = KEYWORDS.iter().map(|&(_, k)| k as u16).max().unwrap();
    assert_eq!(
        (last - first + 1) as usize,
        KEYWORDS.len(),
        "the keyword variants are no longer one contiguous run"
    );
}

#[test]
fn non_keywords_are_not_keywords() {
    for text in ["foo", "logicx", "xlogic", "", "Module", "clk_i"] {
        assert_eq!(
            keyword::lookup(text, KeywordVersion::default()),
            None,
            "{text}"
        );
    }
    for kind in [
        SyntaxKind::IDENT,
        SyntaxKind::WHITESPACE,
        SyntaxKind::L_PAREN,
        SyntaxKind::STRING_LITERAL,
        SyntaxKind::EOF,
    ] {
        assert!(!kind.is_keyword(), "{kind:?}");
        assert_eq!(keyword::text(kind), None);
    }
}

#[test]
fn one_step_is_a_keyword_outside_the_table() {
    // It begins with a digit, so it never reaches the identifier path.
    assert!(SyntaxKind::ONE_STEP_KW.is_keyword());
    assert_eq!(keyword::text(SyntaxKind::ONE_STEP_KW), Some("1step"));
    assert_eq!(lex("1step"), [(SyntaxKind::ONE_STEP_KW, "1step")]);
}

#[test]
fn numbers_lex_in_pieces() {
    use SyntaxKind::*;

    // Written together, the common case is one base token plus its size.
    assert_eq!(lex("8'hFF"), [(INT_LITERAL, "8"), (BASED_LITERAL, "'hFF")]);
    assert_eq!(lex("1'b0"), [(INT_LITERAL, "1"), (BASED_LITERAL, "'b0")]);
    assert_eq!(lex("'sh1F"), [(BASED_LITERAL, "'sh1F")]);
    assert_eq!(
        lex("4'b1?0x"),
        [(INT_LITERAL, "4"), (BASED_LITERAL, "'b1?0x")]
    );

    // Separated by whitespace -- legal, and the parser has to rejoin it. Note
    // `FF` arrives as an identifier, which is exactly why the join cannot
    // happen in the lexer.
    assert_eq!(
        lex("8 'h FF"),
        [
            (INT_LITERAL, "8"),
            (WHITESPACE, " "),
            (INT_BASE, "'h"),
            (WHITESPACE, " "),
            (IDENT, "FF"),
        ]
    );

    assert_eq!(lex("'0"), [(UNBASED_UNSIZED_LITERAL, "'0")]);
    assert_eq!(lex("'z"), [(UNBASED_UNSIZED_LITERAL, "'z")]);
    assert_eq!(lex("1.8e-3"), [(REAL_LITERAL, "1.8e-3")]);
    assert_eq!(lex("10ns"), [(TIME_LITERAL, "10ns")]);
    assert_eq!(lex("1.5us"), [(TIME_LITERAL, "1.5us")]);
}

#[test]
fn escaped_identifier_keeps_its_terminator() {
    use SyntaxKind::*;

    // The trailing whitespace is part of the token. Without it `\foo bar` and
    // `\foobar` are the same bytes, so a formatter must not be able to reach
    // it.
    assert_eq!(
        lex("\\foo.bar[3] x"),
        [(ESCAPED_IDENT, "\\foo.bar[3] "), (IDENT, "x")]
    );
    // An escaped keyword is a name.
    assert_eq!(lex("\\logic "), [(ESCAPED_IDENT, "\\logic ")]);
}

#[test]
fn apostrophe_forms_are_distinguished() {
    use SyntaxKind::*;

    assert_eq!(
        lex("'{1, 2}"),
        [
            (APOSTROPHE_L_BRACE, "'{"),
            (INT_LITERAL, "1"),
            (COMMA, ","),
            (WHITESPACE, " "),
            (INT_LITERAL, "2"),
            (R_BRACE, "}"),
        ]
    );
    // A cast: the apostrophe stands alone.
    assert_eq!(
        lex("int'(x)"),
        [
            (IDENT, "int"),
            (APOSTROPHE, "'"),
            (L_PAREN, "("),
            (IDENT, "x"),
            (R_PAREN, ")"),
        ]
    );
}

#[test]
fn longest_match_settles_the_operator_overlaps() {
    use SyntaxKind::*;

    assert_eq!(lex("<<<="), [(LT_LT_LT_EQ, "<<<=")]);
    assert_eq!(lex("<<="), [(LT_LT_EQ, "<<=")]);
    assert_eq!(lex("<="), [(LT_EQ, "<=")]);
    assert_eq!(lex("|->"), [(PIPE_MINUS_GT, "|->")]);
    assert_eq!(lex("|=>"), [(PIPE_EQ_GT, "|=>")]);
    assert_eq!(lex("==?"), [(EQ_EQ_QUESTION, "==?")]);
    assert_eq!(lex("&&&"), [(AMP_AMP_AMP, "&&&")]);
    assert_eq!(lex("##"), [(HASH_HASH, "##")]);
    assert_eq!(lex("#-#"), [(HASH_MINUS_HASH, "#-#")]);
}

#[test]
fn directives_and_macro_operators() {
    use SyntaxKind::*;

    // The introducer only; the payload is ordinary tokens.
    assert_eq!(
        lex("`ifdef FOO"),
        [(DIRECTIVE, "`ifdef"), (WHITESPACE, " "), (IDENT, "FOO")]
    );
    assert_eq!(lex("`WIDTH"), [(DIRECTIVE, "`WIDTH")]);
    assert_eq!(lex("`\""), [(MACRO_QUOTE, "`\"")]);
    assert_eq!(lex("``"), [(MACRO_PASTE, "``")]);
    assert_eq!(lex("\\\n"), [(LINE_CONTINUATION, "\\\n")]);
}

#[test]
fn system_identifiers() {
    use SyntaxKind::*;

    assert_eq!(lex("$display"), [(SYSTEM_IDENT, "$display")]);
    assert_eq!(lex("$root"), [(SYSTEM_IDENT, "$root")]);
    // A bare `$` is the open end of a range, not a system name.
    assert_eq!(
        lex("[1:$]"),
        [
            (L_BRACK, "["),
            (INT_LITERAL, "1"),
            (COLON, ":"),
            (DOLLAR, "$"),
            (R_BRACK, "]"),
        ]
    );
}

#[test]
fn comments_and_strings() {
    use SyntaxKind::*;

    assert_eq!(lex("/***/"), [(BLOCK_COMMENT, "/***/")]);
    assert_eq!(lex("// hi"), [(LINE_COMMENT, "// hi")]);
    assert_eq!(lex(r#""a\"b""#), [(STRING_LITERAL, r#""a\"b""#)]);
    // A string may be continued across lines with a backslash.
    assert_eq!(lex("\"a\\\nb\""), [(STRING_LITERAL, "\"a\\\nb\"")]);
}
