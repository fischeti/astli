//! Generated from `svirig.ungram` by `tests/codegen.rs`; do not edit.
//! `UPDATE_EXPECT=1 cargo nextest run -p svirig-syntax codegen` rewrites it.

use super::{AstChildren, AstNode, support};
use crate::SyntaxKind::{self, *};
use crate::{SyntaxNode, SyntaxToken};

/// A `ARG` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Arg {
    syntax: SyntaxNode,
}

impl AstNode for Arg {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == ARG
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Arg { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl Arg {
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[DOT])
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT, STAR])
    }
    pub fn type_or_expr(&self) -> Option<TypeOrExpr> {
        support::child(&self.syntax)
    }
}

/// A `ARG_LIST` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArgList {
    syntax: SyntaxNode,
}

impl AstNode for ArgList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == ARG_LIST
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ArgList { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ArgList {
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_PAREN])
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
    pub fn args(&self) -> AstChildren<Arg> {
        support::children(&self.syntax)
    }
}

/// A `ASSIGNMENT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Assignment {
    syntax: SyntaxNode,
}

impl AstNode for Assignment {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == ASSIGNMENT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Assignment { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl Assignment {
    pub fn op(&self) -> Option<SyntaxToken> {
        support::token(
            &self.syntax,
            &[
                EQ,
                LT_EQ,
                PLUS_EQ,
                MINUS_EQ,
                STAR_EQ,
                SLASH_EQ,
                PERCENT_EQ,
                AMP_EQ,
                PIPE_EQ,
                CARET_EQ,
                LT_LT_EQ,
                GT_GT_EQ,
                LT_LT_LT_EQ,
                GT_GT_GT_EQ,
            ],
        )
    }
    pub fn lhs(&self) -> Option<Expr> {
        support::nth(&self.syntax, 0, Expr::can_cast)
    }
    pub fn delay_control(&self) -> Option<DelayControl> {
        support::child(&self.syntax)
    }
    pub fn rhs(&self) -> Option<Expr> {
        support::nth(&self.syntax, 1, Expr::can_cast)
    }
}

/// A `ASSIGNMENT_PATTERN` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssignmentPattern {
    syntax: SyntaxNode,
}

impl AstNode for AssignmentPattern {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == ASSIGNMENT_PATTERN
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(AssignmentPattern { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl AssignmentPattern {
    pub fn apostrophe_l_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[APOSTROPHE_L_BRACE])
    }
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACE])
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn pattern_items(&self) -> AstChildren<PatternItem> {
        support::children(&self.syntax)
    }
}

/// A `ATTRIBUTE_SPEC` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AttributeSpec {
    syntax: SyntaxNode,
}

impl AstNode for AttributeSpec {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == ATTRIBUTE_SPEC
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(AttributeSpec { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl AttributeSpec {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT])
    }
    pub fn eq_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[EQ])
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

/// A `ATTRIBUTES` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Attributes {
    syntax: SyntaxNode,
}

impl AstNode for Attributes {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == ATTRIBUTES
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Attributes { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl Attributes {
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_PAREN])
    }
    pub fn star_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[STAR])
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
    pub fn attribute_specs(&self) -> AstChildren<AttributeSpec> {
        support::children(&self.syntax)
    }
}

/// A `BIN_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BinExpr {
    syntax: SyntaxNode,
}

impl AstNode for BinExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == BIN_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(BinExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl BinExpr {
    pub fn op(&self) -> Option<SyntaxToken> {
        support::token(
            &self.syntax,
            &[
                PLUS,
                MINUS,
                STAR,
                SLASH,
                PERCENT,
                STAR_STAR,
                EQ_EQ,
                BANG_EQ,
                EQ_EQ_EQ,
                BANG_EQ_EQ,
                EQ_EQ_QUESTION,
                BANG_EQ_QUESTION,
                AMP_AMP,
                PIPE_PIPE,
                MINUS_GT,
                LT_MINUS_GT,
                LT,
                LT_EQ,
                GT,
                GT_EQ,
                AMP,
                PIPE,
                CARET,
                CARET_TILDE,
                TILDE_CARET,
                GT_GT,
                LT_LT,
                GT_GT_GT,
                LT_LT_LT,
            ],
        )
    }
    pub fn lhs(&self) -> Option<Expr> {
        support::nth(&self.syntax, 0, Expr::can_cast)
    }
    pub fn attributes(&self) -> Option<Attributes> {
        support::child(&self.syntax)
    }
    pub fn rhs(&self) -> Option<Expr> {
        support::nth(&self.syntax, 1, Expr::can_cast)
    }
}

/// A `BLOCK` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Block {
    syntax: SyntaxNode,
}

impl AstNode for Block {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == BLOCK
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Block { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl Block {
    pub fn open(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[BEGIN_KW, FORK_KW])
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn close(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[END_KW, JOIN_KW, JOIN_ANY_KW, JOIN_NONE_KW])
    }
    pub fn attributes(&self) -> Option<Attributes> {
        support::child(&self.syntax)
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
}

/// A `BREAK_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BreakStmt {
    syntax: SyntaxNode,
}

impl AstNode for BreakStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == BREAK_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(BreakStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl BreakStmt {
    pub fn break_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[BREAK_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
}

/// A `CALL_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallExpr {
    syntax: SyntaxNode,
}

impl AstNode for CallExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CALL_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(CallExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl CallExpr {
    pub fn hash_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[HASH])
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn arg_list(&self) -> Option<ArgList> {
        support::child(&self.syntax)
    }
    pub fn with_clause(&self) -> Option<WithClause> {
        support::child(&self.syntax)
    }
}

/// A `CASE_ITEM` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CaseItem {
    syntax: SyntaxNode,
}

impl AstNode for CaseItem {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CASE_ITEM
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(CaseItem { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl CaseItem {
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn exprs(&self) -> AstChildren<Expr> {
        support::children(&self.syntax)
    }
    pub fn item(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
}

/// A `CASE_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CaseStmt {
    syntax: SyntaxNode,
}

impl AstNode for CaseStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CASE_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(CaseStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl CaseStmt {
    pub fn qualifier(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[UNIQUE_KW, UNIQUE0_KW, PRIORITY_KW])
    }
    pub fn keyword(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[CASE_KW, CASEX_KW, CASEZ_KW])
    }
    pub fn inside_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[INSIDE_KW])
    }
    pub fn matches_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[MATCHES_KW])
    }
    pub fn endcase_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDCASE_KW])
    }
    pub fn paren_expr(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn case_items(&self) -> AstChildren<CaseItem> {
        support::children(&self.syntax)
    }
    pub fn preprocs(&self) -> AstChildren<Preproc> {
        support::children(&self.syntax)
    }
    pub fn verbatims(&self) -> AstChildren<Verbatim> {
        support::children(&self.syntax)
    }
}

/// A `CAST_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CastExpr {
    syntax: SyntaxNode,
}

