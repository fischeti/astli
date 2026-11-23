//! Elaboration: what a design means once its parameters have values.
//!
//! Each answer is a query computed when first asked and kept: a parameter's
//! value, a declaration's type. A query that asks for itself, as a parameter
//! defined by its own value does, is unknown rather than a loop. No pass
//! order would do, since a parameter can call a package's function that
//! takes `$bits` of a type whose width is another parameter.
//!
//! Values are computed in a **body**: a package, a definition with its
//! parameters at their defaults, or a file's top level, its `$unit`. A name
//! declared in another package or at the top of a file is computed in that
//! one's body.
//!
//! An expression is sized as the standard sizes it: each operand has a type
//! of its own, an operator either takes the operand's or extends it to the
//! width of the expression around it, and an assignment extends to the
//! wider of its sides, then truncates. A constant function runs with
//! variables of its own, under a limit on the statements it may take.
//!
//! What sema cannot evaluate is [`Value::Unknown`] and absorbs whatever is
//! computed from it: a name it does not model, a construct left opaque, a
//! cycle, a function that runs too long.

mod call;
mod expr;
mod format;
mod pattern;
mod select;
mod shared;
mod top;
mod tree;

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::sync::{Arc, OnceLock};

use astli_syntax::SyntaxKind::{self, *};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::bind::{self, Bound};
use crate::design::{Design, FileId, SymbolRef};
use crate::hir::*;
use crate::resolve::{Names, Resolution};
use crate::types::{ArrayTy, ClassTy, DynamicTy, EnumTy, MemberTy, Packed, Shape, StructTy, Ty};
use crate::value::{Bits, Literal, Value};
pub use expr::Operator;
use expr::{four_state, literal_type};
use shared::{Arena, Memo, Shards};
pub use top::{TopError, TopSetting, TopValue};
pub use tree::{Child, ChildKind};
use tree::{Deferred, Keyed};

/// Where values are computed: a package, a definition with its parameters
/// decided, a file's top level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BodyId(u32);

impl BodyId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone)]
struct Body {
    file: FileId,
    /// The scope it gives values to: a definition's, a generate block's, or
    /// the file's.
    scope: ScopeId,
    /// The body a generate block is built in; `None` for a definition's or
    /// a file's, which see nothing of where they are instantiated.
    parent: Option<BodyId>,
    /// What is decided outside the body: an instance's overrides, a loop
    /// iteration's genvar.
    given: FxHashMap<SymbolId, Given>,
    /// The `defparam`s and chosen `bind`s a body above passes down through
    /// this one; with those written in it, they are its `deferred`.
    passed: Vec<Deferred>,
}

/// A value a body is given.
#[derive(Debug, Clone)]
enum Given {
    /// A parameter's override, written in `file` and evaluated in `body`:
    /// where its instance stands, which a bound instance's file is not.
    Override {
        body: BodyId,
        file: FileId,
        expr: ExprId,
    },
    /// A top's parameter set from outside the design, as a simulator's
    /// `-G` sets one.
    Literal(Literal),
    /// A genvar's value in one iteration of its loop.
    Genvar(Value),
    /// The body of the interface instance an interface port is connected
    /// to.
    Interface(BodyId),
}

/// What is known of one file, worked out when first needed.
#[derive(Default)]
struct FileFacts {
    /// The scopes holding a `defparam` or a chosen `bind`, and those around
    /// them.
    defers: OnceLock<FxHashSet<ScopeId>>,
}

/// A design elaborated on demand: the values of its constants and the types
/// of its declarations.
///
/// Queries take `&self` and keep their answers inside, so one elaboration
/// serves a whole design, and threads share it. A query asks others as it
/// goes, and one asked again while it runs is a cycle, which only the
/// thread running it can tell from another thread's asking; threads asking
/// the same thing at once both compute it, and none waits for another's
/// query (S33 in `docs/sema.md`).
pub struct Elaboration {
    design: Arc<Design>,
    /// The instances bound into every instance of each definition.
    binds: FxHashMap<SymbolRef, Vec<tree::Added>>,
    /// The definitions elaborated, whose `bind`s apply; `None` for every
    /// one.
    elaborated: Option<FxHashSet<SymbolRef>>,
    facts: Vec<FileFacts>,
    bodies: Arena<Body>,
    /// The body of each package, each definition at its defaults, and each
    /// file's top level.
    defaults: FxHashMap<(FileId, ScopeId), BodyId>,
    values: Memo<(BodyId, SymbolId), Value>,
    types: Memo<(BodyId, SymbolId), Ty>,
    /// The bodies instances of each definition have been given, by the
    /// values of its parameters and the interfaces its ports are connected
    /// to.
    instances: Shards<SymbolRef, Vec<Keyed>>,
    /// The definition and body of each instance a port's connection or a
    /// hierarchical name reaches, by the body it is built in; `None` for an
    /// instance of a gate or of a definition no file declares.
    held: Memo<(BodyId, SymbolRef), Option<(SymbolRef, BodyId)>>,
    /// What each body builds. A generate condition naming a block beside
    /// it asks for the body's children while they are being built: that
    /// cycle finds none.
    children: Memo<BodyId, Arc<[Child]>>,
    /// The `defparam`s and chosen `bind`s on their way down through each
    /// body.
    deferred: Memo<BodyId, Arc<[Deferred]>>,
    /// The `defparam`s and chosen `bind`s whose path starts at a top, by
    /// the top's definition.
    absolute: OnceLock<FxHashMap<SymbolRef, Vec<Deferred>>>,
    /// The body of each top that such a path reaches into, or that has
    /// parameters set, by its definition and the text each is set to.
    tops: Shards<(SymbolRef, Vec<(SymbolId, String)>), BodyId>,
}

