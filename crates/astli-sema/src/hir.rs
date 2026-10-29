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
    /// `foreach`'s, a generate loop's `genvar`; or a `with`'s iterator.
    Loop,
    /// A property's or a sequence's formals and local variables; a `let`'s
    /// formals.
    Property,
    /// What a clocking block declares besides its signals: a property, a
    /// sequence.
    Clocking,
    /// A covergroup's formals and `sample`'s arguments; or, within it, its
    /// cover points and crosses.
    Covergroup,
    /// A class's parameters and members, and the methods and constraints
    /// defined outside it.
    Class,
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
    Bind(Bind),
    /// `default disable iff`, the reset of the assertions that name none.
    DefaultDisable(ExprId),
    /// `default clocking cb;`, naming a clocking block declared apart as the
    /// clock of the assertions that name none.
    DefaultClocking(ExprId),
    Opaque(Opaque),
}

/// `bind`: instances it adds at the end of a definition's scope, whose
/// overrides and connections are names there.
#[derive(Debug, Clone)]
pub struct Bind {
    /// The definition bound into; `None` when a path names the one instance
    /// bound into, which only the tree can tell the definition of.
    pub target: Option<Name>,
    /// Whether it binds into the instances it chooses, by a path or a list
    /// after `:`, rather than into every instance of `target`.
    pub chosen: bool,
    /// The instances it adds, declared in no scope.
    pub instances: Vec<SymbolId>,
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

/// An `always`, `initial` or `final` block; or an assertion standing among
/// items, concurrent or deferred, which runs as one does, its body the
/// assertion.
#[derive(Debug, Clone)]
pub struct Process {
    /// Its keyword: `ALWAYS_FF_KW`, `ASSERT_KW` and so on.
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
        /// Whether `ports` is all of them, which it is not when the port list
        /// holds something opaque.
        ports_known: bool,
        /// Its parameters, in the order of the parameter port list, or the
        /// `parameter`s of its body when it has none.
        parameters: Vec<SymbolId>,
        /// Whether `parameters` is all of them.
        parameters_known: bool,
        /// Whether an undeclared name may declare a net, as it may where it is
        /// connected to a port or assigned continuously: false under
        /// `` `default_nettype none ``.
        implicit_nets: bool,
    },
    Class(Class),
    Port(Port),
    Parameter(Parameter),
    /// A net: declared with a net type such as `wire`, or a user's nettype.
    Net(Data),
    Variable(Data),
    /// A `genvar`, with its initialiser when a generate loop declares it.
    Genvar(Option<ExprId>),
    Typedef(Type),
    /// A forward typedef, `typedef class c;`, which the declaration it
    /// announces stands in for where its scope has one.
    Forward,
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
        /// Whether it is declared without a body: a DPI import, an
        /// `extern` or `pure virtual` prototype.
        prototype: bool,
    },
    Instance(Instance),
    /// A named block, generate or procedural. It is declared so that a name
    /// finds it; the member or statement holding the block is what reaches
    /// its scope in a walk.
    Block(ScopeId),
    /// A labelled statement that is not a block, such as an assertion.
    Label(StmtId),
    /// A `property` or a `sequence`, by its keyword: a property over its
    /// formals, which an instance of it substitutes, as a call does
    /// arguments.
    Property {
        keyword: SyntaxKind,
        scope: ScopeId,
        /// Its formals, in order; ports of its scope.
        ports: Vec<SymbolId>,
        spec: PropertySpec,
    },
    /// A constraint of a class: a block of them, each an expression it
    /// reads. `None` for a prototype, or one whose body is not lowered.
    Constraint(Option<StmtId>),
    /// A `let`: an expression over its formals, which a call of it
    /// substitutes.
    Let {
        scope: ScopeId,
        /// Its formals, in order; ports of its scope.
        ports: Vec<SymbolId>,
        expr: ExprId,
    },
    /// A view of the interface declaring it, which a port of the
    /// interface's type may name: what it lists, in order.
    Modport(Vec<ModportItem>),
    Clocking(Clocking),
    Covergroup(Covergroup),
    /// A cover point or a cross, named by its label. Unlabelled, a cover
    /// point of a variable alone is named by it, and any other has no name.
    Coverpoint(Coverpoint),
    /// What a `with` names each element it runs over: `item`, unless the
    /// call names it.
    Iterator,
    /// Anything else with a name and a body not lowered: a nettype, a
    /// configuration.
    Other(Opaque),
}