impl AstNode for CastExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CAST_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(CastExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl CastExpr {
    pub fn apostrophe_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[APOSTROPHE])
    }
    pub fn ty(&self) -> Option<Expr> {
        support::nth(&self.syntax, 0, |kind| {
            Expr::can_cast(kind) || ParenExpr::can_cast(kind)
        })
    }
    pub fn operand(&self) -> Option<ParenExpr> {
        support::nth(&self.syntax, 1, |kind| {
            Expr::can_cast(kind) || ParenExpr::can_cast(kind)
        })
    }
}

/// A `CLASS_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClassDecl {
    syntax: SyntaxNode,
}

impl AstNode for ClassDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CLASS_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ClassDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ClassDecl {
    pub fn class_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[CLASS_KW])
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn extends_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[EXTENDS_KW])
    }
    pub fn implements_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IMPLEMENTS_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn endclass_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDCLASS_KW])
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn param_port_list(&self) -> Option<ParamPortList> {
        support::child(&self.syntax)
    }
    pub fn type_refs(&self) -> AstChildren<TypeRef> {
        support::children(&self.syntax)
    }
    pub fn arg_list(&self) -> Option<ArgList> {
        support::child(&self.syntax)
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
}

/// A `CONCAT_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConcatExpr {
    syntax: SyntaxNode,
}

impl AstNode for ConcatExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CONCAT_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ConcatExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ConcatExpr {
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACE])
    }
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACE])
    }
    pub fn exprs(&self) -> AstChildren<Expr> {
        support::children(&self.syntax)
    }
}

/// A `CONDITIONAL_BRANCH` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConditionalBranch {
    syntax: SyntaxNode,
}

impl AstNode for ConditionalBranch {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CONDITIONAL_BRANCH
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ConditionalBranch { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ConditionalBranch {
    pub fn directive(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TICK_IDENT])
    }
    pub fn condition(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT])
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
}

/// A `CONDITIONAL_REGION` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConditionalRegion {
    syntax: SyntaxNode,
}

impl AstNode for ConditionalRegion {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CONDITIONAL_REGION
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ConditionalRegion { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ConditionalRegion {
    pub fn endif(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TICK_IDENT])
    }
    pub fn conditional_branches(&self) -> AstChildren<ConditionalBranch> {
        support::children(&self.syntax)
    }
}

/// A `CONSTRAINT_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConstraintDecl {
    syntax: SyntaxNode,
}

impl AstNode for ConstraintDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CONSTRAINT_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ConstraintDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ConstraintDecl {
    pub fn constraint_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[CONSTRAINT_KW])
    }
    pub fn verbatim(&self) -> Option<Verbatim> {
        support::child(&self.syntax)
    }
}

/// A `CONTINUE_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContinueStmt {
    syntax: SyntaxNode,
}

impl AstNode for ContinueStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CONTINUE_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ContinueStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ContinueStmt {
    pub fn continue_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[CONTINUE_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
}

/// A `CONTINUOUS_ASSIGN` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContinuousAssign {
    syntax: SyntaxNode,
}

impl AstNode for ContinuousAssign {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CONTINUOUS_ASSIGN
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ContinuousAssign { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ContinuousAssign {
    pub fn assign_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ASSIGN_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn delay_control(&self) -> Option<DelayControl> {
        support::child(&self.syntax)
    }
    pub fn assignments(&self) -> AstChildren<Assignment> {
        support::children(&self.syntax)
    }
}

/// A `DECLARATOR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Declarator {
    syntax: SyntaxNode,
}

impl AstNode for Declarator {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == DECLARATOR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Declarator { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl Declarator {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn eq_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[EQ])
    }
    pub fn dimensions(&self) -> AstChildren<Dimension> {
        support::children(&self.syntax)
    }
    pub fn init(&self) -> Option<TypeOrExpr> {
        support::child(&self.syntax)
    }
    pub fn preproc(&self) -> Option<Preproc> {
        support::child(&self.syntax)
    }
}

/// A `DELAY_CONTROL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DelayControl {
    syntax: SyntaxNode,
}

impl AstNode for DelayControl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == DELAY_CONTROL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(DelayControl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl DelayControl {
    pub fn op(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[HASH, HASH_HASH])
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

/// A `DIMENSION` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Dimension {
    syntax: SyntaxNode,
}

impl AstNode for Dimension {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == DIMENSION
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Dimension { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl Dimension {
    pub fn l_brack_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACK])
    }
    pub fn r_brack_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACK])
    }
}

/// A `DIRECTIVE` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Directive {
    syntax: SyntaxNode,
}

impl AstNode for Directive {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == DIRECTIVE
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Directive { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl Directive {
    pub fn directive(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TICK_IDENT])
    }
    pub fn macro_body(&self) -> Option<MacroBody> {
        support::child(&self.syntax)
    }
}

/// A `DISABLE_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DisableStmt {
    syntax: SyntaxNode,
}

impl AstNode for DisableStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == DISABLE_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(DisableStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl DisableStmt {
    pub fn disable_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[DISABLE_KW])
    }
    pub fn fork_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[FORK_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

/// A `DIST_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DistExpr {
    syntax: SyntaxNode,
}

impl AstNode for DistExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == DIST_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(DistExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl DistExpr {
    pub fn dist_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[DIST_KW])
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn range_list(&self) -> Option<RangeList> {
        support::child(&self.syntax)
    }
}

/// A `DIST_ITEM` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DistItem {
    syntax: SyntaxNode,
}

impl AstNode for DistItem {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == DIST_ITEM
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(DistItem { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl DistItem {
    pub fn op(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON_EQ, COLON_SLASH])
    }
}

/// A `DO_WHILE_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DoWhileStmt {
    syntax: SyntaxNode,
}

impl AstNode for DoWhileStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == DO_WHILE_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(DoWhileStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl DoWhileStmt {
    pub fn do_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[DO_KW])
    }
    pub fn while_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[WHILE_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn body(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
    pub fn condition(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
}

/// A `ENUM_TYPE` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumType {
    syntax: SyntaxNode,
}

impl AstNode for EnumType {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == ENUM_TYPE
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(EnumType { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl EnumType {
    pub fn enum_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENUM_KW])
    }
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACE])
    }
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACE])
    }
    pub fn type_ref(&self) -> Option<TypeRef> {
        support::child(&self.syntax)
    }
    pub fn enum_variants(&self) -> AstChildren<EnumVariant> {
        support::children(&self.syntax)
    }
}

/// A `ENUM_VARIANT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumVariant {
    syntax: SyntaxNode,
}

impl AstNode for EnumVariant {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == ENUM_VARIANT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(EnumVariant { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl EnumVariant {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT])
    }
    pub fn eq_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[EQ])
    }
    pub fn dimension(&self) -> Option<Dimension> {
        support::child(&self.syntax)
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn preproc(&self) -> Option<Preproc> {
        support::child(&self.syntax)
    }
}