/// Where an expression is evaluated: in a body, and in the run of a
/// constant function if one is running.
#[derive(Clone, Copy)]
pub(crate) struct Cx<'a> {
    body: BodyId,
    file: FileId,
    run: Option<&'a Run>,
    /// What a walk of this body has typed and evaluated, outside any
    /// function's run.
    kept: Option<&'a Kept>,
    /// The statements the outermost constant function running may still
    /// take, those it calls included. A query asked during a run starts
    /// outside it, with a budget of its own, so its value is the same
    /// whoever asks it first.
    steps: Option<&'a Cell<u32>>,
}

impl Cx<'_> {
    /// Where an expression written in `file` is evaluated in `body`,
    /// outside a constant function's run and a walk.
    pub(crate) fn new(body: BodyId, file: FileId) -> Cx<'static> {
        Cx {
            body,
            file,
            run: None,
            kept: None,
            steps: None,
        }
    }

    pub(crate) fn file(&self) -> FileId {
        self.file
    }
}

/// The generate blocks one walk of the tree has entered, up to
/// [`Elaboration::BLOCKS`]. A walk that goes through a block once for each
/// path to it is charged for it once, so a walk of the tree and one of its
/// bodies agree on where they stop.
#[derive(Debug, Default)]
pub struct Blocks {
    entered: FxHashSet<BodyId>,
}

impl Blocks {
    /// Whether the walk may enter the generate block `body`: one it entered
    /// before, or one more while it has entered fewer than
    /// [`Elaboration::BLOCKS`].
    pub fn enter(&mut self, body: BodyId) -> bool {
        if self.entered.contains(&body) {
            return true;
        }
        self.entered.len() < Elaboration::BLOCKS && self.entered.insert(body)
    }

    /// Whether the walk entered the generate block `body`.
    pub fn entered(&self, body: BodyId) -> bool {
        self.entered.contains(&body)
    }
}

/// Each expression's own type and value in one body, as a walk of it finds
/// them. An expression's type is built from its operands', so a walk asking
/// at every level of a deep expression would otherwise type it again at
/// each.
#[derive(Default)]
pub(crate) struct Kept {
    types: RefCell<FxHashMap<ExprId, Ty>>,
    values: RefCell<FxHashMap<ExprId, Value>>,
}

/// What a walk of one body asks of an elaboration, each expression typed
/// and evaluated once however often it is asked.
pub struct Walked<'a> {
    elaboration: &'a Elaboration,
    body: BodyId,
    file: FileId,
    kept: Kept,
}

impl Walked<'_> {
    fn cx(&self) -> Cx<'_> {
        Cx {
            body: self.body,
            file: self.file,
            run: None,
            kept: Some(&self.kept),
            steps: None,
        }
    }

    /// [`Elaboration::expr_type`] in this body.
    pub fn expr_type(&self, expr: ExprId) -> Ty {
        self.elaboration.self_type(self.cx(), expr)
    }

    /// [`Elaboration::eval`] in this body.
    pub fn eval(&self, expr: ExprId) -> Value {
        self.elaboration.eval_self(self.cx(), expr)
    }

    /// Which way `condition` goes, where it is a constant: `None` where it
    /// is not, or holds an `x`.
    pub fn taken(&self, condition: ExprId) -> Option<bool> {
        match self.eval(condition) {
            Value::Bits(bits) => match bits.truth() {
                crate::value::Bit::One => Some(true),
                crate::value::Bit::Zero => Some(false),
                _ => None,
            },
            Value::Real(real) => Some(real != 0.0),
            _ => None,
        }
    }
}

/// A run of statements with variables of their own: a constant function's,
/// with its arguments and return value, or a generate loop's header, with
/// its genvar.
#[derive(Default)]
pub(crate) struct Run {
    /// The function run, whose name holds its return value; `None` for a
    /// loop's header, which returns nothing.
    function: Option<SymbolId>,
    locals: RefCell<FxHashMap<SymbolId, Value>>,
}

// Threads share an elaboration, as an editor's requests or a linter's
// files do.
const _: fn() = || {
    fn shared<T: Send + Sync>() {}
    shared::<Elaboration>();
};

impl Elaboration {
    /// How deep a walk of the instance tree goes, each instance and each
    /// generate block a level: a definition that instantiates itself with
    /// a new parameter each time makes the tree endless. Every walk stops
    /// here, so what a check sees is what the tree shows.
    pub const DEPTH: usize = 128;

    /// How many generate blocks one walk of the tree enters, past which it
    /// takes each other block for undecided and goes no deeper there: a
    /// design's tree many times over, and a bound on what loops nested in
    /// loops can cost. A body's children are built for whoever asks first
    /// and kept, so the bound is the walk's, as [`Blocks`] counts it, and
    /// what a walk enters depends on the walk alone.
    pub const BLOCKS: usize = 1 << 22;

