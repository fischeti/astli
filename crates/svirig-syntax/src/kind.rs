//! Enumeration of all token and syntax node kinds.
//!
//! SystemVerilog syntax trees use a single unified `#[repr(u16)]` enum ([`SyntaxKind`])
//! for both leaf tokens and interior nodes:
//! - **Tokens**: Variants from the start of the enum through [`SyntaxKind::EOF`]. Tokens
//!   up to the keyword block are matched directly via `logos` regular expressions.
//! - **Keywords**: Contiguous block of reserved words matched by identifier lookup in [`crate::keyword`].
//! - **Nodes**: Interior syntax tree nodes following [`SyntaxKind::EOF`], bounded by [`SyntaxKind::LAST`].
//! - **Operators**: Named by written glyphs (e.g. `LT_EQ`) because SystemVerilog punctuation
//!   is overloaded (such as `<=` for relational comparison and non-blocking assignment).

use logos::Logos;

/// Kinds of tokens and nodes in a SystemVerilog syntax tree.
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum SyntaxKind {
    //--------------------------------------------------------------------
    // Trivia
    //--------------------------------------------------------------------
    // Trivia tokens (whitespace, comments). Preserved in the syntax tree for formatting and fidelity.
    #[regex(r"[ \t\r\n]+")]
    WHITESPACE,
    #[regex(r"//[^\r\n]*", allow_greedy = true)]
    LINE_COMMENT,
    #[regex(r"/\*([^*]|\*+[^*/])*\*+/")]
    BLOCK_COMMENT,

    //--------------------------------------------------------------------
    // Identifiers
    //--------------------------------------------------------------------
    /// Simple identifier (IEEE 1800-2023 §5.6), or keyword candidate prior to lookup.
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_$]*")]
    IDENT,
    /// Escaped identifier (§5.6.1): leading backslash followed by non-whitespace and terminated by whitespace.
    #[regex(r"\\[^ \t\r\n]+[ \t\r\n]")]
    ESCAPED_IDENT,
    /// System task or function identifier (§5.6.3), e.g. `$display` or `$clog2`.
    #[regex(r"\$[a-zA-Z0-9_$]+")]
    SYSTEM_IDENT,

    //--------------------------------------------------------------------
    // Literals
    //--------------------------------------------------------------------
    /// Decimal digit sequence (§5.7.1), representing sized literal widths or unsigned integers.
    #[regex(r"[0-9][0-9_]*")]
    INT_LITERAL,
    /// Base specifier with digits (§5.7.1), e.g. `'h1F` or `'sb10x`.
    #[regex(r"'[sS]?[bBoOdDhH][0-9a-fA-FxXzZ?][0-9a-fA-FxXzZ?_]*")]
    BASED_LITERAL,
    /// Unattached base specifier (§5.7.1) followed by whitespace, e.g. `'h` in `'h FF`.
    #[regex(r"'[sS]?[bBoOdDhH]")]
    INT_BASE,
    /// Unbased unsized literal (§5.7.1): `'0`, `'1`, `'x`, `'z`.
    #[regex(r"'[01xXzZ]")]
    UNBASED_UNSIZED_LITERAL,
    /// Real number literal (§5.7.2), e.g. `1.0`, `1e9`, or `1.8e-3`.
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*([eE][+-]?[0-9][0-9_]*)?")]
    #[regex(r"[0-9][0-9_]*[eE][+-]?[0-9][0-9_]*")]
    REAL_LITERAL,
    /// Time value literal (§5.8), e.g. `10ns` or `1.5us`.
    #[regex(r"[0-9][0-9_]*(\.[0-9][0-9_]*)?(s|ms|us|ns|ps|fs)")]
    TIME_LITERAL,
    /// The `1step` delay keyword (IEEE 1800-2023 §14.3).
    #[token("1step")]
    ONE_STEP_KW,
    /// Quoted string literal (§5.9), including escaped characters and line continuations.
    #[regex(r#""([^"\\\r\n]|\\(.|\r?\n))*""#)]
    STRING_LITERAL,

    //--------------------------------------------------------------------
    // Preprocessor
    //--------------------------------------------------------------------
    /// Backtick identifier: compiler directive or macro invocation, e.g. `` `define `` or `` `uvm_info ``.
    #[regex(r"`[a-zA-Z_][a-zA-Z0-9_$]*")]
    TICK_IDENT,
    /// Macro stringification delimiter (`` `\" ``) (§22.5.1).
    #[token("`\"")]
    MACRO_QUOTE,
    /// Escaped quote inside stringified macro text (`` `\\`\" ``).
    #[token("`\\`\"")]
    MACRO_ESCAPED_QUOTE,
    /// Token-pasting operator (``` `` ```).
    #[token("``")]
    MACRO_PASTE,
    /// Backslash-newline line continuation within a macro definition body.
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
    // Keywords (IEEE 1800-2023 Annex B)
    //--------------------------------------------------------------------
    // Matched via identifier lookup in `crate::keyword` rather than direct regex rules.
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
    /// Unrecognized source byte or invalid token sequence.
    LEX_ERROR,
    /// End-of-file marker.
    EOF,

    //--------------------------------------------------------------------
    // Syntax tree interior nodes
    //--------------------------------------------------------------------
    /// Root node of a parsed SystemVerilog source file.
    SOURCE_FILE,
    /// Balanced sequence of unparsed or fallback tokens preserved verbatim.
    VERBATIM,

    // Preprocessor syntax nodes
    /// Compiler directive and its arguments.
    DIRECTIVE,
    /// Macro definition substitution body.
    MACRO_BODY,
    /// Macro call site (directive identifier and optional argument list).
    MACRO_CALL,
    /// Parenthesized argument list of a macro invocation.
    MACRO_ARG_LIST,
    /// Individual argument within a macro argument list.
    MACRO_ARG,
    /// Conditional compilation region (`` `ifdef `` ... `` `endif ``).
    CONDITIONAL_REGION,
    /// Single branch within a conditional compilation region.
    CONDITIONAL_BRANCH,

    // Expressions
    /// Literal constant expression (number, string, etc.).
    LITERAL_EXPR,
    /// Identifier or symbol reference expression.
    NAME_REF,
    /// Parenthesized expression, including min:typ:max expressions.
    PAREN_EXPR,
    /// Prefix unary operator expression (e.g. `-a`, `!b`, `++c`).
    UNARY_EXPR,
    /// Postfix operator expression (e.g. `a++`).
    POSTFIX_EXPR,
    /// Binary operator expression (e.g. `a + b`).
    BIN_EXPR,
    /// Ternary conditional operator expression (`c ? a : b`).
    TERNARY_EXPR,
    /// Member or hierarchical field access (`a.b`).
    FIELD_EXPR,
    /// Scope resolution expression (`A::b`).
    SCOPE_EXPR,
    /// Array or bit index/slice expression (`a[i]`, `a[msb:lsb]`).
    INDEX_EXPR,
    /// Function, task, or system call invocation.
    CALL_EXPR,
    /// Parenthesized argument list for a call expression.
    ARG_LIST,
    /// Positional or named argument (`.port(signal)`).
    ARG,
    /// Explicit type cast expression (e.g. `int'(x)`).
    CAST_EXPR,
    /// Concatenation expression (`{a, b}`).
    CONCAT_EXPR,
    /// Replication expression (`{n{a}}`).
    REPLICATION_EXPR,
    /// Streaming concatenation expression (`{<<{a}}`, `{>>8{b}}`).
    STREAM_EXPR,
    /// Assignment pattern expression (`'{...}`).
    ASSIGNMENT_PATTERN,
    /// Member item within an assignment pattern (`default: 0`, `a: 1`).
    PATTERN_ITEM,
    /// Membership set expression (`a inside {b, [c:d]}`).
    INSIDE_EXPR,
    /// Range or value list used in `inside` or `dist` expressions.
    RANGE_LIST,
    /// Distribution constraint expression (`a dist {...}`).
    DIST_EXPR,
    /// Weighted item within a distribution expression.
    DIST_ITEM,
    /// `with` clause qualifying array methods or constraints.
    WITH_CLAUSE,
    /// Attribute instance specifications (`(* ... *)`).
    ATTRIBUTES,
    /// Individual attribute specification within `(* ... *)`.
    ATTRIBUTE_SPEC,

    // Types and declarations
    /// Type reference (builtin type or `typedef` name with dimensions).
    TYPE_REF,
    /// Enumeration type definition (`enum { ... }`).
    ENUM_TYPE,
    /// Enumeration variant with optional assigned value.
    ENUM_VARIANT,
    /// Structure type definition (`struct { ... }`).
    STRUCT_TYPE,
    /// Union type definition (`union { ... }`).
    UNION_TYPE,
    /// Member declaration within a struct or union.
    STRUCT_MEMBER,
    /// Type query expression (`type(expr)`).
    TYPE_REFERENCE,
    /// Packed or unpacked array dimension range (`[msb:lsb]`).
    DIMENSION,
    /// Variable or net declaration.
    VAR_DECL,
    /// Individual declarator name with dimensions and optional initializer.
    DECLARATOR,
    /// Type definition (`typedef`).
    TYPEDEF,
    /// Parameter or localparam declaration.
    PARAM_DECL,

    // Items and descriptions
    /// Module declaration (`module ... endmodule`).
    MODULE_DECL,
    /// Interface declaration (`interface ... endinterface`).
    INTERFACE_DECL,
    /// Program declaration (`program ... endprogram`).
    PROGRAM_DECL,
    /// Package declaration (`package ... endpackage`).
    PACKAGE_DECL,
    /// Class declaration (`class ... endclass`).
    CLASS_DECL,
    /// Parameter port list in a module or interface header (`#( ... )`).
    PARAM_PORT_LIST,
    /// Port list header (`( ... )`).
    PORT_LIST,
    /// Individual port definition in a port list.
    PORT,
    /// Port declaration within a module body (non-ANSI style).
    PORT_DECL,
    /// Modport declaration in an interface (`modport ...`).
    MODPORT_DECL,
    /// Individual modport definition.
    MODPORT,
    /// Package import or export declaration (`import pkg::*;`).
    IMPORT_DECL,
    /// Continuous assignment statement (`assign a = b;`).
    CONTINUOUS_ASSIGN,
    /// Assignment expression or statement (`lhs = rhs` or `lhs <= rhs`).
    ASSIGNMENT,
    /// Module or interface instantiation statement.
    INSTANTIATION,
    /// Individual instance within an instantiation statement.
    INSTANCE,
    /// Procedural block (`always`, `initial`, `final`).
    PROCEDURAL_BLOCK,
    /// Generate block construct (`generate ... endgenerate`).
    GENERATE_REGION,
    /// Function declaration or prototype.
    FUNCTION_DECL,
    /// Task declaration or prototype.
    TASK_DECL,
    /// Constraint block declaration (`constraint name { ... }`).
    CONSTRAINT_DECL,

    // Statements
    /// Sequential or parallel block statement (`begin ... end`, `fork ... join`).
    BLOCK,
    /// Expression statement or empty null statement.
    EXPR_STMT,
    /// Labeled statement (`label: statement`).
    LABELED_STMT,
    /// Conditional statement (`if ... else`).
    IF_STMT,
    /// Case statement (`case`, `casex`, `casez`).
    CASE_STMT,
    /// Branch item within a case statement.
    CASE_ITEM,
    /// `for` loop statement.
    FOR_STMT,
    /// `foreach` array loop statement.
    FOREACH_STMT,
    /// Header of a `foreach` loop: the array and the loop's variables.
    FOREACH_HEADER,
    /// `while` loop statement.
    WHILE_STMT,
    /// `do ... while` loop statement.
    DO_WHILE_STMT,
    /// `repeat` loop statement.
    REPEAT_STMT,
    /// `forever` loop statement.
    FOREVER_STMT,
    /// `return` statement.
    RETURN_STMT,
    /// `break` statement.
    BREAK_STMT,
    /// `continue` statement.
    CONTINUE_STMT,
    /// `disable` statement.
    DISABLE_STMT,
    /// `wait` statement.
    WAIT_STMT,
    /// Event trigger statement (`-> event` or `->> event`).
    EVENT_TRIGGER,
    /// Statement guarded by a timing delay or event control.
    TIMING_STMT,
    /// Event control specification (`@(posedge clk)`, `@*`).
    EVENT_CONTROL,
    /// Delay control specification (`#10`).
    DELAY_CONTROL,

    /// Sentinel marking the upper bound of valid syntax kinds.
    LAST,
}

