//! The single flat kind enum used for both tokens and nodes.
//!
//! rowan has no type hierarchy: a tree is built from one `#[repr(u16)]` enum
//! where some variants are leaves carrying text (tokens) and the rest are
//! interior nodes carrying children. Everything here is a token; the node half
//! arrives with the parser.
//!
//! # Keywords are not lexed
//!
//! Every variant up to the keyword block carries a `logos` rule. The 248
//! keyword variants deliberately carry none: an identifier is lexed as
//! [`SyntaxKind::IDENT`] and then looked up in [`crate::keyword`]. Baking 248
//! literals into the `logos` DFA costs a state per distinct keyword prefix,
//! each needing an escape edge back to the identifier rule, which measured at
//! roughly 8x the debug compile time and 15x the object size for no runtime
//! gain. It also keeps the keyword set as data, which is what it wants to be:
//! it is transcribed from Annex B, and the language version that selects it is
//! a parameter of the lookup rather than of the enum.
//!
//! # Operator names spell the glyph
//!
//! Operators are named for how they are written, not for what they mean:
//! `LT_EQ`, never `LESS_EQUAL` or `NONBLOCKING_ASSIGN`. SystemVerilog reuses
//! its punctuation heavily -- `<=` is a relational operator *and* the
//! nonblocking assignment, `#` introduces a delay *and* a parameter list, `->`
//! is an event trigger *and* an implication -- and which one it is depends on
//! where it appears. The lexer does not know, so it does not pretend to.

use logos::Logos;

