//! The HIR: one file's declarations, statements and expressions, in arenas.

use std::ops::Index;

use astli_syntax::SyntaxKind;
use astli_text::Span;

/// One file, lowered: its scopes, the symbols declared in them, and the
/// statements and expressions they hold, each addressed by an id.
///
/// It holds no syntax node, so it can be sent across threads and outlive its
/// tree. Its spans are those of the tree it was lowered from, which the
/// session that parsed it resolves.
#[derive(Debug, Clone, Default)]
pub struct Hir {
    pub(crate) scopes: Vec<Scope>,
    pub(crate) symbols: Vec<Symbol>,
    pub(crate) stmts: Vec<Stmt>,
    pub(crate) exprs: Vec<Expr>,
}

macro_rules! ids {
    ($($(#[$doc:meta])* $id:ident => $field:ident: $ty:ty;)*) => {$(
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $id(pub(crate) u32);

        impl $id {
            pub fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl Index<$id> for Hir {
            type Output = $ty;

            fn index(&self, id: $id) -> &$ty {
                &self.$field[id.index()]
            }
        }
    )*};
}

ids! {
    /// A [`Scope`] of a [`Hir`].
    ScopeId => scopes: Scope;
    /// A [`Symbol`] of a [`Hir`].
    SymbolId => symbols: Symbol;
    /// A [`Stmt`] of a [`Hir`].
    StmtId => stmts: Stmt;
    /// An [`Expr`] of a [`Hir`].
    ExprId => exprs: Expr;
}

impl Hir {
    /// The file's top level, which is part of its compilation unit's `$unit`.
    pub fn root(&self) -> ScopeId {
        ScopeId(0)
    }

    pub fn scopes(&self) -> impl Iterator<Item = (ScopeId, &Scope)> {
        (self.scopes.iter().enumerate()).map(|(i, scope)| (ScopeId(i as u32), scope))
    }

    pub fn symbols(&self) -> impl Iterator<Item = (SymbolId, &Symbol)> {
        (self.symbols.iter().enumerate()).map(|(i, symbol)| (SymbolId(i as u32), symbol))
    }

    pub fn stmts(&self) -> impl Iterator<Item = (StmtId, &Stmt)> {
        (self.stmts.iter().enumerate()).map(|(i, stmt)| (StmtId(i as u32), stmt))
    }

    pub fn exprs(&self) -> impl Iterator<Item = (ExprId, &Expr)> {
        (self.exprs.iter().enumerate()).map(|(i, expr)| (ExprId(i as u32), expr))
    }
}

/// A region names are declared in: a file, a design element, a subroutine, a
/// block.
#[derive(Debug, Clone)]
pub struct Scope {
    pub kind: ScopeKind,
    /// The scope around this one; `None` for the file's.
    pub parent: Option<ScopeId>,
    /// The symbol this scope is the body of: a definition, a subroutine, a
    /// named block.
    pub owner: Option<SymbolId>,
    /// What stands in the scope, in the order written.
    pub members: Vec<Member>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    File,
    Definition,
    Subroutine,
    /// A generate block, or the arm of a generate `if` or `case` written
    /// without one.
    Generate,
    /// A procedural `begin` or `fork` block.
    Block,
    /// A loop's header, which may declare its variables: a `for`'s, a
    /// `foreach`'s, a generate loop's `genvar`.
    Loop,
}

/// One thing standing in a scope.
#[derive(Debug, Clone)]
pub enum Member {
    /// A declaration, whose initialiser, type and connections are in the
    /// symbol.
    Declare(SymbolId),
    Import(Import),
    /// A continuous assignment, each of its [`ExprKind::Assign`]s.
    Assign(Vec<ExprId>),
    Process(Process),
    /// A generate `if`, flattened with the `else if`s that follow it: at most
    /// one arm exists once elaborated.
    GenerateIf(Vec<GenerateArm>),
    GenerateCase {
        selector: ExprId,
        /// A `default` arm has no labels.
        arms: Vec<GenerateArm>,
    },
    GenerateFor {
        header: For,
        body: ScopeId,
    },
    /// A generate block standing alone, its label in its owner.
    Generate(ScopeId),
    Opaque(Opaque),
}

/// `import pkg::name;` or `import pkg::*;`, one per name the declaration
/// imports; also `export`.
#[derive(Debug, Clone)]
pub struct Import {
    pub package: Name,
    /// `None` for a wildcard.
    pub item: Option<Name>,
    pub export: bool,
}

/// An `always`, `initial` or `final` block.
#[derive(Debug, Clone)]
pub struct Process {
    /// Its keyword: `ALWAYS_FF_KW` and so on.
    pub kind: SyntaxKind,
    pub body: StmtId,
    /// The keyword's span.
    pub span: Span,
}

/// One arm of a generate `if` or `case`.
#[derive(Debug, Clone)]
pub struct GenerateArm {
    /// An `if` arm's condition, none for its `else`; a `case` arm's labels,
    /// empty for its `default`.
    pub conditions: Vec<ExprId>,
    pub body: ScopeId,
}

/// A `for` header, procedural or generate.
#[derive(Debug, Clone)]
pub struct For {
    /// Holds the variables the header declares, and is the body's parent.
    pub scope: ScopeId,
    /// The initialisations that assign rather than declare.
    pub init: Vec<ExprId>,
    pub condition: Option<ExprId>,
    pub step: Vec<ExprId>,
}

/// A name, spelled without the `\` and space of an escaped identifier, which
/// are the same name as the plain one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Name {
    pub text: Box<str>,
    pub span: Span,
}

/// A declared name.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: Name,
    pub kind: SymbolKind,
}

