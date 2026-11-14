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
//! wider of its sides, then truncates. A constant function runs in a frame
//! of its own, under a limit on the statements it may take.
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
mod tree;

use std::cell::RefCell;
use std::ops::Range;
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, OnceLock};

use astli_syntax::SyntaxKind::{self, *};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::design::{Design, FileId, SymbolRef};
use crate::hir::*;
use crate::resolve::{Names, Resolution};
use crate::types::{ArrayTy, ClassTy, DynamicTy, EnumTy, MemberTy, Packed, Shape, StructTy, Ty};
use crate::value::{Bits, Literal, Value, literal};
use expr::{four_state, literal_type, string_bits};
use shared::{Arena, Memo, Shards};
pub(crate) use tree::Connected;
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
    /// this one.
    deferred: Vec<Deferred>,
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

/// Why [`Elaboration::top_with`] cannot build a top as asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopError {
    /// No module, interface or program has the name.
    NoTop,
    /// The top has no parameter of this name.
    NoParameter(String),
    /// A `localparam`, which only its declaration sets.
    Local(String),
    /// A type parameter, which a value cannot name.
    Type(String),
    /// The parameter named, and text that spells no number or string.
    NotAValue(String, String),
}

/// The literal `text` spells as a parameter's value set from outside the
/// design: a number, which a `-` before may negate, a real or a string.
fn given_literal(text: &str) -> Option<Literal> {
    let (negated, text) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    match (literal(text), negated) {
        (Literal::Unknown, _) => None,
        (Literal::Bits(bits), true) => Some(Literal::Bits(bits.neg())),
        (Literal::Real(real), true) => Some(Literal::Real(-real)),
        (_, true) => None,
        (it, false) => Some(it),
    }
}

/// What is known of one file, worked out when first needed.
#[derive(Default)]
struct FileFacts {
    /// Each enum member's enum, its place there, and the typedef that
    /// names the enum, if one does.
    enums: OnceLock<FxHashMap<SymbolId, EnumOf>>,
    /// The scopes holding a `defparam` or a chosen `bind`, and those around
    /// them.
    defers: OnceLock<FxHashSet<ScopeId>>,
}

/// The enum an enum member belongs to.
#[derive(Clone)]
struct EnumOf {
    /// The enum's type as written, shared by its members.
    ty: Arc<Type>,
    index: usize,
    typedef: Option<SymbolId>,
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
    queries: Queries,
}

/// The queries of an elaboration, and their answers, shared by the threads
/// that ask them.
struct Queries {
    design: Arc<Design>,
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
    /// How many generate loop iterations have been built, by every thread.
    blocks: AtomicU32,
}

/// Where an expression is evaluated: in a body, and in the frame of a
/// constant function if one is running.
#[derive(Clone, Copy)]
pub(crate) struct Cx<'a> {
    body: BodyId,
    file: FileId,
    frame: Option<&'a Frame>,
    /// What a walk of this body has typed and evaluated, outside any
    /// function's frame.
    kept: Option<&'a Kept>,
}

impl Cx<'_> {
    pub(crate) fn file(&self) -> FileId {
        self.file
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
            frame: None,
            kept: Some(&self.kept),
        }
    }

    /// [`Elaboration::expr_type`] in this body.
    pub fn expr_type(&self, expr: ExprId) -> Ty {
        self.elaboration.queries().self_type(self.cx(), expr)
    }

    /// [`Elaboration::eval`] in this body.
    pub fn eval(&self, expr: ExprId) -> Value {
        self.elaboration.queries().eval_self(self.cx(), expr)
    }
}

/// A constant function's run: its arguments, variables and return value.
pub(crate) struct Frame {
    function: SymbolId,
    locals: RefCell<FxHashMap<SymbolId, Value>>,
}

// Threads share an elaboration, as an editor's requests or a linter's
// files do.
const _: fn() = || {
    fn shared<T: Send + Sync>() {}
    shared::<Elaboration>();
};

impl Elaboration {
    /// An elaboration of `design` in which every `bind` applies, as though
    /// each definition were elaborated.
    pub fn new(design: Arc<Design>) -> Elaboration {
        Elaboration {
            design: design.clone(),
            queries: Queries::new(design, None),
        }
    }

    /// An elaboration of `design` from `tops`, in which a `bind` a
    /// definition holds applies only if the tops reach that definition, as
    /// only what is elaborated binds.
    pub fn with_tops(design: Arc<Design>, tops: &[impl AsRef<str>]) -> Elaboration {
        let reached = design.reached(tops);
        Elaboration {
            design: design.clone(),
            queries: Queries::new(design, Some(reached)),
        }
    }

    pub fn design(&self) -> &Design {
        &self.design
    }

    fn queries(&self) -> &Queries {
        &self.queries
    }