/// A `EVENT_CONTROL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventControl {
    syntax: SyntaxNode,
}

impl AstNode for EventControl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == EVENT_CONTROL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(EventControl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl EventControl {
    pub fn at_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[AT])
    }
    pub fn star_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[STAR])
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

/// A `EVENT_TRIGGER` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventTrigger {
    syntax: SyntaxNode,
}

impl AstNode for EventTrigger {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == EVENT_TRIGGER
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(EventTrigger { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl EventTrigger {
    pub fn op(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[MINUS_GT, MINUS_GT_GT])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn delay_control(&self) -> Option<DelayControl> {
        support::child(&self.syntax)
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

/// A `EXPR_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprStmt {
    syntax: SyntaxNode,
}

impl AstNode for ExprStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == EXPR_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ExprStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ExprStmt {
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn assignment(&self) -> Option<Assignment> {
        support::child(&self.syntax)
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

/// A `FIELD_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldExpr {
    syntax: SyntaxNode,
}

impl AstNode for FieldExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == FIELD_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(FieldExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl FieldExpr {
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[DOT])
    }
    pub fn field(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT, NEW_KW])
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

/// A `FOR_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForStmt {
    syntax: SyntaxNode,
}

impl AstNode for ForStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == FOR_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ForStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ForStmt {
    pub fn for_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[FOR_KW])
    }
    pub fn header(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn body(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
}

/// A `FOREACH_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForeachStmt {
    syntax: SyntaxNode,
}

impl AstNode for ForeachStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == FOREACH_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ForeachStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ForeachStmt {
    pub fn foreach_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[FOREACH_KW])
    }
    pub fn header(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn body(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
}

/// A `FOREVER_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForeverStmt {
    syntax: SyntaxNode,
}

impl AstNode for ForeverStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == FOREVER_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ForeverStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ForeverStmt {
    pub fn forever_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[FOREVER_KW])
    }
    pub fn body(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
}

/// A `FUNCTION_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionDecl {
    syntax: SyntaxNode,
}

impl AstNode for FunctionDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == FUNCTION_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(FunctionDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl FunctionDecl {
    pub fn function_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[FUNCTION_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn endfunction_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDFUNCTION_KW])
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn attributes(&self) -> Option<Attributes> {
        support::child(&self.syntax)
    }
    pub fn type_ref(&self) -> Option<TypeRef> {
        support::child(&self.syntax)
    }
    pub fn port_list(&self) -> Option<PortList> {
        support::child(&self.syntax)
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
}

/// A `GENERATE_REGION` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenerateRegion {
    syntax: SyntaxNode,
}

impl AstNode for GenerateRegion {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == GENERATE_REGION
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(GenerateRegion { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl GenerateRegion {
    pub fn generate_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[GENERATE_KW])
    }
    pub fn endgenerate_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDGENERATE_KW])
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
}

/// A `IF_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IfStmt {
    syntax: SyntaxNode,
}

impl AstNode for IfStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == IF_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(IfStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl IfStmt {
    pub fn qualifier(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[UNIQUE_KW, UNIQUE0_KW, PRIORITY_KW])
    }
    pub fn if_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IF_KW])
    }
    pub fn else_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ELSE_KW])
    }
    pub fn condition(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn then_branch(&self) -> Option<Item> {
        support::nth(&self.syntax, 0, Item::can_cast)
    }
    pub fn else_branch(&self) -> Option<Item> {
        support::nth(&self.syntax, 1, Item::can_cast)
    }
}

/// A `IMPORT_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportDecl {
    syntax: SyntaxNode,
}

impl AstNode for ImportDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == IMPORT_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ImportDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ImportDecl {
    pub fn keyword(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IMPORT_KW, EXPORT_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
}

/// A `INDEX_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IndexExpr {
    syntax: SyntaxNode,
}

impl AstNode for IndexExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == INDEX_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(IndexExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl IndexExpr {
    pub fn l_brack_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACK])
    }
    pub fn op(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON, PLUS_COLON, MINUS_COLON])
    }
    pub fn r_brack_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACK])
    }
    pub fn base(&self) -> Option<Expr> {
        support::nth(&self.syntax, 0, Expr::can_cast)
    }
    pub fn index(&self) -> Option<Expr> {
        support::nth(&self.syntax, 1, Expr::can_cast)
    }
    pub fn end(&self) -> Option<Expr> {
        support::nth(&self.syntax, 2, Expr::can_cast)
    }
}

/// A `INSIDE_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InsideExpr {
    syntax: SyntaxNode,
}

impl AstNode for InsideExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == INSIDE_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(InsideExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl InsideExpr {
    pub fn inside_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[INSIDE_KW])
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn range_list(&self) -> Option<RangeList> {
        support::child(&self.syntax)
    }
}

/// A `INSTANCE` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Instance {
    syntax: SyntaxNode,
}

impl AstNode for Instance {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == INSTANCE
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Instance { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl Instance {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn dimensions(&self) -> AstChildren<Dimension> {
        support::children(&self.syntax)
    }
    pub fn arg_list(&self) -> Option<ArgList> {
        support::child(&self.syntax)
    }
}

/// A `INSTANTIATION` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Instantiation {
    syntax: SyntaxNode,
}

impl AstNode for Instantiation {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == INSTANTIATION
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Instantiation { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl Instantiation {
    pub fn hash_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[HASH])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn attributes(&self) -> Option<Attributes> {
        support::child(&self.syntax)
    }
    pub fn type_ref(&self) -> Option<TypeRef> {
        support::child(&self.syntax)
    }
    pub fn arg_list(&self) -> Option<ArgList> {
        support::child(&self.syntax)
    }
    pub fn instances(&self) -> AstChildren<Instance> {
        support::children(&self.syntax)
    }
}

/// A `INTERFACE_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InterfaceDecl {
    syntax: SyntaxNode,
}

impl AstNode for InterfaceDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == INTERFACE_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(InterfaceDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl InterfaceDecl {
    pub fn interface_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[INTERFACE_KW])
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn endinterface_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDINTERFACE_KW])
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn attributes(&self) -> Option<Attributes> {
        support::child(&self.syntax)
    }
    pub fn import_decls(&self) -> AstChildren<ImportDecl> {
        support::children(&self.syntax)
    }
    pub fn param_port_list(&self) -> Option<ParamPortList> {
        support::child(&self.syntax)
    }
    pub fn port_list(&self) -> Option<PortList> {
        support::child(&self.syntax)
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
}

/// A `LABELED_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LabeledStmt {
    syntax: SyntaxNode,
}

impl AstNode for LabeledStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == LABELED_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(LabeledStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl LabeledStmt {
    pub fn label(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT])
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn item(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
}

/// A `LITERAL_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LiteralExpr {
    syntax: SyntaxNode,
}