#[derive(Debug, Clone)]
pub enum SymbolKind {
    /// A module, interface, program or package, by its keyword.
    Definition {
        keyword: SyntaxKind,
        scope: ScopeId,
        /// Its ports, in the order of the port list.
        ports: Vec<SymbolId>,
        /// Its parameters, in the order of the parameter port list.
        parameters: Vec<SymbolId>,
        /// Whether an undeclared name may declare a net, as it may where it is
        /// connected to a port or assigned continuously: false under
        /// `` `default_nettype none ``.
        implicit_nets: bool,
    },
    /// A class, whose body is not lowered.
    Class(Opaque),
    Port(Port),
    Parameter(Parameter),
    /// A net: declared with a net type such as `wire`, or a user's nettype.
    Net(Data),
    Variable(Data),
    /// A `genvar`, with its initialiser when a generate loop declares it.
    Genvar(Option<ExprId>),
    Typedef(Type),
    /// A name an enum declares in the scope around it.
    EnumMember {
        value: Option<ExprId>,
    },
    Subroutine {
        keyword: SyntaxKind,
        scope: ScopeId,
        /// Its arguments, in order; ports of its scope.
        ports: Vec<SymbolId>,
        /// A function's return type.
        returns: Option<Type>,
        body: Vec<StmtId>,
    },
    Instance(Instance),
    /// A named block, generate or procedural. It is declared so that a name
    /// finds it; the member or statement holding the block is what reaches
    /// its scope in a walk.
    Block(ScopeId),
    /// A labelled statement that is not a block.
    Label(StmtId),
    /// Anything else with a name and a body not lowered: a modport, a
    /// clocking block, a property, a sequence, a covergroup, a `let`, an
    /// assertion's label, a nettype, a constraint.
    Other(Opaque),
}

/// A port of a definition or an argument of a subroutine.
#[derive(Debug, Clone)]
pub struct Port {
    /// `None` in a non-ANSI port list until the body declares it.
    pub direction: Option<SyntaxKind>,
    /// A net type or `var` written before its type.
    pub keyword: Option<SyntaxKind>,
    pub ty: Type,
    /// A default value, or an ANSI output's initialiser.
    pub default: Option<ExprId>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    /// `localparam` rather than `parameter`, or a parameter a parameter port
    /// list makes local.
    pub local: bool,
    /// A `type` parameter, whose value is a type.
    pub is_type: bool,
    pub ty: Type,
    pub value: Option<ExprId>,
}

/// A net's or a variable's type and initialiser.
#[derive(Debug, Clone)]
pub struct Data {
    pub ty: Type,
    /// For a net, a continuous assignment.
    pub init: Option<ExprId>,
}