    /// The body of a package, or of a module, interface or program with its
    /// parameters at their defaults; `None` for any other symbol.
    pub fn body(&self, definition: SymbolRef) -> Option<BodyId> {
        self.queries().body(definition)
    }

    /// The value of a parameter or enum member of `body`'s, declared in it
    /// or in a generate block inside it; unknown for anything else.
    pub fn value(&self, body: BodyId, symbol: SymbolId) -> Value {
        self.queries().value(body, symbol)
    }

    /// The type of a declaration of `body`'s: a parameter's, a typedef's, an
    /// enum member's, a net's, a variable's, a function's return type.
    pub fn type_of(&self, body: BodyId, symbol: SymbolId) -> Ty {
        self.queries().type_of(body, symbol)
    }

    /// The type `expr`, written in `body`, has by itself, before the
    /// expression around it sizes it.
    pub fn expr_type(&self, body: BodyId, expr: ExprId) -> Ty {
        let queries = self.queries();
        queries.self_type(queries.cx(body), expr)
    }

    /// The value of `expr`, written in `body`, at the type it has by
    /// itself.
    pub fn eval(&self, body: BodyId, expr: ExprId) -> Value {
        let queries = self.queries();
        queries.eval_self(queries.cx(body), expr)
    }

    /// The bounds of the unpacked dimension `dim`, written in `body`:
    /// `[left:right]`, or `[n]` as `[0:n-1]`. `None` where it is not a
    /// constant size.
    pub(crate) fn unpacked_bounds(&self, body: BodyId, dim: &Dim) -> Option<(i64, i64)> {
        let queries = self.queries();
        queries.unpacked_bounds(queries.cx(body), dim)
    }

    /// `body`, for a walk that asks the type and value of many of its
    /// expressions and of the expressions they are built of.
    pub fn walk(&self, body: BodyId) -> Walked<'_> {
        Walked {
            elaboration: self,
            body,
            file: self.queries().scope(body).0,
            kept: Kept::default(),
        }
    }

    /// What the arguments of an elaboration system task, written in `body`,
    /// say: a string, as `$sformatf` formats it with the rest. `None` where
    /// the first is no string, or sema cannot tell what they make.
    pub(crate) fn message(&self, body: BodyId, args: &[Arg]) -> Option<String> {
        let queries = self.queries();
        let cx = queries.cx(body);
        let Some(Arg::Positional(Some(first))) = args.first() else {
            return None;
        };
        let hir = &self.design[cx.file];
        let literal = matches!(&hir[*first].kind, ExprKind::Literal(text)
            if matches!(literal(text), Literal::String(_)));
        if !literal && queries.self_type(cx, *first) != Ty::String {
            return None;
        }
        match queries.formatted(cx, args) {
            Value::String(text) => Some(String::from_utf8_lossy(&text).into_owned()),
            _ => None,
        }
    }

    /// The type of `at` as `body` sees it: a declaration of the body's, of
    /// one around it, of a package or of a file's top level. Unknown for
    /// what only a hierarchical name reaches.
    pub fn symbol_type(&self, body: BodyId, at: SymbolRef) -> Ty {
        let queries = self.queries();
        queries.type_in(queries.cx(body), at)
    }

    /// The bits of a variable that `expr`, written in `body`, names: the
    /// variable's name, or a select or member of it. They count from 0, a
    /// packed value's from its least significant bit, an unpacked array's
    /// from its left bound's element and an unpacked struct's from its
    /// first member, so two places in one variable overlap if their ranges
    /// do. `None` where an index is not known, or the type is not.
    pub fn reach(&self, body: BodyId, expr: ExprId) -> Option<Range<u64>> {
        let queries = self.queries();
        let (offset, width, _) = queries.reached(queries.cx(body), expr)?;
        Some(offset..offset + width)
    }

    /// The file and the scope `body` gives values to.
    pub fn scope(&self, body: BodyId) -> (FileId, ScopeId) {
        self.queries().scope(body)
    }

    /// The body of `name` as a top, as [`Elaboration::top`] gives it, with
    /// each of `parameters` set, by name, to the value its text spells: a
    /// number, sized or not and maybe negated, a real or a string, as a
    /// simulator's `-G` takes it. A `defparam` whose path starts at the top
    /// still wins, as it does over an instance's override.
    pub fn top_with(
        &self,
        name: &str,
        parameters: &[(impl AsRef<str>, impl AsRef<str>)],
    ) -> Result<BodyId, TopError> {
        let definition = self.design.definition(name).ok_or(TopError::NoTop)?;
        self.queries().top_with(definition, parameters)
    }

    /// Whether `body`'s parameter `symbol` is given a value from outside
    /// the body: an instance's override, a `defparam`, or a top's.
    pub(crate) fn is_given(&self, body: BodyId, symbol: SymbolId) -> bool {
        self.queries().given(body, symbol).is_some()
    }

    /// What each name `file` uses refers to.
    pub fn names(&self, file: FileId) -> &Names {
        self.design.names(file)
    }

    /// The body of the module, interface or program `name` as a top: its
    /// parameters at their defaults, and what the `defparam`s and `bind`s
    /// whose paths start at it change.
    pub fn top(&self, name: &str) -> Option<BodyId> {
        self.queries().top(self.design.definition(name)?)
    }

    /// What `body` holds once built: its instances, and its generate blocks
    /// that are built, in the order written.
    pub fn children(&self, body: BodyId) -> Arc<[Child]> {
        self.queries().children(body)
    }

    /// What each interface port of the instance `at`, built in `holder`, is
    /// connected to.
    pub(crate) fn interface_connections(
        &self,
        holder: BodyId,
        at: SymbolRef,
    ) -> Vec<(SymbolId, Connected)> {
        self.queries().interface_connections(holder, at)
    }

    /// The tree from `top` down, an indented line each: an instance's name,
    /// its definition, and the parameters its overrides change; a generate
    /// block's name; what sema cannot decide or find.
    pub fn dump(&self, top: BodyId) -> String {
        self.queries().dump(top)
    }
}