    /// An elaboration of `design` in which every `bind` applies, as though
    /// each definition were elaborated.
    pub fn new(design: Arc<Design>) -> Elaboration {
        // Every body at its defaults is made here, so that finding one takes
        // no lock: a file's top level, and each definition's and package's
        // scope.
        let bodies = Arena::default();
        let mut defaults = FxHashMap::default();
        for (file, hir) in design.files() {
            for (scope, held) in hir.scopes() {
                if matches!(held.kind, ScopeKind::File | ScopeKind::Definition) {
                    let body = bodies.push(Body {
                        file,
                        scope,
                        parent: None,
                        given: FxHashMap::default(),
                        passed: Vec::new(),
                    });
                    defaults.insert((file, scope), BodyId(body as u32));
                }
            }
        }
        Elaboration {
            facts: design.files().map(|_| FileFacts::default()).collect(),
            binds: tree::binds(&design),
            design,
            elaborated: None,
            bodies,
            defaults,
            values: Memo::default(),
            types: Memo::default(),
            instances: Shards::default(),
            held: Memo::default(),
            children: Memo::default(),
            deferred: Memo::default(),
            absolute: OnceLock::new(),
            tops: Shards::default(),
        }
    }

    /// An elaboration of `design` from `tops`, in which a `bind` a
    /// definition holds applies only if the tops reach that definition, as
    /// only what is elaborated binds.
    pub fn with_tops(design: Arc<Design>, tops: &[impl AsRef<str>]) -> Elaboration {
        let mut elaboration = Elaboration::new(design);
        elaboration.elaborated = Some(elaboration.reached_from(tops));
        elaboration
    }

    pub fn design(&self) -> &Design {
        &self.design
    }

    /// The body of a package, or of a module, interface or program with its
    /// parameters at their defaults; `None` for any other symbol.
    pub fn body(&self, definition: SymbolRef) -> Option<BodyId> {
        match self.design.symbol(definition).kind {
            SymbolKind::Definition { scope, .. } => Some(self.default_body(definition.file, scope)),
            _ => None,
        }
    }

    /// The value of a parameter or enum member of `body`'s, declared in it
    /// or in a generate block inside it; unknown for anything else.
    pub fn value(&self, body: BodyId, symbol: SymbolId) -> Value {
        let cx = self.cx(body);
        let at = SymbolRef {
            file: cx.file,
            symbol,
        };
        if self.owner(cx, at) != Some(body) {
            return Value::Unknown;
        }
        self.values.get((body, symbol), Value::Unknown, || {
            self.compute_value(cx, symbol)
        })
    }

    /// The type of a declaration of `body`'s: a parameter's, a typedef's, an
    /// enum member's, a net's, a variable's, a function's return type.
    pub fn type_of(&self, body: BodyId, symbol: SymbolId) -> Ty {
        let cx = self.cx(body);
        self.types.get((body, symbol), Ty::Unknown, || {
            self.compute_type(cx, symbol)
        })
    }

    /// The type `expr`, written in `body`, has by itself, before the
    /// expression around it sizes it.
    pub fn expr_type(&self, body: BodyId, expr: ExprId) -> Ty {
        self.self_type(self.cx(body), expr)
    }

    /// The value of `expr`, written in `body`, at the type it has by
    /// itself.
    pub fn eval(&self, body: BodyId, expr: ExprId) -> Value {
        self.eval_self(self.cx(body), expr)
    }