impl AstNode for LiteralExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == LITERAL_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(LiteralExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl LiteralExpr {
    pub fn macro_call(&self) -> Option<MacroCall> {
        support::child(&self.syntax)
    }
}

/// A `MACRO_ARG` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MacroArg {
    syntax: SyntaxNode,
}

impl AstNode for MacroArg {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == MACRO_ARG
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(MacroArg { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

/// A `MACRO_ARG_LIST` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MacroArgList {
    syntax: SyntaxNode,
}

impl AstNode for MacroArgList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == MACRO_ARG_LIST
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(MacroArgList { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl MacroArgList {
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_PAREN])
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
    pub fn macro_args(&self) -> AstChildren<MacroArg> {
        support::children(&self.syntax)
    }
}

/// A `MACRO_BODY` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MacroBody {
    syntax: SyntaxNode,
}

impl AstNode for MacroBody {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == MACRO_BODY
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(MacroBody { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

/// A `MACRO_CALL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MacroCall {
    syntax: SyntaxNode,
}

impl AstNode for MacroCall {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == MACRO_CALL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(MacroCall { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl MacroCall {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TICK_IDENT])
    }
    pub fn macro_arg_list(&self) -> Option<MacroArgList> {
        support::child(&self.syntax)
    }
}

/// A `MODPORT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Modport {
    syntax: SyntaxNode,
}

impl AstNode for Modport {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == MODPORT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Modport { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl Modport {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT])
    }
    pub fn port_list(&self) -> Option<PortList> {
        support::child(&self.syntax)
    }
}

/// A `MODPORT_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModportDecl {
    syntax: SyntaxNode,
}

impl AstNode for ModportDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == MODPORT_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ModportDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ModportDecl {
    pub fn modport_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[MODPORT_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn modports(&self) -> AstChildren<Modport> {
        support::children(&self.syntax)
    }
}

/// A `MODULE_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModuleDecl {
    syntax: SyntaxNode,
}

impl AstNode for ModuleDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == MODULE_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ModuleDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ModuleDecl {
    pub fn keyword(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[MODULE_KW, MACROMODULE_KW])
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn endmodule_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDMODULE_KW])
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn attributes(&self) -> Option<Attributes> {
        support::child(&self.syntax)
    }
    pub fn import_decls(&self) -> AstChildren<ImportDecl> {
        support::children(&self.syntax)
    }
    pub fn param_port_list(&self) -> Option<ParamPortList> {
        support::child(&self.syntax)
    }
    pub fn port_list(&self) -> Option<PortList> {
        support::child(&self.syntax)
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
}

/// A `NAME_REF` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NameRef {
    syntax: SyntaxNode,
}

impl AstNode for NameRef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == NAME_REF
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(NameRef { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl NameRef {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(
            &self.syntax,
            &[
                IDENT,
                ESCAPED_IDENT,
                SYSTEM_IDENT,
                THIS_KW,
                SUPER_KW,
                NEW_KW,
                NULL_KW,
                DOLLAR,
                DEFAULT_KW,
                VOID_KW,
                BIT_KW,
                LOGIC_KW,
                REG_KW,
                BYTE_KW,
                SHORTINT_KW,
                INT_KW,
                LONGINT_KW,
                INTEGER_KW,
                TIME_KW,
                REAL_KW,
                SHORTREAL_KW,
                REALTIME_KW,
                STRING_KW,
                SIGNED_KW,
                UNSIGNED_KW,
            ],
        )
    }
    pub fn preproc(&self) -> Option<Preproc> {
        support::child(&self.syntax)
    }
}

/// A `PACKAGE_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageDecl {
    syntax: SyntaxNode,
}

impl AstNode for PackageDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PACKAGE_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(PackageDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl PackageDecl {
    pub fn package_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[PACKAGE_KW])
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn endpackage_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDPACKAGE_KW])
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn attributes(&self) -> Option<Attributes> {
        support::child(&self.syntax)
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
}

/// A `PARAM_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParamDecl {
    syntax: SyntaxNode,
}

impl AstNode for ParamDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PARAM_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ParamDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ParamDecl {
    pub fn keyword(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[PARAMETER_KW, LOCALPARAM_KW])
    }
    pub fn type_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TYPE_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn data_type(&self) -> Option<DataType> {
        support::child(&self.syntax)
    }
    pub fn declarators(&self) -> AstChildren<Declarator> {
        support::children(&self.syntax)
    }
}

/// A `PARAM_PORT_LIST` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParamPortList {
    syntax: SyntaxNode,
}

impl AstNode for ParamPortList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PARAM_PORT_LIST
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ParamPortList { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ParamPortList {
    pub fn hash_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[HASH])
    }
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_PAREN])
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
    pub fn param_decls(&self) -> AstChildren<ParamDecl> {
        support::children(&self.syntax)
    }
    pub fn preprocs(&self) -> AstChildren<Preproc> {
        support::children(&self.syntax)
    }
    pub fn verbatims(&self) -> AstChildren<Verbatim> {
        support::children(&self.syntax)
    }
}

/// A `PAREN_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParenExpr {
    syntax: SyntaxNode,
}

impl AstNode for ParenExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PAREN_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ParenExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ParenExpr {
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_PAREN])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
    pub fn exprs(&self) -> AstChildren<Expr> {
        support::children(&self.syntax)
    }
    pub fn var_decl(&self) -> Option<VarDecl> {
        support::child(&self.syntax)
    }
    pub fn assignments(&self) -> AstChildren<Assignment> {
        support::children(&self.syntax)
    }
}

/// A `PATTERN_ITEM` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PatternItem {
    syntax: SyntaxNode,
}