/// One instance of a module, interface or program.
#[derive(Debug, Clone)]
pub struct Instance {
    /// What is instantiated.
    pub definition: Name,
    /// Parameter overrides, shared by the instances of one instantiation.
    pub parameters: Vec<Arg>,
    /// An instance array's dimensions.
    pub dims: Vec<Dim>,
    pub connections: Vec<Arg>,
}

/// An argument of a call, a parameter override or a port connection.
#[derive(Debug, Clone)]
pub enum Arg {
    /// Left empty when nothing is written between two commas.
    Positional(Option<ExprId>),
    /// `.name(value)`, `.name()` leaving it unconnected.
    Named { name: Name, value: Option<ExprId> },
    /// `.name`, connecting the name in scope.
    Implicit(Name),
    /// `.*`.
    Wildcard(Span),
}

/// A data type as written. Nothing here is evaluated.
#[derive(Debug, Clone)]
pub struct Type {
    pub kind: TypeKind,
    /// Packed dimensions, and a declarator's unpacked ones after them.
    pub dims: Vec<Dim>,
}

#[derive(Debug, Clone)]
pub enum TypeKind {
    /// No type written: `input a`, `wire signed [3:0] a`.
    Implicit {
        signing: Option<SyntaxKind>,
    },
    /// A keyword type, `logic` or `int`, with `signed` or `unsigned` if
    /// written.
    Builtin {
        keyword: SyntaxKind,
        signing: Option<SyntaxKind>,
    },
    /// A typedef, class or interface, by a name expression: `t`, `pkg::t`,
    /// `intf.mp`, `C#(8)`.
    Named(ExprId),
    Enum {
        base: Box<Type>,
        /// Declared in the scope the enum is written in.
        members: Vec<SymbolId>,
    },
    Struct {
        keyword: SyntaxKind,
        fields: Vec<Field>,
    },
    /// `type(expr)`.
    TypeOf(ExprId),
    Opaque(Opaque),
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: Name,
    pub ty: Type,
}

/// One dimension, packed or unpacked.
#[derive(Debug, Clone)]
pub enum Dim {
    /// `[hi:lo]`.
    Range(ExprId, ExprId),
    /// `[n]`, `[$]`, or an associative array's `[T]`.
    Single(ExprId),
    /// `[]` or `[*]`.
    Empty,
}

/// A statement.
#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    /// A lone `;`.
    Empty,
    /// An assignment, a call, an increment.
    Expr(ExprId),
    /// A `begin` or `fork` block; a `fork`'s is its closing keyword.
    Block {
        scope: ScopeId,
        join: Option<SyntaxKind>,
        stmts: Vec<StmtId>,
    },
    If {
        condition: ExprId,
        then_branch: Option<StmtId>,
        else_branch: Option<StmtId>,
    },
    Case {
        /// `CASE_KW`, `CASEZ_KW`, `CASEX_KW` or `RANDCASE_KW`.
        keyword: SyntaxKind,
        selector: Option<ExprId>,
        items: Vec<CaseItem>,
    },
    For {
        header: For,
        body: Option<StmtId>,
    },
    Foreach {
        /// Holds the loop variables.
        scope: ScopeId,
        array: ExprId,
        body: Option<StmtId>,
    },
    /// `while`, `do … while`, `repeat` or `forever`, by keyword; `forever`
    /// has no condition.
    Loop {
        keyword: SyntaxKind,
        condition: Option<ExprId>,
        body: Option<StmtId>,
    },
    Timed {
        timing: Timing,
        body: Option<StmtId>,
    },
    /// `wait (cond) stmt`, or `wait fork` with neither.
    Wait {
        condition: Option<ExprId>,
        body: Option<StmtId>,
    },
    /// `disable name`, or `disable fork` with none.
    Disable(Option<ExprId>),
    Return(Option<ExprId>),
    Break,
    Continue,
    /// `-> ev` or `->> ev`.
    Trigger(ExprId),
    /// `assign`, `deassign`, `force` or `release`, by keyword.
    ProceduralAssign {
        keyword: SyntaxKind,
        expr: ExprId,
    },
    /// An immediate `assert`, `assume` or `cover`, by keyword.
    Assertion {
        keyword: SyntaxKind,
        condition: ExprId,
        pass: Option<StmtId>,
        fail: Option<StmtId>,
    },
    Opaque(Opaque),
}