use SyntaxKind::*;

impl SyntaxKind {
    /// Bounds of the contiguous keyword block, for [`SyntaxKind::is_keyword`].
    const FIRST_KEYWORD: SyntaxKind = ACCEPT_ON_KW;
    const LAST_KEYWORD: SyntaxKind = XOR_KW;

    /// Returns `true` if this kind is trivia (whitespace or comment).
    pub fn is_trivia(self) -> bool {
        matches!(self, WHITESPACE | LINE_COMMENT | BLOCK_COMMENT)
    }

    /// The first interior node kind.
    const FIRST_NODE: SyntaxKind = SOURCE_FILE;

    /// Returns `true` if this kind represents an interior syntax tree node.
    pub fn is_node(self) -> bool {
        (SyntaxKind::FIRST_NODE as u16..LAST as u16).contains(&(self as u16))
    }

    /// Returns `true` if this kind represents a leaf token.
    pub fn is_token(self) -> bool {
        (self as u16) < SyntaxKind::FIRST_NODE as u16
    }

    /// Reconstructs a [`SyntaxKind`] from its raw discriminant value.
    ///
    /// # Panics
    ///
    /// Panics if `raw` does not correspond to any defined variant.
    pub fn from_raw(raw: u16) -> SyntaxKind {
        assert!(raw < LAST as u16, "{raw} names no SyntaxKind");
        // SAFETY: the enum is `#[repr(u16)]` with contiguous zero-based discriminants
        // from 0 up to `LAST`, and `raw < LAST as u16` asserts validity.
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw) }
    }

    /// Returns `true` if this kind represents a reserved keyword.
    pub fn is_keyword(self) -> bool {
        (SyntaxKind::FIRST_KEYWORD as u16..=SyntaxKind::LAST_KEYWORD as u16)
            .contains(&(self as u16))
            || self == ONE_STEP_KW
    }
}