impl AstNode for PatternItem {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PATTERN_ITEM
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(PatternItem { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl PatternItem {
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
}

/// A `PORT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Port {
    syntax: SyntaxNode,
}

impl AstNode for Port {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PORT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Port { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl Port {
    pub fn attributes(&self) -> Option<Attributes> {
        support::child(&self.syntax)
    }
    pub fn data_type(&self) -> Option<DataType> {
        support::child(&self.syntax)
    }
    pub fn declarator(&self) -> Option<Declarator> {
        support::child(&self.syntax)
    }
}

/// A `PORT_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PortDecl {
    syntax: SyntaxNode,
}

impl AstNode for PortDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PORT_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(PortDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl PortDecl {
    pub fn direction(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[INPUT_KW, OUTPUT_KW, INOUT_KW, REF_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn attributes(&self) -> Option<Attributes> {
        support::child(&self.syntax)
    }
    pub fn data_type(&self) -> Option<DataType> {
        support::child(&self.syntax)
    }
    pub fn declarators(&self) -> AstChildren<Declarator> {
        support::children(&self.syntax)
    }
}

/// A `PORT_LIST` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PortList {
    syntax: SyntaxNode,
}

impl AstNode for PortList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PORT_LIST
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(PortList { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl PortList {
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_PAREN])
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
    pub fn ports(&self) -> AstChildren<Port> {
        support::children(&self.syntax)
    }
    pub fn preprocs(&self) -> AstChildren<Preproc> {
        support::children(&self.syntax)
    }
    pub fn verbatims(&self) -> AstChildren<Verbatim> {
        support::children(&self.syntax)
    }
}

/// A `POSTFIX_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PostfixExpr {
    syntax: SyntaxNode,
}

impl AstNode for PostfixExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == POSTFIX_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(PostfixExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl PostfixExpr {
    pub fn op(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[PLUS_PLUS, MINUS_MINUS])
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

/// A `PROCEDURAL_BLOCK` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProceduralBlock {
    syntax: SyntaxNode,
}

impl AstNode for ProceduralBlock {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PROCEDURAL_BLOCK
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ProceduralBlock { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ProceduralBlock {
    pub fn keyword(&self) -> Option<SyntaxToken> {
        support::token(
            &self.syntax,
            &[
                ALWAYS_KW,
                ALWAYS_COMB_KW,
                ALWAYS_FF_KW,
                ALWAYS_LATCH_KW,
                INITIAL_KW,
                FINAL_KW,
            ],
        )
    }
    pub fn body(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
}

/// A `PROGRAM_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProgramDecl {
    syntax: SyntaxNode,
}

impl AstNode for ProgramDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PROGRAM_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ProgramDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ProgramDecl {
    pub fn program_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[PROGRAM_KW])
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn endprogram_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDPROGRAM_KW])
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn attributes(&self) -> Option<Attributes> {
        support::child(&self.syntax)
    }
    pub fn import_decls(&self) -> AstChildren<ImportDecl> {
        support::children(&self.syntax)
    }
    pub fn param_port_list(&self) -> Option<ParamPortList> {
        support::child(&self.syntax)
    }
    pub fn port_list(&self) -> Option<PortList> {
        support::child(&self.syntax)
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
}

/// A `RANGE_LIST` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RangeList {
    syntax: SyntaxNode,
}

impl AstNode for RangeList {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == RANGE_LIST
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(RangeList { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl RangeList {
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACE])
    }
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACE])
    }
    pub fn exprs(&self) -> AstChildren<Expr> {
        support::children(&self.syntax)
    }
    pub fn dist_items(&self) -> AstChildren<DistItem> {
        support::children(&self.syntax)
    }
}

/// A `REPEAT_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepeatStmt {
    syntax: SyntaxNode,
}

impl AstNode for RepeatStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == REPEAT_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(RepeatStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl RepeatStmt {
    pub fn repeat_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[REPEAT_KW])
    }
    pub fn count(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn body(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
}

/// A `REPLICATION_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReplicationExpr {
    syntax: SyntaxNode,
}

impl AstNode for ReplicationExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == REPLICATION_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ReplicationExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ReplicationExpr {
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACE])
    }
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACE])
    }
    pub fn count(&self) -> Option<Expr> {
        support::nth(&self.syntax, 0, |kind| {
            ConcatExpr::can_cast(kind) || Expr::can_cast(kind)
        })
    }
    pub fn concat(&self) -> Option<ConcatExpr> {
        support::nth(&self.syntax, 1, |kind| {
            ConcatExpr::can_cast(kind) || Expr::can_cast(kind)
        })
    }
}

/// A `RETURN_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReturnStmt {
    syntax: SyntaxNode,
}

impl AstNode for ReturnStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == RETURN_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ReturnStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ReturnStmt {
    pub fn return_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[RETURN_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

/// A `SCOPE_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScopeExpr {
    syntax: SyntaxNode,
}

impl AstNode for ScopeExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SCOPE_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ScopeExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl ScopeExpr {
    pub fn colon_colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON_COLON])
    }
    pub fn member(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT, NEW_KW])
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

/// A `SOURCE_FILE` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceFile {
    syntax: SyntaxNode,
}

impl AstNode for SourceFile {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SOURCE_FILE
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(SourceFile { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl SourceFile {
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
}

/// A `STREAM_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamExpr {
    syntax: SyntaxNode,
}

impl AstNode for StreamExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == STREAM_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(StreamExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl StreamExpr {
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACE])
    }
    pub fn op(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[LT_LT, GT_GT])
    }
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACE])
    }
}

/// A `STRUCT_MEMBER` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructMember {
    syntax: SyntaxNode,
}

impl AstNode for StructMember {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == STRUCT_MEMBER
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(StructMember { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl StructMember {
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn attributes(&self) -> Option<Attributes> {
        support::child(&self.syntax)
    }
    pub fn data_type(&self) -> Option<DataType> {
        support::child(&self.syntax)
    }
    pub fn declarators(&self) -> AstChildren<Declarator> {
        support::children(&self.syntax)
    }
}

/// A `STRUCT_TYPE` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructType {
    syntax: SyntaxNode,
}

impl AstNode for StructType {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == STRUCT_TYPE
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(StructType { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl StructType {
    pub fn struct_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[STRUCT_KW])
    }
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACE])
    }
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACE])
    }
    pub fn struct_members(&self) -> AstChildren<StructMember> {
        support::children(&self.syntax)
    }
    pub fn dimensions(&self) -> AstChildren<Dimension> {
        support::children(&self.syntax)
    }
}

/// A `TASK_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskDecl {
    syntax: SyntaxNode,
}

impl AstNode for TaskDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == TASK_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(TaskDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl TaskDecl {
    pub fn task_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TASK_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn endtask_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDTASK_KW])
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn attributes(&self) -> Option<Attributes> {
        support::child(&self.syntax)
    }
    pub fn port_list(&self) -> Option<PortList> {
        support::child(&self.syntax)
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
}

/// A `TERNARY_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TernaryExpr {
    syntax: SyntaxNode,
}

impl AstNode for TernaryExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == TERNARY_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(TernaryExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl TernaryExpr {
    pub fn question_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[QUESTION])
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn condition(&self) -> Option<Expr> {
        support::nth(&self.syntax, 0, Expr::can_cast)
    }
    pub fn then_value(&self) -> Option<Expr> {
        support::nth(&self.syntax, 1, Expr::can_cast)
    }
    pub fn else_value(&self) -> Option<Expr> {
        support::nth(&self.syntax, 2, Expr::can_cast)
    }
}

/// A `TIMING_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TimingStmt {
    syntax: SyntaxNode,
}