/// A class: a type whose objects hold its properties and run its methods,
/// and those of the class it extends.
#[derive(Debug, Clone)]
pub struct Class {
    /// Holds its parameters, its members, and those defined outside it.
    pub scope: ScopeId,
    /// Its parameter port list's parameters, in order.
    pub parameters: Vec<SymbolId>,
    /// The class it extends, a name with its parameters, `B #(T)`; and
    /// the arguments `extends B(…)` gives its constructor.
    pub extends: Option<(Type, Vec<Arg>)>,
    /// The interface classes it implements.
    pub implements: Vec<Type>,
}

/// A covergroup, a type whose instances sample its cover points and crosses
/// on its event, or each time `sample` is called.
#[derive(Debug, Clone)]
pub struct Covergroup {
    /// Holds its formals and `sample`'s arguments.
    pub scope: ScopeId,
    /// Its formals, then `sample`'s arguments; ports of `scope`.
    pub ports: Vec<SymbolId>,
    /// `@(…)`, if written.
    pub event: Option<Timing>,
    /// Holds its cover points and crosses, within `scope`. A cover point's
    /// expression is a name in `scope`, so that its label does not hide the
    /// variable it samples; a cross's, and the bins of either, in `points`,
    /// where a cover point is found by its label.
    pub points: ScopeId,
    pub options: Vec<CoverOption>,
}

#[derive(Debug, Clone)]
pub struct Coverpoint {
    /// `COVERPOINT_KW` or `CROSS_KW`.
    pub keyword: SyntaxKind,
    /// A typed cover point's type.
    pub ty: Option<Type>,
    /// What a cover point samples; the cover points a cross crosses.
    pub exprs: Vec<ExprId>,
    /// `iff (…)`.
    pub iff: Option<ExprId>,
    pub bins: Vec<Bins>,
    pub options: Vec<CoverOption>,
}

/// `bins`, `illegal_bins` or `ignore_bins`, by keyword.
#[derive(Debug, Clone)]
pub struct Bins {
    pub keyword: SyntaxKind,
    pub name: Name,
    /// The number of bins in `[n]`; the values and transitions they count,
    /// or the cover point or the selection of a cross's bins they take; and
    /// how many of those `matches` asks for.
    pub values: Vec<ExprId>,
    /// `with (…)` after the values, keeping each value it holds for.
    pub with: Option<With>,
    /// `iff (…)`.
    pub iff: Option<ExprId>,
}

/// `option.name = value`, or `type_option.name = value`.
#[derive(Debug, Clone)]
pub struct CoverOption {
    /// Whether it is a `type_option`, which all instances share.
    pub shared: bool,
    pub name: Name,
    pub value: ExprId,
}

/// `with (…)`: an expression over each element, which `scope` declares as
/// an [`SymbolKind::Iterator`].
#[derive(Debug, Clone)]
pub struct With {
    pub scope: ScopeId,
    pub expr: ExprId,
}

/// One entry of a modport: a signal its users read or drive, by direction;
/// or a clocking block, by `CLOCKING_KW`.
#[derive(Debug, Clone)]
pub struct ModportItem {
    pub direction: SyntaxKind,
    /// The name a user of the modport accesses it by.
    pub name: Name,
    /// What the name stands for: the signal of that name, or the expression
    /// of `.name(expr)`.
    pub value: ExprId,
}

/// A clocking block, which samples its inputs and drives its outputs on its
/// event. A procedure reaches a signal through it as `cb.name`.
#[derive(Debug, Clone)]
pub struct Clocking {
    /// `DEFAULT_KW` or `GLOBAL_KW`, if written.
    pub qualifier: Option<SyntaxKind>,
    pub event: Timing,
    /// Holds what it declares besides its signals.
    pub scope: ScopeId,
    pub signals: Vec<ClockingSignal>,
    /// The skews its items write, `#2`, each for its signals or by default.
    pub skews: Vec<Timing>,
}

/// One signal of a clocking block: `input a`, `output b`, or `inout c`, which
/// is also what `input output` makes it.
#[derive(Debug, Clone)]
pub struct ClockingSignal {
    pub direction: SyntaxKind,
    /// The name a procedure reaches it by, `cb.name`.
    pub name: Name,
    /// What it samples or drives, in the scope around the clocking block:
    /// the signal of that name, or the expression of `name = expr`.
    pub signal: ExprId,
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
    /// A net's net type, `wire` or `supply0`; `var` for a variable written
    /// with it.
    pub keyword: Option<SyntaxKind>,
    pub ty: Type,
    /// A net's delay, `wire #2 w`.
    pub delay: Option<Timing>,
    /// For a net, a continuous assignment.
    pub init: Option<ExprId>,
}