    /// `body`, for a walk that asks the type and value of many of its
    /// expressions and of the expressions they are built of.
    pub fn walk(&self, body: BodyId) -> Walked<'_> {
        Walked {
            elaboration: self,
            body,
            file: self.scope(body).0,
            kept: Kept::default(),
        }
    }

    /// What the arguments of an elaboration system task, written in `body`,
    /// say: a string, as `$sformatf` formats it with the rest. `None` where
    /// the first is no string, or sema cannot tell what they make.
    pub(crate) fn message(&self, body: BodyId, args: &[Arg]) -> Option<String> {
        let cx = self.cx(body);
        let Some(Arg::Positional(Some(first))) = args.first() else {
            return None;
        };
        let hir = &self.design[cx.file];
        if !hir[*first].kind.is_string() && self.self_type(cx, *first) != Ty::String {
            return None;
        }
        match self.formatted(cx, args) {
            Value::String(text) => Some(String::from_utf8_lossy(&text).into_owned()),
            _ => None,
        }
    }

    /// The type of `at` as `body` sees it: a declaration of the body's, of
    /// one around it, of a package or of a file's top level. Unknown for
    /// what only a hierarchical name reaches.
    pub fn symbol_type(&self, body: BodyId, at: SymbolRef) -> Ty {
        self.type_in(self.cx(body), at)
    }

    /// The bits of a variable that `expr`, written in `body`, names: the
    /// variable's name, or a select or member of it. They count from 0, a
    /// packed value's from its least significant bit, an unpacked array's
    /// from its left bound's element and an unpacked struct's from its
    /// first member, so two places in one variable overlap if their ranges
    /// do. `None` where an index is not known, or the type is not.
    pub fn reach(&self, body: BodyId, expr: ExprId) -> Option<Range<u64>> {
        let (offset, width, _) = self.reached(self.cx(body), expr)?;
        Some(offset..offset + width)
    }

    /// Whether `body`'s parameter `symbol` is given a value from outside
    /// the body: an instance's override, a `defparam`, or a top's.
    pub(crate) fn is_given(&self, body: BodyId, symbol: SymbolId) -> bool {
        self.given(body, symbol).is_some()
    }

    /// What each name `file` uses refers to.
    pub fn names(&self, file: FileId) -> &Names {
        self.design.names(file)
    }

    /// What each port of its definition the instance `at` connects is
    /// connected to, as [`bind`](crate::bind) binds them: in the order
    /// written, then what `.*` connects. A port it leaves unconnected is
    /// not listed; the port is its definition's symbol, what connects it
    /// the instance's.
    pub fn connections(&self, at: SymbolRef) -> Vec<(SymbolId, Bound)> {
        bind::connections(&self.design, at)
    }

    /// What each parameter of its definition the instance `at` overrides is
    /// given, in the order written: by position among those that are not
    /// local, or by name.
    pub fn overrides(&self, at: SymbolRef) -> Vec<(SymbolId, ExprId)> {
        bind::overrides(&self.design, at)
    }

    /// The member of a struct or union that `expr`, an `a.b` written in
    /// `body`, names, as the type of `a` there has it; `None` where `a` is
    /// no struct or union sema knows, or has no member `b`.
    pub fn member(&self, body: BodyId, expr: ExprId) -> Option<MemberTy> {
        let hir = &self.design[self.scope(body).0];
        let ExprKind::Member { base, name } = &hir[expr].kind else {
            return None;
        };
        let ty = match self.expr_type(body, *base) {
            Ty::Struct(ty)
            | Ty::Packed(Packed {
                shape: Shape::Struct(ty),
                ..
            }) => ty,
            _ => return None,
        };
        (ty.members.iter())
            .find(|member| member.name == name.text)
            .cloned()
    }

    // -------------------------------------------------------------- bodies

    /// The body at its defaults that what `scope` of `file` holds is
    /// evaluated in: the innermost definition's or package's around it, or
    /// the file's top level.
    pub fn body_at(&self, file: FileId, scope: ScopeId) -> BodyId {
        let hir = &self.design[file];
        let around = hir.enclosing(scope, ScopeKind::Definition);
        self.default_body(file, around.unwrap_or(hir.root()))
    }

    /// The file and the scope `body` gives values to.
    pub fn scope(&self, body: BodyId) -> (FileId, ScopeId) {
        let body = self.bodies.get(body.index());
        (body.file, body.scope)
    }

    pub(crate) fn cx(&self, body: BodyId) -> Cx<'static> {
        Cx::new(body, self.scope(body).0)
    }

    fn given(&self, body: BodyId, symbol: SymbolId) -> Option<Given> {
        self.bodies.get(body.index()).given.get(&symbol).cloned()
    }

    fn new_body(&self, body: Body) -> BodyId {
        BodyId(self.bodies.push(body) as u32)
    }

    /// The body of a file's top level, or of a definition's or a package's
    /// scope, at its defaults.
    fn default_body(&self, file: FileId, scope: ScopeId) -> BodyId {
        self.defaults[&(file, scope)]
    }

    /// The body `at` takes its value in, seen from `cx`: the innermost
    /// around `cx` whose scope holds it, or the package or file top level
    /// declaring it. `None` for what another definition declares, which only
    /// a hierarchical name reaches, and for a member of a package more than
    /// one file declares, which one a compiler keeps being a guess.
    fn owner(&self, cx: Cx, at: SymbolRef) -> Option<BodyId> {
        let hir = &self.design[at.file];
        // The scopes around `at`, out to its definition's or its file's.
        let mut scope = self.design.symbol(at).scope;
        let mut around = vec![scope];
        while !matches!(hir[scope].kind, ScopeKind::Definition | ScopeKind::File) {
            scope = hir[scope].parent?;
            around.push(scope);
        }
        let mut body = Some(cx.body);
        while let Some(id) = body {
            let held = self.bodies.get(id.index());
            if held.file == at.file && around.contains(&held.scope) {
                return Some(id);
            }
            body = held.parent;
        }
        let package = |owner: Option<SymbolId>| {
            owner.is_some_and(|owner| {
                matches!(
                    hir[owner].kind,
                    SymbolKind::Definition {
                        keyword: PACKAGE_KW,
                        ..
                    }
                )
            })
        };
        match (hir[scope].kind, hir[scope].owner) {
            (ScopeKind::File, _) => Some(self.default_body(at.file, scope)),
            (_, Some(owner)) if package(Some(owner)) => {
                let name = &hir[owner].name.text;
                (!self.design.is_ambiguous(name)).then(|| self.default_body(at.file, scope))
            }
            _ => None,
        }
    }

    // --------------------------------------------------------------- facts

    fn resolution(&self, cx: Cx, expr: ExprId) -> Option<Resolution> {
        self.design.names(cx.file).expr(expr)
    }

    // ------------------------------------------------------------- queries

    fn compute_value(&self, cx: Cx, symbol: SymbolId) -> Value {
        let hir = &self.design[cx.file];
        match &hir[symbol].kind {
            SymbolKind::Parameter(parameter) if !parameter.is_type => {
                let ty = self.type_of(cx.body, symbol);
                if let Some(Given::Literal(given)) = self.given(cx.body, symbol) {
                    return self.convert(given.at(&ty), &ty);
                }
                match self.source(cx, symbol, parameter.value) {
                    Some((from, value)) => self.eval_assign(from, value, &ty),
                    None => Value::Unknown,
                }
            }
            SymbolKind::EnumMember { value, owner } => {
                let Enum { base, members, .. } = &hir[*owner];
                let index = (members.iter().position(|&it| it == symbol))
                    .expect("an enum member is among its enum's members");
                let ty = self.enum_base(cx, base);
                match (value, index) {
                    (Some(value), _) => self.eval_assign(cx, *value, &ty),
                    (None, 0) => self.convert(Value::Bits(Bits::from_u64(1, false, 0)), &ty),
                    (None, _) => match self.value(cx.body, members[index - 1]) {
                        Value::Bits(previous) => {
                            let one = Bits::from_u64(previous.width(), previous.is_signed(), 1);
                            Value::Bits(previous.add(&one))
                        }
                        _ => Value::Unknown,
                    },
                }
            }
            _ => Value::Unknown,
        }
    }

    fn compute_type(&self, cx: Cx, symbol: SymbolId) -> Ty {
        let hir = &self.design[cx.file];
        match &hir[symbol].kind {
            // A type parameter's type is the one it names.
            SymbolKind::Parameter(Parameter {
                is_type: true,
                value,
                ..
            }) => match self.source(cx, symbol, *value) {
                Some((from, value)) => self.type_or_expr(from, value),
                None => Ty::Unknown,
            },
            SymbolKind::Parameter(parameter) => {
                // Neither type nor range: the type of its value.
                let Type {
                    kind: TypeKind::Implicit { signing },
                    packed: 0,
                    ..
                } = &parameter.ty
                else {
                    return self.resolve_type(cx, &parameter.ty);
                };
                let own = match self.given(cx.body, symbol) {
                    Some(Given::Literal(given)) => literal_type(&given),
                    _ => match self.source(cx, symbol, parameter.value) {
                        // `'1` fills a context, and has none here; tools give
                        // it 32 bits.
                        Some((from, value)) if self.fills(from, value) => {
                            Ty::Packed(Packed::vector(32, false, true))
                        }
                        Some((from, value)) => self.self_type(from, value),
                        None => return self.resolve_type(cx, &parameter.ty),
                    },
                };
                let signed = |own: bool| signing.map_or(own, |it| it == SIGNED_KW);
                // Four-state whatever its value, as tools have it, but for an
                // enum or a struct, whose type an assignment to one needs.
                match own {
                    Ty::Packed(
                        own @ Packed {
                            shape: Shape::Vector { .. },
                            ..
                        },
                    ) => Ty::Packed(Packed::vector(own.width, signed(own.signed), true)),
                    Ty::Packed(
                        own @ Packed {
                            shape: Shape::Enum(_) | Shape::Struct(_),
                            ..
                        },
                    ) => Ty::Packed(Packed {
                        signed: signed(own.signed),
                        ..own
                    }),
                    Ty::Packed(own) => four_state(Ty::Packed(Packed {
                        signed: signed(own.signed),
                        ..own
                    })),
                    own => own,
                }
            }
            SymbolKind::Typedef(ty) => {
                let at = SymbolRef {
                    file: cx.file,
                    symbol,
                };
                named(self.resolve_type(cx, ty), &hir[symbol].name.text, at)
            }
            SymbolKind::EnumMember { owner, .. } => match hir[*owner].typedef {
                Some(typedef) => self.type_of(cx.body, typedef),
                None => self.enum_type(cx, *owner),
            },
            SymbolKind::Port(Port { ty, .. })
            | SymbolKind::Net(Data { ty, .. })
            | SymbolKind::Variable(Data { ty, .. }) => self.resolve_type(cx, ty),
            SymbolKind::Genvar(_) => Ty::INTEGER,
            SymbolKind::Subroutine {
                returns: Some(ty), ..
            } => self.resolve_type(cx, ty),
            _ => Ty::Unknown,
        }
    }

    /// Where a parameter of `cx`'s body takes its value from: the override
    /// its instance gives, evaluated where the instance stands, or its
    /// default. A class's parameter has none: each specialization gives it
    /// its own, and they share the class's scope.
    fn source(
        &self,
        cx: Cx,
        symbol: SymbolId,
        default: Option<ExprId>,
    ) -> Option<(Cx<'static>, ExprId)> {
        let hir = &self.design[cx.file];
        let home = hir[symbol].scope;
        if let Some(SymbolKind::Class(class)) = hir[home].owner.map(|owner| &hir[owner].kind)
            && class.parameters.contains(&symbol)
        {
            return None;
        }
        match self.given(cx.body, symbol) {
            Some(Given::Override { body, file, expr }) => Some((Cx::new(body, file), expr)),
            _ => default.map(|default| (self.cx(cx.body), default)),
        }
    }

    /// The type `at` has, seen from `cx`.
    fn type_in(&self, cx: Cx, at: SymbolRef) -> Ty {
        match self.owner(cx, at) {
            // Each iteration of a loop is a body of its own, which would
            // keep its genvar's type once more, though it is always this.
            Some(_) if matches!(self.design.symbol(at).kind, SymbolKind::Genvar(_)) => Ty::INTEGER,
            Some(body) => self.type_of(body, at.symbol),
            None => Ty::Unknown,
        }
    }

    /// The value `at` has, seen from `cx`: a variable of the running
    /// function's, or a constant.
    fn symbol_value(&self, cx: Cx, at: SymbolRef) -> Value {
        if let Some(run) = cx.run
            && at.file == cx.file
            && let Some(value) = run.locals.borrow().get(&at.symbol)
        {
            return value.clone();
        }
        // A genvar has a value only in an iteration of its loop.
        if let SymbolKind::Genvar(_) = self.design.symbol(at).kind {
            let mut body = Some(cx.body);
            while let Some(id) = body {
                let held = self.bodies.get(id.index());
                if held.file == at.file
                    && let Some(Given::Genvar(value)) = held.given.get(&at.symbol)
                {
                    return value.clone();
                }
                body = held.parent;
            }
            return Value::Unknown;
        }
        match self.owner(cx, at) {
            Some(body) => self.value(body, at.symbol),
            None => Value::Unknown,
        }
    }

    // --------------------------------------------------------------- types

    /// The type `ty` spells, its dimensions evaluated in `cx`.
    fn resolve_type(&self, cx: Cx, ty: &Type) -> Ty {
        let base = match &ty.kind {
            TypeKind::Implicit { signing } => {
                Ty::Packed(Packed::scalar(*signing == Some(SIGNED_KW), true))
            }
            TypeKind::Builtin { keyword, signing } => builtin(*keyword, *signing),
            TypeKind::Named(name) => self.named_type(cx, *name),
            TypeKind::Enum(id) => self.enum_type(cx, *id),
            TypeKind::Struct {
                keyword,
                packed,
                signing,
                fields,
            } => self.struct_type(cx, *keyword == UNION_KW, *packed, *signing, fields),
            TypeKind::Opaque(_) => Ty::Unknown,
            TypeKind::TypeOf(expr) => self.type_or_expr(cx, *expr),
            TypeKind::Matched { value, path } => {
                let whole = self.self_type(cx, *value);
                path.iter().fold(whole, |ty, step| matched(&ty, step))
            }
        };
        let (packed, unpacked) = ty.dims.split_at(ty.packed);
        let mut whole = match packed.is_empty() {
            true => base,
            false => match base {
                Ty::Packed(base) => match self.packed_array(cx, base, packed) {
                    Some(array) => Ty::Packed(array),
                    None => return Ty::Unknown,
                },
                _ => return Ty::Unknown,
            },
        };
        // The first unpacked dimension is the outermost.
        for dim in unpacked.iter().rev() {
            let hir = &self.design[cx.file];
            let queue = match dim {
                Dim::Empty => Some(false),
                Dim::Single(size) if matches!(hir[*size].kind, ExprKind::Keyword(DOLLAR)) => {
                    Some(true)
                }
                _ => None,
            };
            if let Some(queue) = queue {
                whole = Ty::Dynamic(Arc::new(DynamicTy {
                    queue,
                    element: whole,
                }));
                continue;
            }
            let Some((left, right)) = self.unpacked_bounds(cx, dim) else {
                return Ty::Unknown;
            };
            whole = Ty::Array(Arc::new(ArrayTy {
                left,
                right,
                element: whole,
            }));
        }
        whole
    }

    /// `base` with the packed dimensions `dims`, the first the outermost. A
    /// scalar's signing is the whole array's, and its first dimension makes
    /// it a vector.
    fn packed_array(&self, cx: Cx, base: Packed, dims: &[Dim]) -> Option<Packed> {
        let signed = matches!(base.shape, Shape::Scalar) && base.signed;
        let mut element = match base.shape {
            Shape::Scalar => None,
            _ => Some(base.clone()),
        };
        let mut width = u64::from(base.width);
        let mut shape = Shape::Scalar;
        for dim in dims.iter().rev() {
            let Dim::Range(left, right) = dim else {
                return None;
            };
            let (left, right) = (self.bound(cx, *left)?, self.bound(cx, *right)?);
            width *= left.abs_diff(right) + 1;
            if width > u64::from(Bits::MAX_WIDTH) {
                return None;
            }
            shape = match element.take() {
                None => Shape::Vector { left, right },
                Some(element) => Shape::Array {
                    left,
                    right,
                    element: Arc::new(element),
                },
            };
            element = Some(Packed {
                width: width as u32,
                signed: false,
                four_state: base.four_state,
                shape: shape.clone(),
            });
        }
        Some(Packed {
            width: width as u32,
            signed,
            four_state: base.four_state,
            shape,
        })
    }

    /// An unpacked dimension's bounds: `[left:right]`, or `[n]` as `[0:n-1]`.
    /// `None` for a dynamic array, a queue or an associative array, whose
    /// size is not a constant.
    pub(crate) fn unpacked_bounds(&self, cx: Cx, dim: &Dim) -> Option<(i64, i64)> {
        let hir = &self.design[cx.file];
        match dim {
            Dim::Range(left, right) => Some((self.bound(cx, *left)?, self.bound(cx, *right)?)),
            Dim::Single(size) => match hir[*size].kind {
                ExprKind::Keyword(_) | ExprKind::Type(_) => None,
                _ if !self.is_constant_size(cx, *size) => None,
                _ => {
                    let size = self.integer(cx, *size).filter(|&size| size > 0)?;
                    Some((0, size - 1))
                }
            },
            Dim::Empty | Dim::Wildcard => None,
        }
    }

    /// Whether `[size]` holds a size rather than an associative array's
    /// index type.
    fn is_constant_size(&self, cx: Cx, size: ExprId) -> bool {
        match self.resolution(cx, size) {
            Some(Resolution::Declared(at)) => !matches!(
                self.design.symbol(at).kind,
                SymbolKind::Typedef(_) | SymbolKind::Parameter(Parameter { is_type: true, .. })
            ),
            _ => true,
        }
    }

    /// A struct or union of `fields`: packed, of packed members, or not.
    fn struct_type(
        &self,
        cx: Cx,
        union: bool,
        packed: bool,
        signing: Option<SyntaxKind>,
        fields: &[Field],
    ) -> Ty {
        let members: Vec<MemberTy> = (fields.iter())
            .map(|field| MemberTy {
                name: field.name.text.clone(),
                file: cx.file,
                span: field.name.span,
                ty: self.resolve_type(cx, &field.ty),
            })
            .collect();
        if !packed {
            return Ty::Struct(Arc::new(StructTy {
                name: None,
                typedef: None,
                union,
                members,
            }));
        }
        let mut widths = Vec::new();
        let mut four_state = false;
        for member in &members {
            let Ty::Packed(member) = &member.ty else {
                return Ty::Unknown;
            };
            widths.push(member.width);
            four_state |= member.four_state;
        }
        let width = match union {
            true => widths.iter().copied().max(),
            false => widths.iter().copied().try_fold(0u32, u32::checked_add),
        };
        match width {
            Some(width) if width > 0 && width <= Bits::MAX_WIDTH => Ty::Packed(Packed {
                width,
                signed: signing == Some(SIGNED_KW),
                four_state,
                shape: Shape::Struct(Arc::new(StructTy {
                    name: None,
                    typedef: None,
                    union,
                    members,
                })),
            }),
            _ => Ty::Unknown,
        }
    }

    /// An enum's base type: `int` when none is written.
    /// The enum `id`, unnamed: a typedef's type names it.
    fn enum_type(&self, cx: Cx, id: EnumId) -> Ty {
        let Enum { base, members, .. } = &self.design[cx.file][id];
        match self.enum_base(cx, base) {
            Ty::Packed(base) => Ty::Packed(Packed {
                shape: Shape::Enum(Arc::new(EnumTy {
                    name: None,
                    typedef: None,
                    base: base.clone(),
                    members: (members.iter())
                        .map(|&symbol| SymbolRef {
                            file: cx.file,
                            symbol,
                        })
                        .collect(),
                })),
                ..base
            }),
            _ => Ty::Unknown,
        }
    }

    fn enum_base(&self, cx: Cx, base: &Type) -> Ty {
        match base {
            Type {
                kind: TypeKind::Implicit { signing: None },
                dims,
                ..
            } if dims.is_empty() => Ty::INT,
            base => self.resolve_type(cx, base),
        }
    }

    /// The type a name stands for: a typedef's, or a type parameter's.
    fn named_type(&self, cx: Cx, name: ExprId) -> Ty {
        // A specialization, `C#(8)`, is its class.
        let name = match self.design[cx.file][name].kind {
            ExprKind::Call { callee, .. } => callee,
            _ => name,
        };
        let Some(Resolution::Declared(at)) = self.resolution(cx, name) else {
            return Ty::Unknown;
        };
        let symbol = self.design.symbol(at);
        match symbol.kind {
            SymbolKind::Typedef(_) | SymbolKind::Parameter(Parameter { is_type: true, .. }) => {
                self.type_in(cx, at)
            }
            SymbolKind::Class(_) => Ty::Class(Arc::new(ClassTy {
                name: symbol.name.text.clone(),
                class: at,
            })),
            _ => Ty::Unknown,
        }
    }

    /// The type `expr` names, if it is a type, or else its own: the operand
    /// of `$bits` or `type()`.
    fn type_or_expr(&self, cx: Cx, expr: ExprId) -> Ty {
        let hir = &self.design[cx.file];
        match &hir[expr].kind {
            ExprKind::Type(ty) => self.resolve_type(cx, ty),
            ExprKind::Keyword(keyword) => builtin(*keyword, None),
            ExprKind::Name(_) | ExprKind::Scoped { .. } => match self.resolution(cx, expr) {
                Some(Resolution::Declared(at)) => match &self.design.symbol(at).kind {
                    SymbolKind::Typedef(_)
                    | SymbolKind::Parameter(Parameter { is_type: true, .. }) => {
                        self.type_in(cx, at)
                    }
                    _ => self.self_type(cx, expr),
                },
                _ => Ty::Unknown,
            },
            ExprKind::Select { .. } => match self.dimensioned_type(cx, expr) {
                Some(ty) => ty,
                None => self.self_type(cx, expr),
            },
            _ => self.self_type(cx, expr),
        }
    }

    /// The type `expr` names if it is a type with packed dimensions,
    /// `nib_t [3:0]`: where a type or a value may stand, it reads as a
    /// part-select of the name, as only the name's declaration tells them
    /// apart. `None` if the name is not a type.
    fn dimensioned_type(&self, cx: Cx, expr: ExprId) -> Option<Ty> {
        let hir = &self.design[cx.file];
        let mut dims = Vec::new();
        let mut at = expr;
        while let ExprKind::Select { base, index, range } = &hir[at].kind {
            // A packed dimension is a range; `[i]` and `[i+:w]` select.
            let Some((COLON, right)) = range else {
                return None;
            };
            dims.push(Dim::Range(*index, *right));
            at = *base;
        }
        if !matches!(hir[at].kind, ExprKind::Name(_) | ExprKind::Scoped { .. })
            || self.is_value(cx, at)
        {
            return None;
        }
        // The innermost select is the first dimension written.
        dims.reverse();
        Some(match self.type_or_expr(cx, at) {
            Ty::Packed(element) => match self.packed_array(cx, element, &dims) {
                Some(array) => Ty::Packed(array),
                None => Ty::Unknown,
            },
            _ => Ty::Unknown,
        })
    }

    /// Whether `expr` is an unbased unsized literal, `'0` or `'1`.
    fn fills(&self, cx: Cx, expr: ExprId) -> bool {
        self.design[cx.file][expr].kind.is_fill()
    }

    /// `expr` as a constant integer, if it is one and fits.
    fn integer(&self, cx: Cx, expr: ExprId) -> Option<i64> {
        match self.eval_self(cx, expr) {
            Value::Bits(bits) => bits.to_i64(),
            _ => None,
        }
    }

    /// `expr` as a dimension's bound, an `int`: an unsigned value that fits
    /// 32 bits is read as one, so `N-1` of an `int unsigned N = 0` is -1,
    /// and `[N-1:0]` two bits rather than four billion, as `slang` has it.
    fn bound(&self, cx: Cx, expr: ExprId) -> Option<i64> {
        let Value::Bits(bits) = self.eval_self(cx, expr) else {
            return None;
        };
        match bits.is_signed() {
            true => bits.to_i64().filter(|&bound| i32::try_from(bound).is_ok()),
            false => bits
                .to_u64()
                .and_then(|bound| u32::try_from(bound).ok())
                .map(|bound| i64::from(bound as i32)),
        }
    }
}