impl AstNode for TimingStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == TIMING_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(TimingStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl TimingStmt {
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn attributes(&self) -> Option<Attributes> {
        support::child(&self.syntax)
    }
    pub fn event_control(&self) -> Option<EventControl> {
        support::child(&self.syntax)
    }
    pub fn delay_control(&self) -> Option<DelayControl> {
        support::child(&self.syntax)
    }
    pub fn item(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
}

/// A `TYPE_REF` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeRef {
    syntax: SyntaxNode,
}

impl AstNode for TypeRef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == TYPE_REF
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(TypeRef { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl TypeRef {
    pub fn hash_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[HASH])
    }
    pub fn preproc(&self) -> Option<Preproc> {
        support::child(&self.syntax)
    }
    pub fn arg_list(&self) -> Option<ArgList> {
        support::child(&self.syntax)
    }
    pub fn dimensions(&self) -> AstChildren<Dimension> {
        support::children(&self.syntax)
    }
}

/// A `TYPE_REFERENCE` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeReference {
    syntax: SyntaxNode,
}

impl AstNode for TypeReference {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == TYPE_REFERENCE
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(TypeReference { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl TypeReference {
    pub fn type_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TYPE_KW])
    }
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_PAREN])
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
    pub fn type_or_expr(&self) -> Option<TypeOrExpr> {
        support::child(&self.syntax)
    }
}

/// A `TYPEDEF` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Typedef {
    syntax: SyntaxNode,
}

impl AstNode for Typedef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == TYPEDEF
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Typedef { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl Typedef {
    pub fn typedef_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TYPEDEF_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn data_type(&self) -> Option<DataType> {
        support::child(&self.syntax)
    }
    pub fn declarator(&self) -> Option<Declarator> {
        support::child(&self.syntax)
    }
}

/// A `UNARY_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnaryExpr {
    syntax: SyntaxNode,
}

impl AstNode for UnaryExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == UNARY_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(UnaryExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl UnaryExpr {
    pub fn op(&self) -> Option<SyntaxToken> {
        support::token(
            &self.syntax,
            &[
                PLUS,
                MINUS,
                BANG,
                TILDE,
                AMP,
                TILDE_AMP,
                PIPE,
                TILDE_PIPE,
                CARET,
                TILDE_CARET,
                CARET_TILDE,
                PLUS_PLUS,
                MINUS_MINUS,
            ],
        )
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

/// A `UNION_TYPE` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnionType {
    syntax: SyntaxNode,
}

impl AstNode for UnionType {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == UNION_TYPE
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(UnionType { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl UnionType {
    pub fn union_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[UNION_KW])
    }
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACE])
    }
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACE])
    }
    pub fn struct_members(&self) -> AstChildren<StructMember> {
        support::children(&self.syntax)
    }
    pub fn dimensions(&self) -> AstChildren<Dimension> {
        support::children(&self.syntax)
    }
}

/// A `VAR_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VarDecl {
    syntax: SyntaxNode,
}

impl AstNode for VarDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == VAR_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(VarDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl VarDecl {
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn attributes(&self) -> Option<Attributes> {
        support::child(&self.syntax)
    }
    pub fn data_type(&self) -> Option<DataType> {
        support::child(&self.syntax)
    }
    pub fn declarators(&self) -> AstChildren<Declarator> {
        support::children(&self.syntax)
    }
}

/// A `VERBATIM` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Verbatim {
    syntax: SyntaxNode,
}

impl AstNode for Verbatim {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == VERBATIM
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Verbatim { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl Verbatim {
    pub fn preprocs(&self) -> AstChildren<Preproc> {
        support::children(&self.syntax)
    }
}

/// A `WAIT_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WaitStmt {
    syntax: SyntaxNode,
}

impl AstNode for WaitStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == WAIT_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(WaitStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl WaitStmt {
    pub fn wait_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[WAIT_KW])
    }
    pub fn fork_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[FORK_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn paren_expr(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn item(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
}

/// A `WHILE_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WhileStmt {
    syntax: SyntaxNode,
}

impl AstNode for WhileStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == WHILE_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(WhileStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl WhileStmt {
    pub fn while_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[WHILE_KW])
    }
    pub fn condition(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn body(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
}

/// A `WITH_CLAUSE` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WithClause {
    syntax: SyntaxNode,
}

impl AstNode for WithClause {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == WITH_CLAUSE
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(WithClause { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl WithClause {
    pub fn with_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[WITH_KW])
    }
    pub fn paren_expr(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
}

/// Any of [`TypeRef`], [`EnumType`], [`StructType`], [`UnionType`], [`TypeReference`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DataType {
    TypeRef(TypeRef),
    EnumType(EnumType),
    StructType(StructType),
    UnionType(UnionType),
    TypeReference(TypeReference),
}

impl AstNode for DataType {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            TYPE_REF | ENUM_TYPE | STRUCT_TYPE | UNION_TYPE | TYPE_REFERENCE
        )
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        match syntax.kind() {
            TYPE_REF => Some(DataType::TypeRef(TypeRef { syntax })),
            ENUM_TYPE => Some(DataType::EnumType(EnumType { syntax })),
            STRUCT_TYPE => Some(DataType::StructType(StructType { syntax })),
            UNION_TYPE => Some(DataType::UnionType(UnionType { syntax })),
            TYPE_REFERENCE => Some(DataType::TypeReference(TypeReference { syntax })),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            DataType::TypeRef(it) => it.syntax(),
            DataType::EnumType(it) => it.syntax(),
            DataType::StructType(it) => it.syntax(),
            DataType::UnionType(it) => it.syntax(),
            DataType::TypeReference(it) => it.syntax(),
        }
    }
}

/// Any of [`LiteralExpr`], [`NameRef`], [`ParenExpr`], [`UnaryExpr`], [`PostfixExpr`], [`BinExpr`], [`TernaryExpr`], [`FieldExpr`], [`ScopeExpr`], [`IndexExpr`], [`CallExpr`], [`CastExpr`], [`ConcatExpr`], [`ReplicationExpr`], [`StreamExpr`], [`AssignmentPattern`], [`InsideExpr`], [`DistExpr`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
    LiteralExpr(LiteralExpr),
    NameRef(NameRef),
    ParenExpr(ParenExpr),
    UnaryExpr(UnaryExpr),
    PostfixExpr(PostfixExpr),
    BinExpr(BinExpr),
    TernaryExpr(TernaryExpr),
    FieldExpr(FieldExpr),
    ScopeExpr(ScopeExpr),
    IndexExpr(IndexExpr),
    CallExpr(CallExpr),
    CastExpr(CastExpr),
    ConcatExpr(ConcatExpr),
    ReplicationExpr(ReplicationExpr),
    StreamExpr(StreamExpr),
    AssignmentPattern(AssignmentPattern),
    InsideExpr(InsideExpr),
    DistExpr(DistExpr),
}