/// One instance of a module, interface, program, checker or UDP, or of a
/// gate or a switch. An instance of a gate, a switch or a UDP may be left
/// unnamed, and is declared with an empty name, which no name finds.
#[derive(Debug, Clone)]
pub struct Instance {
    /// What is instantiated: a definition's name, or a gate's keyword.
    pub definition: Name,
    /// A gate's or a switch's keyword, `AND_KW` and so on, which names no
    /// definition.
    pub gate: Option<SyntaxKind>,
    /// Parameter overrides, shared by the instances of one instantiation;
    /// a UDP's delay in parentheses, which reads as them.
    pub parameters: Vec<Arg>,
    /// A gate's or a UDP's delay, `#2`.
    pub delay: Option<Timing>,
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
    /// How many of `dims` are packed: those written with the type.
    pub packed: usize,
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
    /// A struct or union, by its keyword.
    Struct {
        keyword: SyntaxKind,
        packed: bool,
        /// `signed` or `unsigned`, if written; only a packed one may be.
        signing: Option<SyntaxKind>,
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
    /// An assignment, a call, an increment; or a constraint.
    Expr(ExprId),
    /// A `begin` or `fork` block, a `fork`'s with its closing keyword; or
    /// a block of constraints.
    Block {
        scope: ScopeId,
        join: Option<SyntaxKind>,
        stmts: Vec<StmtId>,
    },
    /// Also a constraint's implication, `a -> b`, with no `else`.
    If {
        condition: ExprId,
        then_branch: Option<StmtId>,
        else_branch: Option<StmtId>,
    },
    Case {
        /// `CASE_KW`, `CASEZ_KW`, `CASEX_KW` or `RANDCASE_KW`.
        keyword: SyntaxKind,
        /// `case … inside`, whose labels are a set as `inside`'s.
        inside: bool,
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
    /// A concurrent `assert`, `assume`, `cover`, `restrict` or `expect`, by
    /// keyword.
    ConcurrentAssertion {
        keyword: SyntaxKind,
        spec: PropertySpec,
        pass: Option<StmtId>,
        fail: Option<StmtId>,
    },
    Opaque(Opaque),
}

/// What a concurrent assertion checks, or a property or sequence declares:
/// a property, the clock it is sampled on and the condition that resets it.
#[derive(Debug, Clone)]
pub struct PropertySpec {
    pub clock: Option<Timing>,
    /// `disable iff (…)`.
    pub disable: Option<ExprId>,
    pub property: ExprId,
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
    /// `#d` or `##n`, by its operator; `#(rise, fall, off)` gives each.
    Delay(SyntaxKind, Vec<ExprId>),
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
    /// A number or a string, as spelled.
    Literal(Box<str>),
    /// A time, as spelled, and its value in the time unit where it is
    /// written, rounded to the precision there; `None` if malformed.
    Time {
        text: Box<str>,
        value: Option<f64>,
    },
    /// By its operator's kind; `++` and `--` write their operand, and
    /// `new` copies the object it holds.
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
    /// `x inside {…}`: each value, or a [`Range`](ExprKind::Range).
    Inside {
        expr: ExprId,
        set: Vec<ExprId>,
    },
    /// `[lo:hi]` in the set of `inside` or `case … inside`, or by its
    /// operator `[a +/- d]` and `[a +%- d]`; a bound `$` is
    /// `Keyword(DOLLAR)`.
    Range {
        op: SyntaxKind,
        lo: ExprId,
        hi: ExprId,
    },
    /// An assignment: in a statement, a continuous assignment, or used as a
    /// value. `<=` is a nonblocking one.
    Assign {
        op: SyntaxKind,
        lhs: ExprId,
        rhs: ExprId,
        timing: Option<Timing>,
    },
    /// `a.sum() with (item * 2)`, an array method's expression for each
    /// element; or `cp with (item > 2)`, a cover point's values a bin
    /// keeps.
    With {
        /// The call or the cover point.
        base: ExprId,
        with: With,
    },
    /// `std::randomize(x) with {…}`: a call, and the constraints it solves
    /// under, a block of them.
    Constrained {
        call: ExprId,
        constraints: StmtId,
    },
    /// `x dist {…}`: each value, each `[lo:hi]`'s two bounds, and each
    /// weight.
    Dist {
        expr: ExprId,
        set: Vec<ExprId>,
    },
    /// `binsof(cp.b) intersect {…}`: in a cross, the bins of a cover point,
    /// or one bin of it, whose values meet each value and each `[lo:hi]`'s
    /// two bounds.
    Binsof {
        bins: ExprId,
        intersect: Vec<ExprId>,
    },
    /// `tagged Member value`.
    Tagged {
        member: Name,
        value: Option<ExprId>,
    },
    /// A data type where an expression may stand: an argument, a parameter
    /// value.
    Type(Box<Type>),
    /// A sequence or a property an operator builds. An expression is a
    /// sequence too, so one stands alone where a property may.
    Property(Property),
    Opaque(Opaque),
}

/// A sequence or a property an operator builds, which has no value: what it
/// matches is not evaluated.
#[derive(Debug, Clone)]
pub enum Property {
    /// `a |-> b`, `a and b`, `a throughout b`, by its operator.
    Binary {
        op: SyntaxKind,
        lhs: ExprId,
        rhs: ExprId,
    },
    /// `not p`, `always [2:$] p`, `accept_on (c) p`, by its operator, with
    /// the range or the condition it takes.
    Unary {
        op: SyntaxKind,
        bounds: Option<Bounds>,
        condition: Option<ExprId>,
        operand: ExprId,
    },
    /// `a ##[1:3] b`, or `##1 b`, which starts with the delay. `##[*]` and
    /// `##[+]` have no bounds, and `STAR` or `PLUS` for `op`.
    Delay {
        lhs: Option<ExprId>,
        op: Option<SyntaxKind>,
        bounds: Option<Bounds>,
        rhs: ExprId,
    },
    /// `s [*2]`, `s [->1:3]`, by the operator after `[`.
    Repetition {
        op: SyntaxKind,
        operand: ExprId,
        bounds: Option<Bounds>,
    },
    /// `(s, x = a, n++)`: a sequence, and what each of its matches assigns.
    Match {
        sequence: ExprId,
        items: Vec<ExprId>,
    },
    If {
        condition: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
    },
    /// `@(posedge clk) p`.
    Clocked { clock: Timing, property: ExprId },
}

impl Property {
    /// The expressions it is built from, in the order written, but for a
    /// clock's.
    pub fn operands(&self) -> Vec<ExprId> {
        let bounds = |bounds: Option<Bounds>| {
            (bounds.into_iter())
                .flat_map(|bounds| [Some(bounds.low), bounds.high])
                .flatten()
        };
        match self {
            Property::Binary { lhs, rhs, .. } => vec![*lhs, *rhs],
            Property::Unary {
                bounds: range,
                condition,
                operand,
                ..
            } => bounds(*range).chain(*condition).chain([*operand]).collect(),
            Property::Delay {
                lhs,
                bounds: range,
                rhs,
                ..
            } => lhs
                .iter()
                .copied()
                .chain(bounds(*range))
                .chain([*rhs])
                .collect(),
            Property::Repetition {
                operand,
                bounds: range,
                ..
            } => [*operand].into_iter().chain(bounds(*range)).collect(),
            Property::Match { sequence, items } => [*sequence]
                .into_iter()
                .chain(items.iter().copied())
                .collect(),
            Property::If {
                condition,
                then_branch,
                else_branch,
            } => [*condition, *then_branch]
                .into_iter()
                .chain(*else_branch)
                .collect(),
            Property::Clocked { property, .. } => vec![*property],
        }
    }
}

/// How many cycles a delay waits, or how often a repetition repeats: `[2]`,
/// `[1:3]`, or `[1:$]`, its `$` a [`ExprKind::Keyword`]. Where `[*]` or
/// `[+]` stands for a range, there are none.
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub low: ExprId,
    pub high: Option<ExprId>,
}

#[derive(Debug, Clone)]
pub struct PatternItem {
    /// A member name, index, type or `default`; `None` for a positional
    /// item. A member name resolves to nothing in scope.
    pub key: Option<ExprId>,
    pub value: ExprId,
}

/// A region not lowered: a `VERBATIM`, a construct not modelled yet, or, with no names, an `` `include `` not followed. What it
/// means is unknown, so each name it spells may be a use of anything by that
/// name, and an analysis must not conclude from its absence.
#[derive(Debug, Clone, Default)]
pub struct Opaque {
    pub names: Vec<Name>,
}