impl Queries {
    fn new(design: Arc<Design>, elaborated: Option<FxHashSet<SymbolRef>>) -> Queries {
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
                        deferred: Vec::new(),
                    });
                    defaults.insert((file, scope), BodyId(body as u32));
                }
            }
        }
        Queries {
            facts: design.files().map(|_| FileFacts::default()).collect(),
            design,
            elaborated,
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
            blocks: AtomicU32::new(0),
        }
    }

    fn body(&self, definition: SymbolRef) -> Option<BodyId> {
        match self.design.symbol(definition).kind {
            SymbolKind::Definition { scope, .. } => Some(self.default_body(definition.file, scope)),
            _ => None,
        }
    }

    fn value(&self, body: BodyId, symbol: SymbolId) -> Value {
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

    fn type_of(&self, body: BodyId, symbol: SymbolId) -> Ty {
        let cx = self.cx(body);
        self.types.get((body, symbol), Ty::Unknown, || {
            self.compute_type(cx, symbol)
        })
    }

    // -------------------------------------------------------------- bodies

    fn scope(&self, body: BodyId) -> (FileId, ScopeId) {
        let body = self.bodies.get(body.index());
        (body.file, body.scope)
    }

    pub(crate) fn cx(&self, body: BodyId) -> Cx<'static> {
        Cx {
            body,
            file: self.scope(body).0,
            frame: None,
            kept: None,
        }
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
        let mut scope = self.design.home(at);
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

    pub(crate) fn resolution(&self, cx: Cx, expr: ExprId) -> Option<Resolution> {
        self.design.names(cx.file).expr(expr)
    }

    fn enum_of(&self, file: FileId, member: SymbolId) -> Option<EnumOf> {
        let enums = self.facts[file.index()].enums.get_or_init(|| {
            let hir = &self.design[file];
            let mut enums = FxHashMap::default();
            let mut types: Vec<(&Type, Option<SymbolId>)> = Vec::new();
            for (id, symbol) in hir.symbols() {
                match &symbol.kind {
                    SymbolKind::Typedef(ty) => types.push((ty, Some(id))),
                    SymbolKind::Port(Port { ty, .. })
                    | SymbolKind::Parameter(Parameter { ty, .. })
                    | SymbolKind::Net(Data { ty, .. })
                    | SymbolKind::Variable(Data { ty, .. })
                    | SymbolKind::Subroutine {
                        returns: Some(ty), ..
                    } => types.push((ty, None)),
                    _ => {}
                }
            }
            for (_, expr) in hir.exprs() {
                if let ExprKind::Type(ty) = &expr.kind {
                    types.push((ty, None));
                }
            }
            while let Some((ty, typedef)) = types.pop() {
                match &ty.kind {
                    TypeKind::Enum { base, members } => {
                        types.push((base, None));
                        // A typedef names the enum only if it adds no
                        // dimensions.
                        let typedef = typedef.filter(|_| ty.dims.is_empty());
                        let shared = Arc::new(ty.clone());
                        for (index, &member) in members.iter().enumerate() {
                            let ty = shared.clone();
                            enums.insert(member, EnumOf { ty, index, typedef });
                        }
                    }
                    TypeKind::Struct { fields, .. } => {
                        types.extend(fields.iter().map(|field| (&field.ty, None)));
                    }
                    _ => {}
                }
            }
            enums
        });
        enums.get(&member).cloned()
    }

    // ------------------------------------------------------------- queries

    fn compute_value(&self, cx: Cx, symbol: SymbolId) -> Value {
        let hir = &self.design[cx.file];
        match &hir[symbol].kind {
            SymbolKind::Parameter(parameter) if !parameter.is_type => {
                let ty = self.type_of(cx.body, symbol);
                if let Some(Given::Literal(given)) = self.given(cx.body, symbol) {
                    let value = match (given, &ty) {
                        (Literal::Bits(bits), _) => Value::Bits(bits),
                        (Literal::Fill(bit), Ty::Packed(packed)) => {
                            Value::Bits(Bits::filled(packed.width, packed.signed, bit))
                        }
                        (Literal::Fill(bit), _) => Value::Bits(Bits::bit_value(bit)),
                        (Literal::Real(real), _) => Value::Real(real),
                        (Literal::String(bytes), Ty::String) => Value::String(bytes),
                        (Literal::String(bytes), _) => Value::Bits(string_bits(&bytes)),
                        (Literal::Unknown, _) => Value::Unknown,
                    };
                    return self.convert(value, &ty);
                }
                match self.source(cx, symbol, parameter.value) {
                    Some((from, value)) => self.eval_assign(from, value, &ty),
                    None => Value::Unknown,
                }
            }
            SymbolKind::EnumMember { value } => {
                let Some(EnumOf {
                    ty: enum_ty, index, ..
                }) = self.enum_of(cx.file, symbol)
                else {
                    return Value::Unknown;
                };
                let TypeKind::Enum { base, members } = &enum_ty.kind else {
                    return Value::Unknown;
                };
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
            SymbolKind::EnumMember { .. } => match self.enum_of(cx.file, symbol) {
                Some(EnumOf {
                    typedef: Some(typedef),
                    ..
                }) => self.type_of(cx.body, typedef),
                Some(EnumOf { ty, .. }) => {
                    let mut ty = (*ty).clone();
                    ty.dims.clear();
                    ty.packed = 0;
                    self.resolve_type(cx, &ty)
                }
                None => Ty::Unknown,
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
        let home = self.design.home(SymbolRef {
            file: cx.file,
            symbol,
        });
        if let Some(SymbolKind::Class(class)) = hir[home].owner.map(|owner| &hir[owner].kind)
            && class.parameters.contains(&symbol)
        {
            return None;
        }
        match self.given(cx.body, symbol) {
            Some(Given::Override { body, file, expr }) => {
                let cx = Cx {
                    body,
                    file,
                    frame: None,
                    kept: None,
                };
                Some((cx, expr))
            }
            _ => default.map(|default| (self.cx(cx.body), default)),
        }
    }

    /// The type `at` has, seen from `cx`.
    pub(crate) fn type_in(&self, cx: Cx, at: SymbolRef) -> Ty {
        match self.owner(cx, at) {
            Some(body) => self.type_of(body, at.symbol),
            None => Ty::Unknown,
        }
    }

    /// The value `at` has, seen from `cx`: a variable of the running
    /// function's, or a constant.
    pub(crate) fn symbol_value(&self, cx: Cx, at: SymbolRef) -> Value {
        if let Some(frame) = cx.frame
            && at.file == cx.file
            && let Some(value) = frame.locals.borrow().get(&at.symbol)
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
    pub(crate) fn resolve_type(&self, cx: Cx, ty: &Type) -> Ty {
        let base = match &ty.kind {
            TypeKind::Implicit { signing } => {
                Ty::Packed(Packed::scalar(*signing == Some(SIGNED_KW), true))
            }
            TypeKind::Builtin { keyword, signing } => builtin(*keyword, *signing),
            TypeKind::Named(name) => self.named_type(cx, *name),
            TypeKind::Enum { base, members } => match self.enum_base(cx, base) {
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
            },
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
    fn unpacked_bounds(&self, cx: Cx, dim: &Dim) -> Option<(i64, i64)> {
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
    pub(crate) fn type_or_expr(&self, cx: Cx, expr: ExprId) -> Ty {
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
            _ => self.self_type(cx, expr),
        }
    }

    /// Whether `expr` is an unbased unsized literal, `'0` or `'1`.
    fn fills(&self, cx: Cx, expr: ExprId) -> bool {
        let hir = &self.design[cx.file];
        matches!(&hir[expr].kind, ExprKind::Literal(text) if matches!(literal(text), Literal::Fill(_)))
    }

    /// `expr` as a constant integer, if it is one and fits.
    pub(crate) fn integer(&self, cx: Cx, expr: ExprId) -> Option<i64> {
        match self.eval_self(cx, expr) {
            Value::Bits(bits) => bits.to_i64(),
            _ => None,
        }
    }

    /// `expr` as a dimension's bound, an `int`: an unsigned value that fits
    /// 32 bits is read as one, so `N-1` of an `int unsigned N = 0` is -1,
    /// and `[N-1:0]` two bits rather than four billion, as `slang` has it.
    pub(crate) fn bound(&self, cx: Cx, expr: ExprId) -> Option<i64> {
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