impl AstNode for Expr {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            LITERAL_EXPR
                | NAME_REF
                | PAREN_EXPR
                | UNARY_EXPR
                | POSTFIX_EXPR
                | BIN_EXPR
                | TERNARY_EXPR
                | FIELD_EXPR
                | SCOPE_EXPR
                | INDEX_EXPR
                | CALL_EXPR
                | CAST_EXPR
                | CONCAT_EXPR
                | REPLICATION_EXPR
                | STREAM_EXPR
                | ASSIGNMENT_PATTERN
                | INSIDE_EXPR
                | DIST_EXPR
        )
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        match syntax.kind() {
            LITERAL_EXPR => Some(Expr::LiteralExpr(LiteralExpr { syntax })),
            NAME_REF => Some(Expr::NameRef(NameRef { syntax })),
            PAREN_EXPR => Some(Expr::ParenExpr(ParenExpr { syntax })),
            UNARY_EXPR => Some(Expr::UnaryExpr(UnaryExpr { syntax })),
            POSTFIX_EXPR => Some(Expr::PostfixExpr(PostfixExpr { syntax })),
            BIN_EXPR => Some(Expr::BinExpr(BinExpr { syntax })),
            TERNARY_EXPR => Some(Expr::TernaryExpr(TernaryExpr { syntax })),
            FIELD_EXPR => Some(Expr::FieldExpr(FieldExpr { syntax })),
            SCOPE_EXPR => Some(Expr::ScopeExpr(ScopeExpr { syntax })),
            INDEX_EXPR => Some(Expr::IndexExpr(IndexExpr { syntax })),
            CALL_EXPR => Some(Expr::CallExpr(CallExpr { syntax })),
            CAST_EXPR => Some(Expr::CastExpr(CastExpr { syntax })),
            CONCAT_EXPR => Some(Expr::ConcatExpr(ConcatExpr { syntax })),
            REPLICATION_EXPR => Some(Expr::ReplicationExpr(ReplicationExpr { syntax })),
            STREAM_EXPR => Some(Expr::StreamExpr(StreamExpr { syntax })),
            ASSIGNMENT_PATTERN => Some(Expr::AssignmentPattern(AssignmentPattern { syntax })),
            INSIDE_EXPR => Some(Expr::InsideExpr(InsideExpr { syntax })),
            DIST_EXPR => Some(Expr::DistExpr(DistExpr { syntax })),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Expr::LiteralExpr(it) => it.syntax(),
            Expr::NameRef(it) => it.syntax(),
            Expr::ParenExpr(it) => it.syntax(),
            Expr::UnaryExpr(it) => it.syntax(),
            Expr::PostfixExpr(it) => it.syntax(),
            Expr::BinExpr(it) => it.syntax(),
            Expr::TernaryExpr(it) => it.syntax(),
            Expr::FieldExpr(it) => it.syntax(),
            Expr::ScopeExpr(it) => it.syntax(),
            Expr::IndexExpr(it) => it.syntax(),
            Expr::CallExpr(it) => it.syntax(),
            Expr::CastExpr(it) => it.syntax(),
            Expr::ConcatExpr(it) => it.syntax(),
            Expr::ReplicationExpr(it) => it.syntax(),
            Expr::StreamExpr(it) => it.syntax(),
            Expr::AssignmentPattern(it) => it.syntax(),
            Expr::InsideExpr(it) => it.syntax(),
            Expr::DistExpr(it) => it.syntax(),
        }
    }
}

/// Any of [`ModuleDecl`], [`InterfaceDecl`], [`ProgramDecl`], [`PackageDecl`], [`ClassDecl`], [`FunctionDecl`], [`TaskDecl`], [`ConstraintDecl`], [`VarDecl`], [`ParamDecl`], [`Typedef`], [`ImportDecl`], [`PortDecl`], [`ModportDecl`], [`ContinuousAssign`], [`Instantiation`], [`ProceduralBlock`], [`GenerateRegion`], [`Block`], [`ExprStmt`], [`LabeledStmt`], [`IfStmt`], [`CaseStmt`], [`ForStmt`], [`ForeachStmt`], [`WhileStmt`], [`DoWhileStmt`], [`RepeatStmt`], [`ForeverStmt`], [`ReturnStmt`], [`BreakStmt`], [`ContinueStmt`], [`DisableStmt`], [`WaitStmt`], [`EventTrigger`], [`TimingStmt`], [`Preproc`], [`Verbatim`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item {
    ModuleDecl(ModuleDecl),
    InterfaceDecl(InterfaceDecl),
    ProgramDecl(ProgramDecl),
    PackageDecl(PackageDecl),
    ClassDecl(ClassDecl),
    FunctionDecl(FunctionDecl),
    TaskDecl(TaskDecl),
    ConstraintDecl(ConstraintDecl),
    VarDecl(VarDecl),
    ParamDecl(ParamDecl),
    Typedef(Typedef),
    ImportDecl(ImportDecl),
    PortDecl(PortDecl),
    ModportDecl(ModportDecl),
    ContinuousAssign(ContinuousAssign),
    Instantiation(Instantiation),
    ProceduralBlock(ProceduralBlock),
    GenerateRegion(GenerateRegion),
    Block(Block),
    ExprStmt(ExprStmt),
    LabeledStmt(LabeledStmt),
    IfStmt(IfStmt),
    CaseStmt(CaseStmt),
    ForStmt(ForStmt),
    ForeachStmt(ForeachStmt),
    WhileStmt(WhileStmt),
    DoWhileStmt(DoWhileStmt),
    RepeatStmt(RepeatStmt),
    ForeverStmt(ForeverStmt),
    ReturnStmt(ReturnStmt),
    BreakStmt(BreakStmt),
    ContinueStmt(ContinueStmt),
    DisableStmt(DisableStmt),
    WaitStmt(WaitStmt),
    EventTrigger(EventTrigger),
    TimingStmt(TimingStmt),
    Preproc(Preproc),
    Verbatim(Verbatim),
}