/// The type a keyword names, with `signed` or `unsigned` if written.
pub(crate) fn builtin(keyword: SyntaxKind, signing: Option<SyntaxKind>) -> Ty {
    let scalar = |four_state| Ty::Packed(Packed::scalar(signing == Some(SIGNED_KW), four_state));
    let (width, signed, four_state) = match keyword {
        LOGIC_KW | REG_KW => return scalar(true),
        BIT_KW => return scalar(false),
        BYTE_KW => (8, true, false),
        SHORTINT_KW => (16, true, false),
        INT_KW => (32, true, false),
        LONGINT_KW => (64, true, false),
        INTEGER_KW => (32, true, true),
        TIME_KW => (64, false, true),
        REAL_KW | REALTIME_KW => return Ty::REAL,
        SHORTREAL_KW => return Ty::SHORTREAL,
        STRING_KW => return Ty::String,
        _ => return Ty::Unknown,
    };
    let signed = match signing {
        Some(SIGNED_KW) => true,
        Some(UNSIGNED_KW) => false,
        _ => signed,
    };
    Ty::Packed(Packed::vector(width, signed, four_state))
}

/// The part of a value of type `ty` a pattern's `step` takes apart.
fn matched(ty: &Ty, step: &PatternStep) -> Ty {
    match step {
        PatternStep::Tagged(name) | PatternStep::Member(name) => {
            expr::member(ty, name).map_or(Ty::Unknown, |member| member.ty)
        }
        PatternStep::Position(at) => match ty {
            Ty::Packed(Packed {
                shape: Shape::Struct(ty),
                ..
            })
            | Ty::Struct(ty) => {
                (ty.members.get(*at)).map_or(Ty::Unknown, |member| member.ty.clone())
            }
            _ => Ty::Unknown,
        },
    }
}

/// `ty` as the typedef `name`, at `at`, declares it: a struct or enum the
/// typedef writes takes its name. One a typedef only renames keeps the
/// first, as both name the same type.
fn named(ty: Ty, name: &str, at: SymbolRef) -> Ty {
    let name = Some(name.into());
    let typedef = Some(at);
    match ty {
        Ty::Packed(Packed {
            width,
            signed,
            four_state,
            shape: Shape::Struct(ty),
        }) if ty.typedef.is_none() => Ty::Packed(Packed {
            width,
            signed,
            four_state,
            shape: Shape::Struct(Arc::new(StructTy {
                name,
                typedef,
                ..(*ty).clone()
            })),
        }),
        Ty::Packed(Packed {
            width,
            signed,
            four_state,
            shape: Shape::Enum(ty),
        }) if ty.typedef.is_none() => Ty::Packed(Packed {
            width,
            signed,
            four_state,
            shape: Shape::Enum(Arc::new(EnumTy {
                name,
                typedef,
                ..(*ty).clone()
            })),
        }),
        Ty::Struct(ty) if ty.typedef.is_none() => Ty::Struct(Arc::new(StructTy {
            name,
            typedef,
            ..(*ty).clone()
        })),
        ty => ty,
    }
}
