// Generated from `astli.ungram` by `tests/codegen.rs`; do not edit.
// `UPDATE_EXPECT=1 cargo nextest run -p astli-syntax codegen` rewrites it.

use super::{AstChildren, AstNode, support};
use crate::SyntaxKind::{self, *};
use crate::{SyntaxNode, SyntaxToken};
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
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn endmodule_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDMODULE_KW])
    }
    pub fn import_decls(&self) -> AstChildren<ImportDecl> {
        support::children(&self.syntax)
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
    pub fn keyword(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[MODULE_KW, MACROMODULE_KW])
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn param_port_list(&self) -> Option<ParamPortList> {
        support::child(&self.syntax)
    }
    pub fn port_list(&self) -> Option<PortList> {
        support::child(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
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
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn endinterface_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDINTERFACE_KW])
    }
    pub fn import_decls(&self) -> AstChildren<ImportDecl> {
        support::children(&self.syntax)
    }
    pub fn interface_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[INTERFACE_KW])
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn param_port_list(&self) -> Option<ParamPortList> {
        support::child(&self.syntax)
    }
    pub fn port_list(&self) -> Option<PortList> {
        support::child(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
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
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn endprogram_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDPROGRAM_KW])
    }
    pub fn import_decls(&self) -> AstChildren<ImportDecl> {
        support::children(&self.syntax)
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn param_port_list(&self) -> Option<ParamPortList> {
        support::child(&self.syntax)
    }
    pub fn port_list(&self) -> Option<PortList> {
        support::child(&self.syntax)
    }
    pub fn program_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[PROGRAM_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
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
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn endpackage_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDPACKAGE_KW])
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn package_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[PACKAGE_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
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
    pub fn arg_list(&self) -> Option<ArgList> {
        support::child(&self.syntax)
    }
    pub fn class_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[CLASS_KW])
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn endclass_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDCLASS_KW])
    }
    pub fn extends_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[EXTENDS_KW])
    }
    pub fn implements_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IMPLEMENTS_KW])
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn param_port_list(&self) -> Option<ParamPortList> {
        support::child(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn type_refs(&self) -> AstChildren<TypeRef> {
        support::children(&self.syntax)
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
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn endfunction_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDFUNCTION_KW])
    }
    pub fn function_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[FUNCTION_KW])
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
    pub fn port_list(&self) -> Option<PortList> {
        support::child(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn type_ref(&self) -> Option<TypeRef> {
        support::child(&self.syntax)
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
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn endtask_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDTASK_KW])
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
    pub fn port_list(&self) -> Option<PortList> {
        support::child(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn task_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TASK_KW])
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
    pub fn constraint_block(&self) -> Option<ConstraintBlock> {
        support::child(&self.syntax)
    }
    pub fn constraint_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[CONSTRAINT_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
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
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
    pub fn data_type(&self) -> Option<DataType> {
        support::child(&self.syntax)
    }
    pub fn declarators(&self) -> AstChildren<Declarator> {
        support::children(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
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
    pub fn data_type(&self) -> Option<DataType> {
        support::child(&self.syntax)
    }
    pub fn declarators(&self) -> AstChildren<Declarator> {
        support::children(&self.syntax)
    }
    pub fn keyword(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[PARAMETER_KW, LOCALPARAM_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn type_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TYPE_KW])
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
    pub fn data_type(&self) -> Option<DataType> {
        support::child(&self.syntax)
    }
    pub fn declarator(&self) -> Option<Declarator> {
        support::child(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn typedef_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TYPEDEF_KW])
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
/// A `TIMEUNIT_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TimeunitDecl {
    syntax: SyntaxNode,
}
impl AstNode for TimeunitDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == TIMEUNIT_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(TimeunitDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl TimeunitDecl {
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn keyword(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TIMEUNIT_KW, TIMEPRECISION_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
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
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
    pub fn data_type(&self) -> Option<DataType> {
        support::child(&self.syntax)
    }
    pub fn declarators(&self) -> AstChildren<Declarator> {
        support::children(&self.syntax)
    }
    pub fn direction(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[INPUT_KW, OUTPUT_KW, INOUT_KW, REF_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
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
    pub fn modports(&self) -> AstChildren<Modport> {
        support::children(&self.syntax)
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
    pub fn assignments(&self) -> AstChildren<Assignment> {
        support::children(&self.syntax)
    }
    pub fn delay_control(&self) -> Option<DelayControl> {
        support::child(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
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
    pub fn arg_list(&self) -> Option<ArgList> {
        support::child(&self.syntax)
    }
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
    pub fn hash_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[HASH])
    }
    pub fn instances(&self) -> AstChildren<Instance> {
        support::children(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn type_ref(&self) -> Option<TypeRef> {
        support::child(&self.syntax)
    }
}
/// A `BIND_DIRECTIVE` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BindDirective {
    syntax: SyntaxNode,
}
impl AstNode for BindDirective {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == BIND_DIRECTIVE
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(BindDirective { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl BindDirective {
    pub fn bind_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[BIND_KW])
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn exprs(&self) -> AstChildren<Expr> {
        support::children(&self.syntax)
    }
    pub fn instantiation(&self) -> Option<Instantiation> {
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
    pub fn body(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
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
    pub fn endgenerate_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDGENERATE_KW])
    }
    pub fn generate_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[GENERATE_KW])
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
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
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
    pub fn close(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[END_KW, JOIN_KW, JOIN_ANY_KW, JOIN_NONE_KW])
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
    pub fn open(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[BEGIN_KW, FORK_KW])
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
    pub fn assignment(&self) -> Option<Assignment> {
        support::child(&self.syntax)
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
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
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn item(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
    pub fn label(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT])
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
    pub fn condition(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn else_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ELSE_KW])
    }
    pub fn if_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IF_KW])
    }
    pub fn qualifier(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[UNIQUE_KW, UNIQUE0_KW, PRIORITY_KW])
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
    pub fn case_items(&self) -> AstChildren<CaseItem> {
        support::children(&self.syntax)
    }
    pub fn endcase_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDCASE_KW])
    }
    pub fn inside_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[INSIDE_KW])
    }
    pub fn keyword(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[CASE_KW, CASEX_KW, CASEZ_KW, RANDCASE_KW])
    }
    pub fn matches_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[MATCHES_KW])
    }
    pub fn paren_expr(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn preprocs(&self) -> AstChildren<Preproc> {
        support::children(&self.syntax)
    }
    pub fn qualifier(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[UNIQUE_KW, UNIQUE0_KW, PRIORITY_KW])
    }
    pub fn verbatims(&self) -> AstChildren<Verbatim> {
        support::children(&self.syntax)
    }
}
/// A `RANDSEQUENCE_STMT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RandsequenceStmt {
    syntax: SyntaxNode,
}
impl AstNode for RandsequenceStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == RANDSEQUENCE_STMT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(RandsequenceStmt { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl RandsequenceStmt {
    pub fn endsequence_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDSEQUENCE_KW])
    }
    pub fn paren_expr(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn productions(&self) -> AstChildren<Production> {
        support::children(&self.syntax)
    }
    pub fn randsequence_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[RANDSEQUENCE_KW])
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
    pub fn body(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
    pub fn for_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[FOR_KW])
    }
    pub fn header(&self) -> Option<ParenExpr> {
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
    pub fn body(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
    pub fn foreach_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[FOREACH_KW])
    }
    pub fn header(&self) -> Option<ForeachHeader> {
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
    pub fn body(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
    pub fn condition(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn while_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[WHILE_KW])
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
    pub fn body(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
    pub fn condition(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn do_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[DO_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn while_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[WHILE_KW])
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
    pub fn body(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
    pub fn count(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn repeat_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[REPEAT_KW])
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
    pub fn body(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
    pub fn forever_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[FOREVER_KW])
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
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn return_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[RETURN_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
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
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn fork_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[FORK_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
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
    pub fn fork_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[FORK_KW])
    }
    pub fn item(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
    pub fn paren_expr(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn wait_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[WAIT_KW])
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
    pub fn delay_control(&self) -> Option<DelayControl> {
        support::child(&self.syntax)
    }
    pub fn event_control(&self) -> Option<EventControl> {
        support::child(&self.syntax)
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn op(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[MINUS_GT, MINUS_GT_GT])
    }
    pub fn repeat_control(&self) -> Option<RepeatControl> {
        support::child(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
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
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
    pub fn delay_control(&self) -> Option<DelayControl> {
        support::child(&self.syntax)
    }
    pub fn event_control(&self) -> Option<EventControl> {
        support::child(&self.syntax)
    }
    pub fn item(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
}
/// A `IMMEDIATE_ASSERTION` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImmediateAssertion {
    syntax: SyntaxNode,
}
impl AstNode for ImmediateAssertion {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == IMMEDIATE_ASSERTION
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ImmediateAssertion { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl ImmediateAssertion {
    pub fn condition(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn else_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ELSE_KW])
    }
    pub fn final_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[FINAL_KW])
    }
    pub fn hash_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[HASH])
    }
    pub fn keyword(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ASSERT_KW, ASSUME_KW, COVER_KW])
    }
}
/// A `PROCEDURAL_ASSIGN` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProceduralAssign {
    syntax: SyntaxNode,
}
impl AstNode for ProceduralAssign {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PROCEDURAL_ASSIGN
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ProceduralAssign { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl ProceduralAssign {
    pub fn assignment(&self) -> Option<Assignment> {
        support::child(&self.syntax)
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn keyword(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[FORCE_KW, RELEASE_KW, ASSIGN_KW, DEASSIGN_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
}
/// A `CONSTRAINT_BLOCK` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConstraintBlock {
    syntax: SyntaxNode,
}
impl AstNode for ConstraintBlock {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CONSTRAINT_BLOCK
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ConstraintBlock { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl ConstraintBlock {
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACE])
    }
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACE])
    }
}
/// A `CONSTRAINT_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConstraintExpr {
    syntax: SyntaxNode,
}
impl AstNode for ConstraintExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CONSTRAINT_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ConstraintExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl ConstraintExpr {
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn qualifier(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SOFT_KW, UNIQUE_KW, DISABLE_KW])
    }
    pub fn range_list(&self) -> Option<RangeList> {
        support::child(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn soft_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SOFT_KW])
    }
}
/// A `IMPLICATION` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Implication {
    syntax: SyntaxNode,
}
impl AstNode for Implication {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == IMPLICATION
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Implication { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl Implication {
    pub fn condition(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn item(&self) -> Option<Item> {
        support::child(&self.syntax)
    }
    pub fn minus_gt_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[MINUS_GT])
    }
}
/// A `SOLVE_BEFORE` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SolveBefore {
    syntax: SyntaxNode,
}
impl AstNode for SolveBefore {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SOLVE_BEFORE
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(SolveBefore { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl SolveBefore {
    pub fn before_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[BEFORE_KW])
    }
    pub fn exprs(&self) -> AstChildren<Expr> {
        support::children(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn solve_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SOLVE_KW])
    }
}
/// A `COVERGROUP_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CovergroupDecl {
    syntax: SyntaxNode,
}
impl AstNode for CovergroupDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == COVERGROUP_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(CovergroupDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl CovergroupDecl {
    pub fn at_at_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[AT_AT])
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn covergroup_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COVERGROUP_KW])
    }
    pub fn endgroup_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDGROUP_KW])
    }
    pub fn event_control(&self) -> Option<EventControl> {
        support::child(&self.syntax)
    }
    pub fn extends_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[EXTENDS_KW])
    }
    pub fn function_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[FUNCTION_KW])
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn paren_expr(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn with_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[WITH_KW])
    }
}
/// A `COVERPOINT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Coverpoint {
    syntax: SyntaxNode,
}
impl AstNode for Coverpoint {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == COVERPOINT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Coverpoint { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl Coverpoint {
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
    pub fn bins_block(&self) -> Option<BinsBlock> {
        support::child(&self.syntax)
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn coverpoint_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COVERPOINT_KW])
    }
    pub fn data_type(&self) -> Option<DataType> {
        support::child(&self.syntax)
    }
    pub fn iff_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IFF_KW])
    }
    pub fn label(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
}
/// A `CROSS` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cross {
    syntax: SyntaxNode,
}
impl AstNode for Cross {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CROSS
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Cross { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl Cross {
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
    pub fn bins_block(&self) -> Option<BinsBlock> {
        support::child(&self.syntax)
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn cross_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[CROSS_KW])
    }
    pub fn exprs(&self) -> AstChildren<Expr> {
        support::children(&self.syntax)
    }
    pub fn iff_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IFF_KW])
    }
    pub fn label(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
}
/// A `BINS` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Bins {
    syntax: SyntaxNode,
}
impl AstNode for Bins {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == BINS
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Bins { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl Bins {
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
    pub fn default_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[DEFAULT_KW])
    }
    pub fn eq_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[EQ])
    }
    pub fn iff_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IFF_KW])
    }
    pub fn keyword(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[BINS_KW, ILLEGAL_BINS_KW, IGNORE_BINS_KW])
    }
    pub fn l_brack_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACK])
    }
    pub fn matches_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[MATCHES_KW])
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT])
    }
    pub fn r_brack_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACK])
    }
    pub fn range_list(&self) -> Option<RangeList> {
        support::child(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn sequence_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEQUENCE_KW])
    }
    pub fn trans_sets(&self) -> AstChildren<TransSet> {
        support::children(&self.syntax)
    }
    pub fn wildcard_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[WILDCARD_KW])
    }
    pub fn with_clause(&self) -> Option<WithClause> {
        support::child(&self.syntax)
    }
}
/// A `PROPERTY_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PropertyDecl {
    syntax: SyntaxNode,
}
impl AstNode for PropertyDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PROPERTY_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(PropertyDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl PropertyDecl {
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn endproperty_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDPROPERTY_KW])
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn port_list(&self) -> Option<PortList> {
        support::child(&self.syntax)
    }
    pub fn property_spec(&self) -> Option<PropertySpec> {
        support::child(&self.syntax)
    }
    pub fn property_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[PROPERTY_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn var_decls(&self) -> AstChildren<VarDecl> {
        support::children(&self.syntax)
    }
}
/// A `SEQUENCE_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SequenceDecl {
    syntax: SyntaxNode,
}
impl AstNode for SequenceDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SEQUENCE_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(SequenceDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl SequenceDecl {
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn endsequence_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDSEQUENCE_KW])
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn port_list(&self) -> Option<PortList> {
        support::child(&self.syntax)
    }
    pub fn property_spec(&self) -> Option<PropertySpec> {
        support::child(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn sequence_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEQUENCE_KW])
    }
    pub fn var_decls(&self) -> AstChildren<VarDecl> {
        support::children(&self.syntax)
    }
}
/// A `CONCURRENT_ASSERTION` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConcurrentAssertion {
    syntax: SyntaxNode,
}
impl AstNode for ConcurrentAssertion {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CONCURRENT_ASSERTION
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ConcurrentAssertion { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl ConcurrentAssertion {
    pub fn else_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ELSE_KW])
    }
    pub fn keyword(&self) -> Option<SyntaxToken> {
        support::token(
            &self.syntax,
            &[ASSERT_KW, ASSUME_KW, COVER_KW, RESTRICT_KW, EXPECT_KW],
        )
    }
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_PAREN])
    }
    pub fn property_spec(&self) -> Option<PropertySpec> {
        support::child(&self.syntax)
    }
    pub fn property_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[PROPERTY_KW])
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
    pub fn sequence_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEQUENCE_KW])
    }
}
/// A `DEFAULT_DISABLE` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DefaultDisable {
    syntax: SyntaxNode,
}
impl AstNode for DefaultDisable {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == DEFAULT_DISABLE
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(DefaultDisable { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl DefaultDisable {
    pub fn default_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[DEFAULT_KW])
    }
    pub fn disable_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[DISABLE_KW])
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn iff_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IFF_KW])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
}
/// A `CLOCKING_DECL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClockingDecl {
    syntax: SyntaxNode,
}
impl AstNode for ClockingDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CLOCKING_DECL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ClockingDecl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl ClockingDecl {
    pub fn clocking_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[CLOCKING_KW])
    }
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn default_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[DEFAULT_KW])
    }
    pub fn endclocking_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDCLOCKING_KW])
    }
    pub fn event_control(&self) -> Option<EventControl> {
        support::child(&self.syntax)
    }
    pub fn global_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[GLOBAL_KW])
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
}
/// A `CLOCKING_ITEM` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClockingItem {
    syntax: SyntaxNode,
}
impl AstNode for ClockingItem {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CLOCKING_ITEM
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ClockingItem { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl ClockingItem {
    pub fn declarators(&self) -> AstChildren<Declarator> {
        support::children(&self.syntax)
    }
    pub fn default_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[DEFAULT_KW])
    }
    pub fn delay_controls(&self) -> AstChildren<DelayControl> {
        support::children(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
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
    pub fn macro_arg_list(&self) -> Option<MacroArgList> {
        support::child(&self.syntax)
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TICK_IDENT])
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
    pub fn conditional_branches(&self) -> AstChildren<ConditionalBranch> {
        support::children(&self.syntax)
    }
    pub fn endif(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TICK_IDENT])
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
    pub fn assignments(&self) -> AstChildren<Assignment> {
        support::children(&self.syntax)
    }
    pub fn exprs(&self) -> AstChildren<Expr> {
        support::children(&self.syntax)
    }
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_PAREN])
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
    pub fn type_or_exprs(&self) -> AstChildren<TypeOrExpr> {
        support::children(&self.syntax)
    }
    pub fn var_decls(&self) -> AstChildren<VarDecl> {
        support::children(&self.syntax)
    }
    pub fn verbatim(&self) -> Option<Verbatim> {
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
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
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
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn op(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[PLUS_PLUS, MINUS_MINUS])
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
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
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
                MATCHES_KW,
                AMP_AMP_AMP,
            ],
        )
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
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn question_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[QUESTION])
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
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn field(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT, NEW_KW])
    }
    pub fn macro_call(&self) -> Option<MacroCall> {
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
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn member(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT, NEW_KW])
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
    pub fn arg_list(&self) -> Option<ArgList> {
        support::child(&self.syntax)
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn hash_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[HASH])
    }
    pub fn with_clause(&self) -> Option<WithClause> {
        support::child(&self.syntax)
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
    pub fn exprs(&self) -> AstChildren<Expr> {
        support::children(&self.syntax)
    }
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACE])
    }
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACE])
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
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn pattern_items(&self) -> AstChildren<PatternItem> {
        support::children(&self.syntax)
    }
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACE])
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
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn inside_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[INSIDE_KW])
    }
    pub fn range_list(&self) -> Option<RangeList> {
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
/// A `BINSOF_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BinsofExpr {
    syntax: SyntaxNode,
}
impl AstNode for BinsofExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == BINSOF_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(BinsofExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl BinsofExpr {
    pub fn binsof_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[BINSOF_KW])
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn intersect_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[INTERSECT_KW])
    }
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_PAREN])
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
    pub fn range_list(&self) -> Option<RangeList> {
        support::child(&self.syntax)
    }
}
/// A `TAGGED_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaggedExpr {
    syntax: SyntaxNode,
}
impl AstNode for TaggedExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == TAGGED_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(TaggedExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl TaggedExpr {
    pub fn member(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn tagged_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TAGGED_KW])
    }
    pub fn value(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}