impl AstNode for Item {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            MODULE_DECL
                | INTERFACE_DECL
                | PROGRAM_DECL
                | PACKAGE_DECL
                | CLASS_DECL
                | FUNCTION_DECL
                | TASK_DECL
                | CONSTRAINT_DECL
                | VAR_DECL
                | PARAM_DECL
                | TYPEDEF
                | IMPORT_DECL
                | PORT_DECL
                | MODPORT_DECL
                | CONTINUOUS_ASSIGN
                | INSTANTIATION
                | PROCEDURAL_BLOCK
                | GENERATE_REGION
                | BLOCK
                | EXPR_STMT
                | LABELED_STMT
                | IF_STMT
                | CASE_STMT
                | FOR_STMT
                | FOREACH_STMT
                | WHILE_STMT
                | DO_WHILE_STMT
                | REPEAT_STMT
                | FOREVER_STMT
                | RETURN_STMT
                | BREAK_STMT
                | CONTINUE_STMT
                | DISABLE_STMT
                | WAIT_STMT
                | EVENT_TRIGGER
                | TIMING_STMT
                | VERBATIM
        ) || Preproc::can_cast(kind)
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        match syntax.kind() {
            MODULE_DECL => Some(Item::ModuleDecl(ModuleDecl { syntax })),
            INTERFACE_DECL => Some(Item::InterfaceDecl(InterfaceDecl { syntax })),
            PROGRAM_DECL => Some(Item::ProgramDecl(ProgramDecl { syntax })),
            PACKAGE_DECL => Some(Item::PackageDecl(PackageDecl { syntax })),
            CLASS_DECL => Some(Item::ClassDecl(ClassDecl { syntax })),
            FUNCTION_DECL => Some(Item::FunctionDecl(FunctionDecl { syntax })),
            TASK_DECL => Some(Item::TaskDecl(TaskDecl { syntax })),
            CONSTRAINT_DECL => Some(Item::ConstraintDecl(ConstraintDecl { syntax })),
            VAR_DECL => Some(Item::VarDecl(VarDecl { syntax })),
            PARAM_DECL => Some(Item::ParamDecl(ParamDecl { syntax })),
            TYPEDEF => Some(Item::Typedef(Typedef { syntax })),
            IMPORT_DECL => Some(Item::ImportDecl(ImportDecl { syntax })),
            PORT_DECL => Some(Item::PortDecl(PortDecl { syntax })),
            MODPORT_DECL => Some(Item::ModportDecl(ModportDecl { syntax })),
            CONTINUOUS_ASSIGN => Some(Item::ContinuousAssign(ContinuousAssign { syntax })),
            INSTANTIATION => Some(Item::Instantiation(Instantiation { syntax })),
            PROCEDURAL_BLOCK => Some(Item::ProceduralBlock(ProceduralBlock { syntax })),
            GENERATE_REGION => Some(Item::GenerateRegion(GenerateRegion { syntax })),
            BLOCK => Some(Item::Block(Block { syntax })),
            EXPR_STMT => Some(Item::ExprStmt(ExprStmt { syntax })),
            LABELED_STMT => Some(Item::LabeledStmt(LabeledStmt { syntax })),
            IF_STMT => Some(Item::IfStmt(IfStmt { syntax })),
            CASE_STMT => Some(Item::CaseStmt(CaseStmt { syntax })),
            FOR_STMT => Some(Item::ForStmt(ForStmt { syntax })),
            FOREACH_STMT => Some(Item::ForeachStmt(ForeachStmt { syntax })),
            WHILE_STMT => Some(Item::WhileStmt(WhileStmt { syntax })),
            DO_WHILE_STMT => Some(Item::DoWhileStmt(DoWhileStmt { syntax })),
            REPEAT_STMT => Some(Item::RepeatStmt(RepeatStmt { syntax })),
            FOREVER_STMT => Some(Item::ForeverStmt(ForeverStmt { syntax })),
            RETURN_STMT => Some(Item::ReturnStmt(ReturnStmt { syntax })),
            BREAK_STMT => Some(Item::BreakStmt(BreakStmt { syntax })),
            CONTINUE_STMT => Some(Item::ContinueStmt(ContinueStmt { syntax })),
            DISABLE_STMT => Some(Item::DisableStmt(DisableStmt { syntax })),
            WAIT_STMT => Some(Item::WaitStmt(WaitStmt { syntax })),
            EVENT_TRIGGER => Some(Item::EventTrigger(EventTrigger { syntax })),
            TIMING_STMT => Some(Item::TimingStmt(TimingStmt { syntax })),
            VERBATIM => Some(Item::Verbatim(Verbatim { syntax })),
            kind if Preproc::can_cast(kind) => Preproc::cast(syntax).map(Item::Preproc),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Item::ModuleDecl(it) => it.syntax(),
            Item::InterfaceDecl(it) => it.syntax(),
            Item::ProgramDecl(it) => it.syntax(),
            Item::PackageDecl(it) => it.syntax(),
            Item::ClassDecl(it) => it.syntax(),
            Item::FunctionDecl(it) => it.syntax(),
            Item::TaskDecl(it) => it.syntax(),
            Item::ConstraintDecl(it) => it.syntax(),
            Item::VarDecl(it) => it.syntax(),
            Item::ParamDecl(it) => it.syntax(),
            Item::Typedef(it) => it.syntax(),
            Item::ImportDecl(it) => it.syntax(),
            Item::PortDecl(it) => it.syntax(),
            Item::ModportDecl(it) => it.syntax(),
            Item::ContinuousAssign(it) => it.syntax(),
            Item::Instantiation(it) => it.syntax(),
            Item::ProceduralBlock(it) => it.syntax(),
            Item::GenerateRegion(it) => it.syntax(),
            Item::Block(it) => it.syntax(),
            Item::ExprStmt(it) => it.syntax(),
            Item::LabeledStmt(it) => it.syntax(),
            Item::IfStmt(it) => it.syntax(),
            Item::CaseStmt(it) => it.syntax(),
            Item::ForStmt(it) => it.syntax(),
            Item::ForeachStmt(it) => it.syntax(),
            Item::WhileStmt(it) => it.syntax(),
            Item::DoWhileStmt(it) => it.syntax(),
            Item::RepeatStmt(it) => it.syntax(),
            Item::ForeverStmt(it) => it.syntax(),
            Item::ReturnStmt(it) => it.syntax(),
            Item::BreakStmt(it) => it.syntax(),
            Item::ContinueStmt(it) => it.syntax(),
            Item::DisableStmt(it) => it.syntax(),
            Item::WaitStmt(it) => it.syntax(),
            Item::EventTrigger(it) => it.syntax(),
            Item::TimingStmt(it) => it.syntax(),
            Item::Preproc(it) => it.syntax(),
            Item::Verbatim(it) => it.syntax(),
        }
    }
}

/// Any of [`MacroCall`], [`Directive`], [`ConditionalRegion`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Preproc {
    MacroCall(MacroCall),
    Directive(Directive),
    ConditionalRegion(ConditionalRegion),
}

impl AstNode for Preproc {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(kind, MACRO_CALL | DIRECTIVE | CONDITIONAL_REGION)
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        match syntax.kind() {
            MACRO_CALL => Some(Preproc::MacroCall(MacroCall { syntax })),
            DIRECTIVE => Some(Preproc::Directive(Directive { syntax })),
            CONDITIONAL_REGION => Some(Preproc::ConditionalRegion(ConditionalRegion { syntax })),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Preproc::MacroCall(it) => it.syntax(),
            Preproc::Directive(it) => it.syntax(),
            Preproc::ConditionalRegion(it) => it.syntax(),
        }
    }
}

/// Any of [`Expr`], [`DataType`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeOrExpr {
    Expr(Expr),
    DataType(DataType),
}

impl AstNode for TypeOrExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        Expr::can_cast(kind) || DataType::can_cast(kind)
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        match syntax.kind() {
            kind if Expr::can_cast(kind) => Expr::cast(syntax).map(TypeOrExpr::Expr),
            kind if DataType::can_cast(kind) => DataType::cast(syntax).map(TypeOrExpr::DataType),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            TypeOrExpr::Expr(it) => it.syntax(),
            TypeOrExpr::DataType(it) => it.syntax(),
        }
    }
}