/// Kinds of tokens and nodes in a SystemVerilog syntax tree.
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum SyntaxKind {
    //--------------------------------------------------------------------
    // Trivia
    //--------------------------------------------------------------------
    // Not skipped. A formatter needs every byte of the input in the tree, so
    // whitespace and comments are ordinary tokens rather than a hidden channel.
    #[regex(r"[ \t\r\n]+")]
    WHITESPACE,
    #[regex(r"//[^\r\n]*", allow_greedy = true)]
    LINE_COMMENT,
    // Written this way rather than `([^*]|\*[^/])*` so that `/***/` matches.
    #[regex(r"/\*([^*]|\*+[^*/])*\*+/")]
    BLOCK_COMMENT,

    //--------------------------------------------------------------------
    // Identifiers
    //--------------------------------------------------------------------
    /// A simple identifier (1800-2023 5.6), or a keyword before
    /// [`crate::keyword::lookup`] has had a say.
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_$]*")]
    IDENT,
    /// An escaped identifier (5.6.1): `\`, then any non-whitespace, terminated
    /// by whitespace.
    ///
    /// The terminating whitespace is *part of the token*, and a formatter may
    /// not collapse it -- without it `\foo bar` and `\foobar` are the same
    /// bytes. Keeping it inside the token is what makes that unrepresentable
    /// rather than merely discouraged.
    #[regex(r"\\[^ \t\r\n]+[ \t\r\n]")]
    ESCAPED_IDENT,
    /// A system task or function name (5.6.3): `$display`, `$clog2`.
    ///
    /// `$root` and `$unit` lex here too; they are ordinary system names as far
    /// as the lexer is concerned.
    #[regex(r"\$[a-zA-Z0-9_$]+")]
    SYSTEM_IDENT,

    //--------------------------------------------------------------------
    // Literals
    //--------------------------------------------------------------------
    // Numbers are lexed in pieces and joined by the parser. `8 'h FF` is one
    // legal integer literal (5.7.1) whose three parts may be separated by
    // whitespace and even comments, so no context-free rule can produce it as
    // a single token.
    /// A run of decimal digits: the size of a sized literal, or a whole
    /// unsigned number.
    #[regex(r"[0-9][0-9_]*")]
    INT_LITERAL,
    /// A base specifier and its digits, written together: `'h1F`, `'sb10x`.
    ///
    /// The common case gets its own rule only because splitting it would be
    /// worse: `'h1F` would otherwise come out as three tokens, since `1F` is
    /// neither an integer nor an identifier.
    #[regex(r"'[sS]?[bBoOdDhH][0-9a-fA-FxXzZ?][0-9a-fA-FxXzZ?_]*")]
    BASED_LITERAL,
    /// A base specifier whose digits are separated from it by whitespace or a
    /// comment, as 5.7.1 permits. The digits follow as their own token -- an
    /// [`SyntaxKind::INT_LITERAL`] or, for `'h FF`, an
    /// [`SyntaxKind::IDENT`] -- and the parser rejoins them.
    #[regex(r"'[sS]?[bBoOdDhH]")]
    INT_BASE,
    /// `'0`, `'1`, `'x`, `'z` (5.7.1) -- fill the width with that value.
    #[regex(r"'[01xXzZ]")]
    UNBASED_UNSIZED_LITERAL,
    /// A real literal (5.7.2): `1.0`, `1e9`, `1.8e-3`.
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*([eE][+-]?[0-9][0-9_]*)?")]
    #[regex(r"[0-9][0-9_]*[eE][+-]?[0-9][0-9_]*")]
    REAL_LITERAL,
    /// A time literal (5.8): `10ns`, `1.5us`.
    #[regex(r"[0-9][0-9_]*(\.[0-9][0-9_]*)?(s|ms|us|ns|ps|fs)")]
    TIME_LITERAL,
    /// The `1step` delay value (1800-2023 14.3), which is a keyword that
    /// begins with a digit and so cannot go through the identifier path.
    #[token("1step")]
    ONE_STEP_KW,
    /// A string literal (5.9), including `\`-continuations across lines.
    #[regex(r#""([^"\\\r\n]|\\(.|\r?\n))*""#)]
    STRING_LITERAL,

    //--------------------------------------------------------------------
    // Preprocessor
    //--------------------------------------------------------------------
    // A directive is lexed as its introducing token only; its payload is
    // ordinary tokens. Macros carry arguments and appear in expression
    // position, so the preprocessor has to see inside them -- an opaque
    // run-to-end-of-line token would hide exactly what it needs.
    // See `docs/preprocessor.md`.
    /// A directive introducer: `` `define ``, `` `ifdef ``, `` `include ``.
    ///
    /// Which directive it is comes from the token text, not from the kind --
    /// there are ~25 of them and the preprocessor dispatches on text anyway.
    #[regex(r"`[a-zA-Z_][a-zA-Z0-9_$]*")]
    DIRECTIVE,
    /// `` `" `` -- open or close a stringified macro body (22.5.1).
    #[token("`\"")]
    MACRO_QUOTE,
    /// `` `\`" `` -- an escaped quote inside a stringified macro body.
    #[token("`\\`\"")]
    MACRO_ESCAPED_QUOTE,
    /// ``` `` ``` -- the token-pasting operator.
    #[token("``")]
    MACRO_PASTE,
    /// A backslash-newline line continuation, which ends a `` `define `` body
    /// line without ending the definition.
    #[regex(r"\\\r?\n")]
    LINE_CONTINUATION,

    //--------------------------------------------------------------------
    // Punctuation and operators
    //--------------------------------------------------------------------
    #[token("(")]
    L_PAREN,
    #[token(")")]
    R_PAREN,
    #[token("[")]
    L_BRACK,
    #[token("]")]
    R_BRACK,
    #[token("{")]
    L_BRACE,
    #[token("}")]
    R_BRACE,
    #[token("'{")]
    APOSTROPHE_L_BRACE,
    #[token("'")]
    APOSTROPHE,
    #[token(";")]
    SEMICOLON,
    #[token(",")]
    COMMA,
    #[token(".")]
    DOT,
    #[token("?")]
    QUESTION,
    #[token(":")]
    COLON,
    #[token("::")]
    COLON_COLON,
    #[token(":=")]
    COLON_EQ,
    #[token(":/")]
    COLON_SLASH,
    #[token("@")]
    AT,
    #[token("@@")]
    AT_AT,
    #[token("#")]
    HASH,
    #[token("##")]
    HASH_HASH,
    #[token("#-#")]
    HASH_MINUS_HASH,
    #[token("#=#")]
    HASH_EQ_HASH,
    #[token("$")]
    DOLLAR,
    #[token("+")]
    PLUS,
    #[token("++")]
    PLUS_PLUS,
    #[token("+=")]
    PLUS_EQ,
    #[token("+:")]
    PLUS_COLON,
    #[token("+/-")]
    PLUS_SLASH_MINUS,
    #[token("+%-")]
    PLUS_PERCENT_MINUS,
    #[token("-")]
    MINUS,
    #[token("--")]
    MINUS_MINUS,
    #[token("-=")]
    MINUS_EQ,
    #[token("-:")]
    MINUS_COLON,
    #[token("->")]
    MINUS_GT,
    #[token("->>")]
    MINUS_GT_GT,
    #[token("*")]
    STAR,
    #[token("**")]
    STAR_STAR,
    #[token("*=")]
    STAR_EQ,
    #[token("*>")]
    STAR_GT,
    #[token("/")]
    SLASH,
    #[token("/=")]
    SLASH_EQ,
    #[token("%")]
    PERCENT,
    #[token("%=")]
    PERCENT_EQ,
    #[token("=")]
    EQ,
    #[token("==")]
    EQ_EQ,
    #[token("===")]
    EQ_EQ_EQ,
    #[token("==?")]
    EQ_EQ_QUESTION,
    #[token("=>")]
    EQ_GT,
    #[token("!")]
    BANG,
    #[token("!=")]
    BANG_EQ,
    #[token("!==")]
    BANG_EQ_EQ,
    #[token("!=?")]
    BANG_EQ_QUESTION,
    #[token("<")]
    LT,
    #[token("<=")]
    LT_EQ,
    #[token("<<")]
    LT_LT,
    #[token("<<<")]
    LT_LT_LT,
    #[token("<<=")]
    LT_LT_EQ,
    #[token("<<<=")]
    LT_LT_LT_EQ,
    #[token("<->")]
    LT_MINUS_GT,
    #[token(">")]
    GT,
    #[token(">=")]
    GT_EQ,
    #[token(">>")]
    GT_GT,
    #[token(">>>")]
    GT_GT_GT,
    #[token(">>=")]
    GT_GT_EQ,
    #[token(">>>=")]
    GT_GT_GT_EQ,
    #[token("&")]
    AMP,
    #[token("&&")]
    AMP_AMP,
    #[token("&&&")]
    AMP_AMP_AMP,
    #[token("&=")]
    AMP_EQ,
    #[token("|")]
    PIPE,
    #[token("||")]
    PIPE_PIPE,
    #[token("|=")]
    PIPE_EQ,
    #[token("|->")]
    PIPE_MINUS_GT,
    #[token("|=>")]
    PIPE_EQ_GT,
    #[token("^")]
    CARET,
    #[token("^=")]
    CARET_EQ,
    #[token("^~")]
    CARET_TILDE,
    #[token("~")]
    TILDE,
    #[token("~&")]
    TILDE_AMP,
    #[token("~|")]
    TILDE_PIPE,
    #[token("~^")]
    TILDE_CARET,

    //--------------------------------------------------------------------
    // Keywords (1800-2023 Annex B)
    //--------------------------------------------------------------------
    // No `logos` rules here -- see the module docs. `crate::keyword::lookup`
    // turns an IDENT into one of these.
    ACCEPT_ON_KW,
    ALIAS_KW,
    ALWAYS_KW,
    ALWAYS_COMB_KW,
    ALWAYS_FF_KW,
    ALWAYS_LATCH_KW,
    AND_KW,
    ASSERT_KW,
    ASSIGN_KW,
    ASSUME_KW,
    AUTOMATIC_KW,
    BEFORE_KW,
    BEGIN_KW,
    BIND_KW,
    BINS_KW,
    BINSOF_KW,
    BIT_KW,
    BREAK_KW,
    BUF_KW,
    BUFIF0_KW,
    BUFIF1_KW,
    BYTE_KW,
    CASE_KW,
    CASEX_KW,
    CASEZ_KW,
    CELL_KW,
    CHANDLE_KW,
    CHECKER_KW,
    CLASS_KW,
    CLOCKING_KW,
    CMOS_KW,
    CONFIG_KW,
    CONST_KW,
    CONSTRAINT_KW,
    CONTEXT_KW,
    CONTINUE_KW,
    COVER_KW,
    COVERGROUP_KW,
    COVERPOINT_KW,
    CROSS_KW,
    DEASSIGN_KW,
    DEFAULT_KW,
    DEFPARAM_KW,
    DESIGN_KW,
    DISABLE_KW,
    DIST_KW,
    DO_KW,
    EDGE_KW,
    ELSE_KW,
    END_KW,
    ENDCASE_KW,
    ENDCHECKER_KW,
    ENDCLASS_KW,
    ENDCLOCKING_KW,
    ENDCONFIG_KW,
    ENDFUNCTION_KW,
    ENDGENERATE_KW,
    ENDGROUP_KW,
    ENDINTERFACE_KW,
    ENDMODULE_KW,
    ENDPACKAGE_KW,
    ENDPRIMITIVE_KW,
    ENDPROGRAM_KW,
    ENDPROPERTY_KW,
    ENDSEQUENCE_KW,
    ENDSPECIFY_KW,
    ENDTABLE_KW,
    ENDTASK_KW,
    ENUM_KW,
    EVENT_KW,
    EVENTUALLY_KW,
    EXPECT_KW,
    EXPORT_KW,
    EXTENDS_KW,
    EXTERN_KW,
    FINAL_KW,
    FIRST_MATCH_KW,
    FOR_KW,
    FORCE_KW,
    FOREACH_KW,
    FOREVER_KW,
    FORK_KW,
    FORKJOIN_KW,
    FUNCTION_KW,
    GENERATE_KW,
    GENVAR_KW,
    GLOBAL_KW,
    HIGHZ0_KW,
    HIGHZ1_KW,
    IF_KW,
    IFF_KW,
    IFNONE_KW,
    IGNORE_BINS_KW,
    ILLEGAL_BINS_KW,
    IMPLEMENTS_KW,
    IMPLIES_KW,
    IMPORT_KW,
    INCDIR_KW,
    INCLUDE_KW,
    INITIAL_KW,
    INOUT_KW,
    INPUT_KW,
    INSIDE_KW,
    INSTANCE_KW,
    INT_KW,
    INTEGER_KW,
    INTERCONNECT_KW,
    INTERFACE_KW,
    INTERSECT_KW,
    JOIN_KW,
    JOIN_ANY_KW,
    JOIN_NONE_KW,
    LARGE_KW,
    LET_KW,
    LIBLIST_KW,
    LIBRARY_KW,
    LOCAL_KW,
    LOCALPARAM_KW,
    LOGIC_KW,
    LONGINT_KW,
    MACROMODULE_KW,
    MATCHES_KW,
    MEDIUM_KW,
    MODPORT_KW,
    MODULE_KW,
    NAND_KW,
    NEGEDGE_KW,
    NETTYPE_KW,
    NEW_KW,
    NEXTTIME_KW,
    NMOS_KW,
    NOR_KW,
    NOSHOWCANCELLED_KW,
    NOT_KW,
    NOTIF0_KW,
    NOTIF1_KW,
    NULL_KW,
    OR_KW,
    OUTPUT_KW,
    PACKAGE_KW,
    PACKED_KW,
    PARAMETER_KW,
    PMOS_KW,
    POSEDGE_KW,
    PRIMITIVE_KW,
    PRIORITY_KW,
    PROGRAM_KW,
    PROPERTY_KW,
    PROTECTED_KW,
    PULL0_KW,
    PULL1_KW,
    PULLDOWN_KW,
    PULLUP_KW,
    PULSESTYLE_ONDETECT_KW,
    PULSESTYLE_ONEVENT_KW,
    PURE_KW,
    RAND_KW,
    RANDC_KW,
    RANDCASE_KW,
    RANDSEQUENCE_KW,
    RCMOS_KW,
    REAL_KW,
    REALTIME_KW,
    REF_KW,
    REG_KW,
    REJECT_ON_KW,
    RELEASE_KW,
    REPEAT_KW,
    RESTRICT_KW,
    RETURN_KW,
    RNMOS_KW,
    RPMOS_KW,
    RTRAN_KW,
    RTRANIF0_KW,
    RTRANIF1_KW,
    S_ALWAYS_KW,
    S_EVENTUALLY_KW,
    S_NEXTTIME_KW,
    S_UNTIL_KW,
    S_UNTIL_WITH_KW,
    SCALARED_KW,
    SEQUENCE_KW,
    SHORTINT_KW,
    SHORTREAL_KW,
    SHOWCANCELLED_KW,
    SIGNED_KW,
    SMALL_KW,
    SOFT_KW,
    SOLVE_KW,
    SPECIFY_KW,
    SPECPARAM_KW,
    STATIC_KW,
    STRING_KW,
    STRONG_KW,
    STRONG0_KW,
    STRONG1_KW,
    STRUCT_KW,
    SUPER_KW,
    SUPPLY0_KW,
    SUPPLY1_KW,
    SYNC_ACCEPT_ON_KW,
    SYNC_REJECT_ON_KW,
    TABLE_KW,
    TAGGED_KW,
    TASK_KW,
    THIS_KW,
    THROUGHOUT_KW,
    TIME_KW,
    TIMEPRECISION_KW,
    TIMEUNIT_KW,
    TRAN_KW,
    TRANIF0_KW,
    TRANIF1_KW,
    TRI_KW,
    TRI0_KW,
    TRI1_KW,
    TRIAND_KW,
    TRIOR_KW,
    TRIREG_KW,
    TYPE_KW,
    TYPEDEF_KW,
    UNION_KW,
    UNIQUE_KW,
    UNIQUE0_KW,
    UNSIGNED_KW,
    UNTIL_KW,
    UNTIL_WITH_KW,
    UNTYPED_KW,
    USE_KW,
    UWIRE_KW,
    VAR_KW,
    VECTORED_KW,
    VIRTUAL_KW,
    VOID_KW,
    WAIT_KW,
    WAIT_ORDER_KW,
    WAND_KW,
    WEAK_KW,
    WEAK0_KW,
    WEAK1_KW,
    WHILE_KW,
    WILDCARD_KW,
    WIRE_KW,
    WITH_KW,
    WITHIN_KW,
    WOR_KW,
    XNOR_KW,
    XOR_KW,

    //--------------------------------------------------------------------
    // Sentinels
    //--------------------------------------------------------------------
    /// A byte the lexer could not begin any token with.
    ///
    /// Never produced by `logos` itself -- the lexer maps its `Err` to this so
    /// that the token stream still covers every byte of the input, which the
    /// round-trip invariant requires.
    LEX_ERROR,
    /// End of file. Carries no text.
    EOF,
}

impl SyntaxKind {
    /// Bounds of the contiguous keyword block, for [`SyntaxKind::is_keyword`].
    /// Asserted against [`crate::keyword`] in the tests.
    const FIRST_KEYWORD: SyntaxKind = SyntaxKind::ACCEPT_ON_KW;
    const LAST_KEYWORD: SyntaxKind = SyntaxKind::XOR_KW;

    /// Whitespace and comments: present in the tree, but never part of the
    /// grammar.
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::WHITESPACE | SyntaxKind::LINE_COMMENT | SyntaxKind::BLOCK_COMMENT
        )
    }

    /// Whether this kind is a reserved word.
    pub fn is_keyword(self) -> bool {
        // The keyword block is contiguous, which is worth one assertion in the
        // tests rather than a 248-arm match here.
        (SyntaxKind::FIRST_KEYWORD as u16..=SyntaxKind::LAST_KEYWORD as u16)
            .contains(&(self as u16))
            || self == SyntaxKind::ONE_STEP_KW
    }
}