/// A `BIND_PATTERN` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BindPattern {
    syntax: SyntaxNode,
}
impl AstNode for BindPattern {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == BIND_PATTERN
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(BindPattern { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl BindPattern {
    pub fn dot_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[DOT])
    }
    pub fn star_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[STAR])
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
    pub fn arg_list(&self) -> Option<ArgList> {
        support::child(&self.syntax)
    }
    pub fn dimensions(&self) -> AstChildren<Dimension> {
        support::children(&self.syntax)
    }
    pub fn hash_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[HASH])
    }
    pub fn preproc(&self) -> Option<Preproc> {
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
    pub fn enum_variants(&self) -> AstChildren<EnumVariant> {
        support::children(&self.syntax)
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
    pub fn dimensions(&self) -> AstChildren<Dimension> {
        support::children(&self.syntax)
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
    pub fn struct_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[STRUCT_KW])
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
    pub fn dimensions(&self) -> AstChildren<Dimension> {
        support::children(&self.syntax)
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
    pub fn union_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[UNION_KW])
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
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_PAREN])
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
    pub fn type_or_expr(&self) -> Option<TypeOrExpr> {
        support::child(&self.syntax)
    }
    pub fn type_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TYPE_KW])
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
    pub fn macro_args(&self) -> AstChildren<MacroArg> {
        support::children(&self.syntax)
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
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
    pub fn args(&self) -> AstChildren<Arg> {
        support::children(&self.syntax)
    }
    pub fn case_items(&self) -> AstChildren<CaseItem> {
        support::children(&self.syntax)
    }
    pub fn condition(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT])
    }
    pub fn directive(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[TICK_IDENT])
    }
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
    pub fn ports(&self) -> AstChildren<Port> {
        support::children(&self.syntax)
    }
}
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
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
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
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
    pub fn data_type(&self) -> Option<DataType> {
        support::child(&self.syntax)
    }
    pub fn declarator(&self) -> Option<Declarator> {
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
    pub fn delay_control(&self) -> Option<DelayControl> {
        support::child(&self.syntax)
    }
    pub fn event_control(&self) -> Option<EventControl> {
        support::child(&self.syntax)
    }
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
    pub fn repeat_control(&self) -> Option<RepeatControl> {
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
    pub fn attribute_specs(&self) -> AstChildren<AttributeSpec> {
        support::children(&self.syntax)
    }
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_PAREN])
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
    pub fn star_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[STAR])
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
    pub fn args(&self) -> AstChildren<Arg> {
        support::children(&self.syntax)
    }
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_PAREN])
    }
    pub fn preprocs(&self) -> AstChildren<Preproc> {
        support::children(&self.syntax)
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
    pub fn verbatims(&self) -> AstChildren<Verbatim> {
        support::children(&self.syntax)
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
    pub fn constraint_block(&self) -> Option<ConstraintBlock> {
        support::child(&self.syntax)
    }
    pub fn paren_expr(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn with_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[WITH_KW])
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
    pub fn dist_items(&self) -> AstChildren<DistItem> {
        support::children(&self.syntax)
    }
    pub fn exprs(&self) -> AstChildren<Expr> {
        support::children(&self.syntax)
    }
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACE])
    }
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACE])
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
    pub fn eq_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[EQ])
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT])
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
    pub fn dimension(&self) -> Option<Dimension> {
        support::child(&self.syntax)
    }
    pub fn eq_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[EQ])
    }
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT])
    }
    pub fn preproc(&self) -> Option<Preproc> {
        support::child(&self.syntax)
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
    pub fn attributeses(&self) -> AstChildren<Attributes> {
        support::children(&self.syntax)
    }
    pub fn data_type(&self) -> Option<DataType> {
        support::child(&self.syntax)
    }
    pub fn declarators(&self) -> AstChildren<Declarator> {
        support::children(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
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
    pub fn dimensions(&self) -> AstChildren<Dimension> {
        support::children(&self.syntax)
    }
    pub fn eq_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[EQ])
    }
    pub fn init(&self) -> Option<TypeOrExpr> {
        support::child(&self.syntax)
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn preproc(&self) -> Option<Preproc> {
        support::child(&self.syntax)
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
    pub fn param_decls(&self) -> AstChildren<ParamDecl> {
        support::children(&self.syntax)
    }
    pub fn preprocs(&self) -> AstChildren<Preproc> {
        support::children(&self.syntax)
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
    pub fn verbatims(&self) -> AstChildren<Verbatim> {
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
    pub fn ports(&self) -> AstChildren<Port> {
        support::children(&self.syntax)
    }
    pub fn preprocs(&self) -> AstChildren<Preproc> {
        support::children(&self.syntax)
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
    pub fn verbatims(&self) -> AstChildren<Verbatim> {
        support::children(&self.syntax)
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
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn op(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[HASH, HASH_HASH])
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
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn star_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[STAR])
    }
}
/// A `REPEAT_CONTROL` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepeatControl {
    syntax: SyntaxNode,
}
impl AstNode for RepeatControl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == REPEAT_CONTROL
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(RepeatControl { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl RepeatControl {
    pub fn count(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn event_control(&self) -> Option<EventControl> {
        support::child(&self.syntax)
    }
    pub fn repeat_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[REPEAT_KW])
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
    pub fn arg_list(&self) -> Option<ArgList> {
        support::child(&self.syntax)
    }
    pub fn dimensions(&self) -> AstChildren<Dimension> {
        support::children(&self.syntax)
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
}
/// A `PROPERTY_BIN_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PropertyBinExpr {
    syntax: SyntaxNode,
}
impl AstNode for PropertyBinExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PROPERTY_BIN_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(PropertyBinExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl PropertyBinExpr {
    pub fn op(&self) -> Option<SyntaxToken> {
        support::token(
            &self.syntax,
            &[
                PIPE_MINUS_GT,
                PIPE_EQ_GT,
                HASH_MINUS_HASH,
                HASH_EQ_HASH,
                UNTIL_KW,
                S_UNTIL_KW,
                UNTIL_WITH_KW,
                S_UNTIL_WITH_KW,
                IMPLIES_KW,
                IFF_KW,
                OR_KW,
                AND_KW,
                INTERSECT_KW,
                WITHIN_KW,
                THROUGHOUT_KW,
            ],
        )
    }
}
/// A `PROPERTY_UNARY_EXPR` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PropertyUnaryExpr {
    syntax: SyntaxNode,
}
impl AstNode for PropertyUnaryExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PROPERTY_UNARY_EXPR
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(PropertyUnaryExpr { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl PropertyUnaryExpr {
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn l_brack_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACK])
    }
    pub fn op(&self) -> Option<SyntaxToken> {
        support::token(
            &self.syntax,
            &[
                NOT_KW,
                NEXTTIME_KW,
                S_NEXTTIME_KW,
                ALWAYS_KW,
                S_ALWAYS_KW,
                EVENTUALLY_KW,
                S_EVENTUALLY_KW,
                ACCEPT_ON_KW,
                REJECT_ON_KW,
                SYNC_ACCEPT_ON_KW,
                SYNC_REJECT_ON_KW,
                STRONG_KW,
                WEAK_KW,
                FIRST_MATCH_KW,
            ],
        )
    }
    pub fn r_brack_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACK])
    }
}
/// A `PROPERTY_PAREN` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PropertyParen {
    syntax: SyntaxNode,
}
impl AstNode for PropertyParen {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PROPERTY_PAREN
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(PropertyParen { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl PropertyParen {
    pub fn assignments(&self) -> AstChildren<Assignment> {
        support::children(&self.syntax)
    }
    pub fn exprs(&self) -> AstChildren<Expr> {
        support::children(&self.syntax)
    }
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_PAREN])
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
}
/// A `PROPERTY_IF` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PropertyIf {
    syntax: SyntaxNode,
}
impl AstNode for PropertyIf {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PROPERTY_IF
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(PropertyIf { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl PropertyIf {
    pub fn else_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ELSE_KW])
    }
    pub fn if_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IF_KW])
    }
}
/// A `CLOCKED_PROPERTY` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClockedProperty {
    syntax: SyntaxNode,
}
impl AstNode for ClockedProperty {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CLOCKED_PROPERTY
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ClockedProperty { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl ClockedProperty {
    pub fn event_control(&self) -> Option<EventControl> {
        support::child(&self.syntax)
    }
    pub fn prop_expr(&self) -> Option<PropExpr> {
        support::child(&self.syntax)
    }
}
/// A `SEQUENCE_DELAY` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SequenceDelay {
    syntax: SyntaxNode,
}
impl AstNode for SequenceDelay {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SEQUENCE_DELAY
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(SequenceDelay { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl SequenceDelay {
    pub fn cycle_delay(&self) -> Option<CycleDelay> {
        support::child(&self.syntax)
    }
}
/// A `REPETITION` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Repetition {
    syntax: SyntaxNode,
}
impl AstNode for Repetition {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == REPETITION
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Repetition { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl Repetition {
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn l_brack_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACK])
    }
    pub fn op(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[STAR, EQ, MINUS_GT, PLUS])
    }
    pub fn r_brack_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACK])
    }
}
/// A `PROPERTY_SPEC` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PropertySpec {
    syntax: SyntaxNode,
}
impl AstNode for PropertySpec {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PROPERTY_SPEC
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(PropertySpec { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl PropertySpec {
    pub fn disable_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[DISABLE_KW])
    }
    pub fn event_control(&self) -> Option<EventControl> {
        support::child(&self.syntax)
    }
    pub fn iff_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IFF_KW])
    }
}
/// A `CYCLE_DELAY` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CycleDelay {
    syntax: SyntaxNode,
}
impl AstNode for CycleDelay {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == CYCLE_DELAY
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(CycleDelay { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl CycleDelay {
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn hash_hash_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[HASH_HASH])
    }
    pub fn l_brack_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACK])
    }
    pub fn plus_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[PLUS])
    }
    pub fn r_brack_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACK])
    }
    pub fn star_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[STAR])
    }
}
/// A `BINS_BLOCK` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BinsBlock {
    syntax: SyntaxNode,
}
impl AstNode for BinsBlock {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == BINS_BLOCK
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(BinsBlock { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl BinsBlock {
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACE])
    }
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACE])
    }
}
/// A `TRANS_SET` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransSet {
    syntax: SyntaxNode,
}
impl AstNode for TransSet {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == TRANS_SET
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(TransSet { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl TransSet {
    pub fn exprs(&self) -> AstChildren<Expr> {
        support::children(&self.syntax)
    }
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_PAREN])
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
}
/// A `PRODUCTION` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Production {
    syntax: SyntaxNode,
}
impl AstNode for Production {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PRODUCTION
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(Production { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl Production {
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn data_type(&self) -> Option<DataType> {
        support::child(&self.syntax)
    }
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IDENT, ESCAPED_IDENT])
    }
    pub fn port_list(&self) -> Option<PortList> {
        support::child(&self.syntax)
    }
    pub fn production_rules(&self) -> AstChildren<ProductionRule> {
        support::children(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
}
/// A `PRODUCTION_RULE` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProductionRule {
    syntax: SyntaxNode,
}
impl AstNode for ProductionRule {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PRODUCTION_RULE
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ProductionRule { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl ProductionRule {
    pub fn colon_eq_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON_EQ])
    }
    pub fn exprs(&self) -> AstChildren<Expr> {
        support::children(&self.syntax)
    }
    pub fn join_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[JOIN_KW])
    }
    pub fn production_blocks(&self) -> AstChildren<ProductionBlock> {
        support::children(&self.syntax)
    }
    pub fn production_cases(&self) -> AstChildren<ProductionCase> {
        support::children(&self.syntax)
    }
    pub fn production_ifs(&self) -> AstChildren<ProductionIf> {
        support::children(&self.syntax)
    }
    pub fn production_repeats(&self) -> AstChildren<ProductionRepeat> {
        support::children(&self.syntax)
    }
    pub fn rand_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[RAND_KW])
    }
}
/// A `PRODUCTION_BLOCK` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProductionBlock {
    syntax: SyntaxNode,
}
impl AstNode for ProductionBlock {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PRODUCTION_BLOCK
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ProductionBlock { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl ProductionBlock {
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
    pub fn l_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACE])
    }
    pub fn r_brace_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACE])
    }
}
/// A `PRODUCTION_IF` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProductionIf {
    syntax: SyntaxNode,
}
impl AstNode for ProductionIf {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PRODUCTION_IF
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ProductionIf { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl ProductionIf {
    pub fn else_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ELSE_KW])
    }
    pub fn if_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[IF_KW])
    }
}
/// A `PRODUCTION_REPEAT` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProductionRepeat {
    syntax: SyntaxNode,
}
impl AstNode for ProductionRepeat {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PRODUCTION_REPEAT
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ProductionRepeat { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl ProductionRepeat {
    pub fn repeat_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[REPEAT_KW])
    }
}
/// A `PRODUCTION_CASE` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProductionCase {
    syntax: SyntaxNode,
}
impl AstNode for ProductionCase {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PRODUCTION_CASE
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ProductionCase { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl ProductionCase {
    pub fn case_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[CASE_KW])
    }
    pub fn endcase_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[ENDCASE_KW])
    }
    pub fn paren_expr(&self) -> Option<ParenExpr> {
        support::child(&self.syntax)
    }
    pub fn production_case_items(&self) -> AstChildren<ProductionCaseItem> {
        support::children(&self.syntax)
    }
}
/// A `PRODUCTION_CASE_ITEM` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProductionCaseItem {
    syntax: SyntaxNode,
}
impl AstNode for ProductionCaseItem {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == PRODUCTION_CASE_ITEM
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ProductionCaseItem { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl ProductionCaseItem {
    pub fn colon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[COLON])
    }
    pub fn default_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[DEFAULT_KW])
    }
    pub fn exprs(&self) -> AstChildren<Expr> {
        support::children(&self.syntax)
    }
    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[SEMICOLON])
    }
}
/// A `FOREACH_HEADER` node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForeachHeader {
    syntax: SyntaxNode,
}
impl AstNode for ForeachHeader {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == FOREACH_HEADER
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Self::can_cast(syntax.kind()).then_some(ForeachHeader { syntax })
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}
impl ForeachHeader {
    pub fn array(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
    pub fn l_brack_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_BRACK])
    }
    pub fn l_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[L_PAREN])
    }
    pub fn r_brack_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_BRACK])
    }
    pub fn r_paren_token(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, &[R_PAREN])
    }
}
/// Any of [`ModuleDecl`], [`InterfaceDecl`], [`ProgramDecl`], [`PackageDecl`], [`ClassDecl`], [`FunctionDecl`], [`TaskDecl`], [`ConstraintDecl`], [`VarDecl`], [`ParamDecl`], [`Typedef`], [`ImportDecl`], [`TimeunitDecl`], [`PortDecl`], [`ModportDecl`], [`ContinuousAssign`], [`Instantiation`], [`BindDirective`], [`ProceduralBlock`], [`GenerateRegion`], [`Block`], [`ExprStmt`], [`LabeledStmt`], [`IfStmt`], [`CaseStmt`], [`RandsequenceStmt`], [`ForStmt`], [`ForeachStmt`], [`WhileStmt`], [`DoWhileStmt`], [`RepeatStmt`], [`ForeverStmt`], [`ReturnStmt`], [`BreakStmt`], [`ContinueStmt`], [`DisableStmt`], [`WaitStmt`], [`EventTrigger`], [`TimingStmt`], [`ImmediateAssertion`], [`ProceduralAssign`], [`ConstraintBlock`], [`ConstraintExpr`], [`Implication`], [`SolveBefore`], [`CovergroupDecl`], [`Coverpoint`], [`Cross`], [`Bins`], [`PropertyDecl`], [`SequenceDecl`], [`ConcurrentAssertion`], [`DefaultDisable`], [`ClockingDecl`], [`ClockingItem`], [`Preproc`], [`Verbatim`].
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
    TimeunitDecl(TimeunitDecl),
    PortDecl(PortDecl),
    ModportDecl(ModportDecl),
    ContinuousAssign(ContinuousAssign),
    Instantiation(Instantiation),
    BindDirective(BindDirective),
    ProceduralBlock(ProceduralBlock),
    GenerateRegion(GenerateRegion),
    Block(Block),
    ExprStmt(ExprStmt),
    LabeledStmt(LabeledStmt),
    IfStmt(IfStmt),
    CaseStmt(CaseStmt),
    RandsequenceStmt(RandsequenceStmt),
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
    ImmediateAssertion(ImmediateAssertion),
    ProceduralAssign(ProceduralAssign),
    ConstraintBlock(ConstraintBlock),
    ConstraintExpr(ConstraintExpr),
    Implication(Implication),
    SolveBefore(SolveBefore),
    CovergroupDecl(CovergroupDecl),
    Coverpoint(Coverpoint),
    Cross(Cross),
    Bins(Bins),
    PropertyDecl(PropertyDecl),
    SequenceDecl(SequenceDecl),
    ConcurrentAssertion(ConcurrentAssertion),
    DefaultDisable(DefaultDisable),
    ClockingDecl(ClockingDecl),
    ClockingItem(ClockingItem),
    Preproc(Preproc),
    Verbatim(Verbatim),
}
impl AstNode for Item {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind, MODULE_DECL | INTERFACE_DECL | PROGRAM_DECL | PACKAGE_DECL | CLASS_DECL
            | FUNCTION_DECL | TASK_DECL | CONSTRAINT_DECL | VAR_DECL | PARAM_DECL |
            TYPEDEF | IMPORT_DECL | TIMEUNIT_DECL | PORT_DECL | MODPORT_DECL |
            CONTINUOUS_ASSIGN | INSTANTIATION | BIND_DIRECTIVE | PROCEDURAL_BLOCK |
            GENERATE_REGION | BLOCK | EXPR_STMT | LABELED_STMT | IF_STMT | CASE_STMT |
            RANDSEQUENCE_STMT | FOR_STMT | FOREACH_STMT | WHILE_STMT | DO_WHILE_STMT |
            REPEAT_STMT | FOREVER_STMT | RETURN_STMT | BREAK_STMT | CONTINUE_STMT |
            DISABLE_STMT | WAIT_STMT | EVENT_TRIGGER | TIMING_STMT | IMMEDIATE_ASSERTION
            | PROCEDURAL_ASSIGN | CONSTRAINT_BLOCK | CONSTRAINT_EXPR | IMPLICATION |
            SOLVE_BEFORE | COVERGROUP_DECL | COVERPOINT | CROSS | BINS | PROPERTY_DECL |
            SEQUENCE_DECL | CONCURRENT_ASSERTION | DEFAULT_DISABLE | CLOCKING_DECL |
            CLOCKING_ITEM | VERBATIM
        ) || Preproc::can_cast(kind)
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        match syntax.kind() {
            MODULE_DECL => Some(Self::ModuleDecl(ModuleDecl { syntax })),
            INTERFACE_DECL => Some(Self::InterfaceDecl(InterfaceDecl { syntax })),
            PROGRAM_DECL => Some(Self::ProgramDecl(ProgramDecl { syntax })),
            PACKAGE_DECL => Some(Self::PackageDecl(PackageDecl { syntax })),
            CLASS_DECL => Some(Self::ClassDecl(ClassDecl { syntax })),
            FUNCTION_DECL => Some(Self::FunctionDecl(FunctionDecl { syntax })),
            TASK_DECL => Some(Self::TaskDecl(TaskDecl { syntax })),
            CONSTRAINT_DECL => Some(Self::ConstraintDecl(ConstraintDecl { syntax })),
            VAR_DECL => Some(Self::VarDecl(VarDecl { syntax })),
            PARAM_DECL => Some(Self::ParamDecl(ParamDecl { syntax })),
            TYPEDEF => Some(Self::Typedef(Typedef { syntax })),
            IMPORT_DECL => Some(Self::ImportDecl(ImportDecl { syntax })),
            TIMEUNIT_DECL => Some(Self::TimeunitDecl(TimeunitDecl { syntax })),
            PORT_DECL => Some(Self::PortDecl(PortDecl { syntax })),
            MODPORT_DECL => Some(Self::ModportDecl(ModportDecl { syntax })),
            CONTINUOUS_ASSIGN => {
                Some(Self::ContinuousAssign(ContinuousAssign { syntax }))
            }
            INSTANTIATION => Some(Self::Instantiation(Instantiation { syntax })),
            BIND_DIRECTIVE => Some(Self::BindDirective(BindDirective { syntax })),
            PROCEDURAL_BLOCK => Some(Self::ProceduralBlock(ProceduralBlock { syntax })),
            GENERATE_REGION => Some(Self::GenerateRegion(GenerateRegion { syntax })),
            BLOCK => Some(Self::Block(Block { syntax })),
            EXPR_STMT => Some(Self::ExprStmt(ExprStmt { syntax })),
            LABELED_STMT => Some(Self::LabeledStmt(LabeledStmt { syntax })),
            IF_STMT => Some(Self::IfStmt(IfStmt { syntax })),
            CASE_STMT => Some(Self::CaseStmt(CaseStmt { syntax })),
            RANDSEQUENCE_STMT => {
                Some(Self::RandsequenceStmt(RandsequenceStmt { syntax }))
            }
            FOR_STMT => Some(Self::ForStmt(ForStmt { syntax })),
            FOREACH_STMT => Some(Self::ForeachStmt(ForeachStmt { syntax })),
            WHILE_STMT => Some(Self::WhileStmt(WhileStmt { syntax })),
            DO_WHILE_STMT => Some(Self::DoWhileStmt(DoWhileStmt { syntax })),
            REPEAT_STMT => Some(Self::RepeatStmt(RepeatStmt { syntax })),
            FOREVER_STMT => Some(Self::ForeverStmt(ForeverStmt { syntax })),
            RETURN_STMT => Some(Self::ReturnStmt(ReturnStmt { syntax })),
            BREAK_STMT => Some(Self::BreakStmt(BreakStmt { syntax })),
            CONTINUE_STMT => Some(Self::ContinueStmt(ContinueStmt { syntax })),
            DISABLE_STMT => Some(Self::DisableStmt(DisableStmt { syntax })),
            WAIT_STMT => Some(Self::WaitStmt(WaitStmt { syntax })),
            EVENT_TRIGGER => Some(Self::EventTrigger(EventTrigger { syntax })),
            TIMING_STMT => Some(Self::TimingStmt(TimingStmt { syntax })),
            IMMEDIATE_ASSERTION => {
                Some(Self::ImmediateAssertion(ImmediateAssertion { syntax }))
            }
            PROCEDURAL_ASSIGN => {
                Some(Self::ProceduralAssign(ProceduralAssign { syntax }))
            }
            CONSTRAINT_BLOCK => Some(Self::ConstraintBlock(ConstraintBlock { syntax })),
            CONSTRAINT_EXPR => Some(Self::ConstraintExpr(ConstraintExpr { syntax })),
            IMPLICATION => Some(Self::Implication(Implication { syntax })),
            SOLVE_BEFORE => Some(Self::SolveBefore(SolveBefore { syntax })),
            COVERGROUP_DECL => Some(Self::CovergroupDecl(CovergroupDecl { syntax })),
            COVERPOINT => Some(Self::Coverpoint(Coverpoint { syntax })),
            CROSS => Some(Self::Cross(Cross { syntax })),
            BINS => Some(Self::Bins(Bins { syntax })),
            PROPERTY_DECL => Some(Self::PropertyDecl(PropertyDecl { syntax })),
            SEQUENCE_DECL => Some(Self::SequenceDecl(SequenceDecl { syntax })),
            CONCURRENT_ASSERTION => {
                Some(Self::ConcurrentAssertion(ConcurrentAssertion { syntax }))
            }
            DEFAULT_DISABLE => Some(Self::DefaultDisable(DefaultDisable { syntax })),
            CLOCKING_DECL => Some(Self::ClockingDecl(ClockingDecl { syntax })),
            CLOCKING_ITEM => Some(Self::ClockingItem(ClockingItem { syntax })),
            VERBATIM => Some(Self::Verbatim(Verbatim { syntax })),
            kind if Preproc::can_cast(kind) => Preproc::cast(syntax).map(Self::Preproc),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::ModuleDecl(it) => it.syntax(),
            Self::InterfaceDecl(it) => it.syntax(),
            Self::ProgramDecl(it) => it.syntax(),
            Self::PackageDecl(it) => it.syntax(),
            Self::ClassDecl(it) => it.syntax(),
            Self::FunctionDecl(it) => it.syntax(),
            Self::TaskDecl(it) => it.syntax(),
            Self::ConstraintDecl(it) => it.syntax(),
            Self::VarDecl(it) => it.syntax(),
            Self::ParamDecl(it) => it.syntax(),
            Self::Typedef(it) => it.syntax(),
            Self::ImportDecl(it) => it.syntax(),
            Self::TimeunitDecl(it) => it.syntax(),
            Self::PortDecl(it) => it.syntax(),
            Self::ModportDecl(it) => it.syntax(),
            Self::ContinuousAssign(it) => it.syntax(),
            Self::Instantiation(it) => it.syntax(),
            Self::BindDirective(it) => it.syntax(),
            Self::ProceduralBlock(it) => it.syntax(),
            Self::GenerateRegion(it) => it.syntax(),
            Self::Block(it) => it.syntax(),
            Self::ExprStmt(it) => it.syntax(),
            Self::LabeledStmt(it) => it.syntax(),
            Self::IfStmt(it) => it.syntax(),
            Self::CaseStmt(it) => it.syntax(),
            Self::RandsequenceStmt(it) => it.syntax(),
            Self::ForStmt(it) => it.syntax(),
            Self::ForeachStmt(it) => it.syntax(),
            Self::WhileStmt(it) => it.syntax(),
            Self::DoWhileStmt(it) => it.syntax(),
            Self::RepeatStmt(it) => it.syntax(),
            Self::ForeverStmt(it) => it.syntax(),
            Self::ReturnStmt(it) => it.syntax(),
            Self::BreakStmt(it) => it.syntax(),
            Self::ContinueStmt(it) => it.syntax(),
            Self::DisableStmt(it) => it.syntax(),
            Self::WaitStmt(it) => it.syntax(),
            Self::EventTrigger(it) => it.syntax(),
            Self::TimingStmt(it) => it.syntax(),
            Self::ImmediateAssertion(it) => it.syntax(),
            Self::ProceduralAssign(it) => it.syntax(),
            Self::ConstraintBlock(it) => it.syntax(),
            Self::ConstraintExpr(it) => it.syntax(),
            Self::Implication(it) => it.syntax(),
            Self::SolveBefore(it) => it.syntax(),
            Self::CovergroupDecl(it) => it.syntax(),
            Self::Coverpoint(it) => it.syntax(),
            Self::Cross(it) => it.syntax(),
            Self::Bins(it) => it.syntax(),
            Self::PropertyDecl(it) => it.syntax(),
            Self::SequenceDecl(it) => it.syntax(),
            Self::ConcurrentAssertion(it) => it.syntax(),
            Self::DefaultDisable(it) => it.syntax(),
            Self::ClockingDecl(it) => it.syntax(),
            Self::ClockingItem(it) => it.syntax(),
            Self::Preproc(it) => it.syntax(),
            Self::Verbatim(it) => it.syntax(),
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
            MACRO_CALL => Some(Self::MacroCall(MacroCall { syntax })),
            DIRECTIVE => Some(Self::Directive(Directive { syntax })),
            CONDITIONAL_REGION => {
                Some(Self::ConditionalRegion(ConditionalRegion { syntax }))
            }
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::MacroCall(it) => it.syntax(),
            Self::Directive(it) => it.syntax(),
            Self::ConditionalRegion(it) => it.syntax(),
        }
    }
}
/// Any of [`LiteralExpr`], [`NameRef`], [`ParenExpr`], [`UnaryExpr`], [`PostfixExpr`], [`BinExpr`], [`TernaryExpr`], [`FieldExpr`], [`ScopeExpr`], [`IndexExpr`], [`CallExpr`], [`CastExpr`], [`ConcatExpr`], [`ReplicationExpr`], [`StreamExpr`], [`AssignmentPattern`], [`InsideExpr`], [`DistExpr`], [`BinsofExpr`], [`TaggedExpr`], [`BindPattern`].
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
    BinsofExpr(BinsofExpr),
    TaggedExpr(TaggedExpr),
    BindPattern(BindPattern),
}
impl AstNode for Expr {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind, LITERAL_EXPR | NAME_REF | PAREN_EXPR | UNARY_EXPR | POSTFIX_EXPR |
            BIN_EXPR | TERNARY_EXPR | FIELD_EXPR | SCOPE_EXPR | INDEX_EXPR | CALL_EXPR |
            CAST_EXPR | CONCAT_EXPR | REPLICATION_EXPR | STREAM_EXPR | ASSIGNMENT_PATTERN
            | INSIDE_EXPR | DIST_EXPR | BINSOF_EXPR | TAGGED_EXPR | BIND_PATTERN
        )
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        match syntax.kind() {
            LITERAL_EXPR => Some(Self::LiteralExpr(LiteralExpr { syntax })),
            NAME_REF => Some(Self::NameRef(NameRef { syntax })),
            PAREN_EXPR => Some(Self::ParenExpr(ParenExpr { syntax })),
            UNARY_EXPR => Some(Self::UnaryExpr(UnaryExpr { syntax })),
            POSTFIX_EXPR => Some(Self::PostfixExpr(PostfixExpr { syntax })),
            BIN_EXPR => Some(Self::BinExpr(BinExpr { syntax })),
            TERNARY_EXPR => Some(Self::TernaryExpr(TernaryExpr { syntax })),
            FIELD_EXPR => Some(Self::FieldExpr(FieldExpr { syntax })),
            SCOPE_EXPR => Some(Self::ScopeExpr(ScopeExpr { syntax })),
            INDEX_EXPR => Some(Self::IndexExpr(IndexExpr { syntax })),
            CALL_EXPR => Some(Self::CallExpr(CallExpr { syntax })),
            CAST_EXPR => Some(Self::CastExpr(CastExpr { syntax })),
            CONCAT_EXPR => Some(Self::ConcatExpr(ConcatExpr { syntax })),
            REPLICATION_EXPR => Some(Self::ReplicationExpr(ReplicationExpr { syntax })),
            STREAM_EXPR => Some(Self::StreamExpr(StreamExpr { syntax })),
            ASSIGNMENT_PATTERN => {
                Some(Self::AssignmentPattern(AssignmentPattern { syntax }))
            }
            INSIDE_EXPR => Some(Self::InsideExpr(InsideExpr { syntax })),
            DIST_EXPR => Some(Self::DistExpr(DistExpr { syntax })),
            BINSOF_EXPR => Some(Self::BinsofExpr(BinsofExpr { syntax })),
            TAGGED_EXPR => Some(Self::TaggedExpr(TaggedExpr { syntax })),
            BIND_PATTERN => Some(Self::BindPattern(BindPattern { syntax })),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::LiteralExpr(it) => it.syntax(),
            Self::NameRef(it) => it.syntax(),
            Self::ParenExpr(it) => it.syntax(),
            Self::UnaryExpr(it) => it.syntax(),
            Self::PostfixExpr(it) => it.syntax(),
            Self::BinExpr(it) => it.syntax(),
            Self::TernaryExpr(it) => it.syntax(),
            Self::FieldExpr(it) => it.syntax(),
            Self::ScopeExpr(it) => it.syntax(),
            Self::IndexExpr(it) => it.syntax(),
            Self::CallExpr(it) => it.syntax(),
            Self::CastExpr(it) => it.syntax(),
            Self::ConcatExpr(it) => it.syntax(),
            Self::ReplicationExpr(it) => it.syntax(),
            Self::StreamExpr(it) => it.syntax(),
            Self::AssignmentPattern(it) => it.syntax(),
            Self::InsideExpr(it) => it.syntax(),
            Self::DistExpr(it) => it.syntax(),
            Self::BinsofExpr(it) => it.syntax(),
            Self::TaggedExpr(it) => it.syntax(),
            Self::BindPattern(it) => it.syntax(),
        }
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
        matches!(kind, TYPE_REF | ENUM_TYPE | STRUCT_TYPE | UNION_TYPE | TYPE_REFERENCE)
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        match syntax.kind() {
            TYPE_REF => Some(Self::TypeRef(TypeRef { syntax })),
            ENUM_TYPE => Some(Self::EnumType(EnumType { syntax })),
            STRUCT_TYPE => Some(Self::StructType(StructType { syntax })),
            UNION_TYPE => Some(Self::UnionType(UnionType { syntax })),
            TYPE_REFERENCE => Some(Self::TypeReference(TypeReference { syntax })),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::TypeRef(it) => it.syntax(),
            Self::EnumType(it) => it.syntax(),
            Self::StructType(it) => it.syntax(),
            Self::UnionType(it) => it.syntax(),
            Self::TypeReference(it) => it.syntax(),
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
            kind if Expr::can_cast(kind) => Expr::cast(syntax).map(Self::Expr),
            kind if DataType::can_cast(kind) => {
                DataType::cast(syntax).map(Self::DataType)
            }
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Expr(it) => it.syntax(),
            Self::DataType(it) => it.syntax(),
        }
    }
}
/// Any of [`Expr`], [`PropertyBinExpr`], [`PropertyUnaryExpr`], [`PropertyParen`], [`PropertyIf`], [`ClockedProperty`], [`SequenceDelay`], [`Repetition`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PropExpr {
    Expr(Expr),
    PropertyBinExpr(PropertyBinExpr),
    PropertyUnaryExpr(PropertyUnaryExpr),
    PropertyParen(PropertyParen),
    PropertyIf(PropertyIf),
    ClockedProperty(ClockedProperty),
    SequenceDelay(SequenceDelay),
    Repetition(Repetition),
}
impl AstNode for PropExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind, PROPERTY_BIN_EXPR | PROPERTY_UNARY_EXPR | PROPERTY_PAREN | PROPERTY_IF
            | CLOCKED_PROPERTY | SEQUENCE_DELAY | REPETITION
        ) || Expr::can_cast(kind)
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        match syntax.kind() {
            PROPERTY_BIN_EXPR => Some(Self::PropertyBinExpr(PropertyBinExpr { syntax })),
            PROPERTY_UNARY_EXPR => {
                Some(Self::PropertyUnaryExpr(PropertyUnaryExpr { syntax }))
            }
            PROPERTY_PAREN => Some(Self::PropertyParen(PropertyParen { syntax })),
            PROPERTY_IF => Some(Self::PropertyIf(PropertyIf { syntax })),
            CLOCKED_PROPERTY => Some(Self::ClockedProperty(ClockedProperty { syntax })),
            SEQUENCE_DELAY => Some(Self::SequenceDelay(SequenceDelay { syntax })),
            REPETITION => Some(Self::Repetition(Repetition { syntax })),
            kind if Expr::can_cast(kind) => Expr::cast(syntax).map(Self::Expr),
            _ => None,
        }
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Expr(it) => it.syntax(),
            Self::PropertyBinExpr(it) => it.syntax(),
            Self::PropertyUnaryExpr(it) => it.syntax(),
            Self::PropertyParen(it) => it.syntax(),
            Self::PropertyIf(it) => it.syntax(),
            Self::ClockedProperty(it) => it.syntax(),
            Self::SequenceDelay(it) => it.syntax(),
            Self::Repetition(it) => it.syntax(),
        }
    }
}