#[derive(Debug, Clone)]
pub struct CaseItem {
    /// Empty for `default`.
    pub labels: Vec<ExprId>,
    pub body: Option<StmtId>,
}

/// An event or delay control.
#[derive(Debug, Clone)]
pub enum Timing {
    /// `#d` or `##n`, by its operator.
    Delay(SyntaxKind, ExprId),
    /// `@(a or posedge b)`, `@ev`.
    Events(Vec<Event>),
    /// `@*` or `@(*)`.
    Star,
    /// `repeat (n) @(…)`.
    Repeat(ExprId, Vec<Event>),
}

#[derive(Debug, Clone)]
pub struct Event {
    /// `POSEDGE_KW`, `NEGEDGE_KW` or `EDGE_KW`.
    pub edge: Option<SyntaxKind>,
    pub expr: ExprId,
    pub iff: Option<ExprId>,
}

/// An expression.
#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Name(Box<str>),
    /// `$bits`, `$display`: a call when it is a [`Call`](ExprKind::Call)'s
    /// callee.
    System(Box<str>),
    /// `this`, `super`, `null`, `$`, `default`, `new`, and a keyword type
    /// written where an expression is, as in `int'(x)`.
    Keyword(SyntaxKind),
    /// A number, string or time, as spelled.
    Literal(Box<str>),
    /// By its operator's kind; `++` and `--` write their operand.
    Unary {
        op: SyntaxKind,
        operand: ExprId,
    },
    Postfix {
        op: SyntaxKind,
        operand: ExprId,
    },
    Binary {
        op: SyntaxKind,
        lhs: ExprId,
        rhs: ExprId,
    },
    Conditional {
        condition: ExprId,
        then_value: ExprId,
        else_value: ExprId,
    },
    /// `a[i]`, or `a[hi:lo]`, `a[i+:w]`, `a[i-:w]` by the operator.
    Select {
        base: ExprId,
        index: ExprId,
        range: Option<(SyntaxKind, ExprId)>,
    },
    /// `a.b`: a member, or the next step of a hierarchical name.
    Member {
        base: ExprId,
        name: Name,
    },
    /// `a::b`.
    Scoped {
        base: ExprId,
        name: Name,
    },
    Call {
        callee: ExprId,
        args: Vec<Arg>,
    },
    Concat(Vec<ExprId>),
    Replication {
        count: ExprId,
        concat: ExprId,
    },
    Stream {
        op: SyntaxKind,
        slice: Option<ExprId>,
        concat: ExprId,
    },
    /// `'{…}`, or `T'{…}` with its type.
    Pattern {
        ty: Option<ExprId>,
        items: Vec<PatternItem>,
    },
    /// `T'(x)`, where `T` is a type, a width or a signing.
    Cast {
        ty: ExprId,
        operand: ExprId,
    },
    /// `x inside {…}`: each value, and each `[lo:hi]` as its two bounds.
    Inside {
        expr: ExprId,
        set: Vec<ExprId>,
    },
    /// An assignment: in a statement, a continuous assignment, or used as a
    /// value. `<=` is a nonblocking one.
    Assign {
        op: SyntaxKind,
        lhs: ExprId,
        rhs: ExprId,
        timing: Option<Timing>,
    },
    /// `tagged Member value`.
    Tagged {
        member: Name,
        value: Option<ExprId>,
    },
    /// A data type where an expression may stand: an argument, a parameter
    /// value.
    Type(Box<Type>),
    Opaque(Opaque),
}

#[derive(Debug, Clone)]
pub struct PatternItem {
    /// A member name, index, type or `default`; `None` for a positional
    /// item. A member name resolves to nothing in scope.
    pub key: Option<ExprId>,
    pub value: ExprId,
}

/// A region not lowered: a `VERBATIM`, a class body, a construct not
/// modelled yet, or, with no names, an `` `include `` not followed. What it
/// means is unknown, so each name it spells may be a use of anything by that
/// name, and an analysis must not conclude from its absence.
#[derive(Debug, Clone, Default)]
pub struct Opaque {
    pub names: Vec<Name>,
}
