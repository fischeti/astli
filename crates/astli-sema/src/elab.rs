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

use std::cell::{Cell, OnceCell, RefCell};
use std::hash::Hash;

use astli_syntax::SyntaxKind::{self, *};
use rustc_hash::FxHashMap;

use crate::design::{Design, FileId, SymbolRef};
use crate::hir::*;
use crate::resolve::{Names, Resolution};
use crate::types::{Integral, Ty};
use crate::value::{Bits, Literal, Value, literal};

/// Where values are computed: a package, a definition with its parameters
/// decided, a file's top level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BodyId(u32);

impl BodyId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy)]
struct Body {
    file: FileId,
    /// The scope it gives values to: a definition's, or the file's.
    scope: ScopeId,
}

/// A query computed, or being computed.
enum Slot<T> {
    Busy,
    Done(T),
}

/// What is known of one file, worked out when first needed.
#[derive(Default)]
struct FileFacts<'d> {
    names: OnceCell<Names>,
    /// The scope each symbol is declared in.
    homes: OnceCell<Vec<ScopeId>>,
    /// Each enum member's enum, and its place there.
    enums: OnceCell<FxHashMap<SymbolId, (&'d Type, usize)>>,
}

/// A design elaborated on demand: the values of its constants and the types
/// of its declarations.
///
/// Queries take `&self` and keep their answers inside, so one elaboration
/// serves a whole design; it is not shared across threads.
pub struct Elaboration<'d> {
    design: &'d Design,
    facts: Vec<FileFacts<'d>>,
    bodies: RefCell<Vec<Body>>,
    /// The body of each package, each definition at its defaults, and each
    /// file's top level.
    defaults: RefCell<FxHashMap<(FileId, ScopeId), BodyId>>,
    values: RefCell<FxHashMap<(BodyId, SymbolId), Slot<Value>>>,
    types: RefCell<FxHashMap<(BodyId, SymbolId), Slot<Ty>>>,
    /// How deep constant functions are calling each other.
    depth: Cell<u32>,
    /// The statements the outermost constant function running may still
    /// take, those it calls included.
    steps: Cell<u32>,
}

/// Where an expression is evaluated: in a body, and in the frame of a
/// constant function if one is running.
#[derive(Clone, Copy)]
pub(crate) struct Cx<'a> {
    body: BodyId,
    file: FileId,
    frame: Option<&'a Frame>,
}

/// A constant function's run: its arguments, variables and return value.
pub(crate) struct Frame {
    function: SymbolId,
    locals: RefCell<FxHashMap<SymbolId, Value>>,
}

impl<'d> Elaboration<'d> {
    pub fn new(design: &'d Design) -> Elaboration<'d> {
        Elaboration {
            design,
            facts: design.files().map(|_| FileFacts::default()).collect(),
            bodies: RefCell::default(),
            defaults: RefCell::default(),
            values: RefCell::default(),
            types: RefCell::default(),
            depth: Cell::new(0),
            steps: Cell::new(0),
        }
    }

    pub fn design(&self) -> &'d Design {
        self.design
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
        memo(&self.values, (body, symbol), Value::Unknown, || {
            self.compute_value(cx, symbol)
        })
    }

    /// The type of a declaration of `body`'s: a parameter's, a typedef's, an
    /// enum member's, a net's, a variable's, a function's return type.
    pub fn type_of(&self, body: BodyId, symbol: SymbolId) -> Ty {
        let cx = self.cx(body);
        memo(&self.types, (body, symbol), Ty::Unknown, || {
            self.compute_type(cx, symbol)
        })
    }

    // -------------------------------------------------------------- bodies

    fn cx(&self, body: BodyId) -> Cx<'static> {
        Cx {
            body,
            file: self.bodies.borrow()[body.index()].file,
            frame: None,
        }
    }

    fn default_body(&self, file: FileId, scope: ScopeId) -> BodyId {
        if let Some(&body) = self.defaults.borrow().get(&(file, scope)) {
            return body;
        }
        let mut bodies = self.bodies.borrow_mut();
        let body = BodyId(bodies.len() as u32);
        bodies.push(Body { file, scope });
        self.defaults.borrow_mut().insert((file, scope), body);
        body
    }

    /// The body `at` takes its value in, seen from `cx`: `cx`'s own if `at`
    /// is declared there, or the package or file top level declaring it.
    /// `None` for what another definition declares, which only a
    /// hierarchical name reaches, and for a member of a package more than
    /// one file declares, which one a compiler keeps being a guess.
    fn owner(&self, cx: Cx, at: SymbolRef) -> Option<BodyId> {
        let hir = &self.design[at.file];
        let mut scope = self.home(at.file, at.symbol);
        while !matches!(hir[scope].kind, ScopeKind::Definition | ScopeKind::File) {
            scope = hir[scope].parent?;
        }
        let body = self.bodies.borrow()[cx.body.index()];
        if (body.file, body.scope) == (at.file, scope) {
            return Some(cx.body);
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

    pub(crate) fn names(&self, file: FileId) -> &Names {
        self.facts[file.index()]
            .names
            .get_or_init(|| self.design.resolve(file))
    }

    pub(crate) fn resolution(&self, cx: Cx, expr: ExprId) -> Option<Resolution> {
        self.names(cx.file).expr(expr)
    }

    fn home(&self, file: FileId, symbol: SymbolId) -> ScopeId {
        let homes = self.facts[file.index()].homes.get_or_init(|| {
            let hir = &self.design[file];
            let mut homes = vec![hir.root(); hir.symbols.len()];
            for (id, scope) in hir.scopes() {
                for member in &scope.members {
                    if let Member::Declare(symbol) = member {
                        homes[symbol.index()] = id;
                    }
                }
            }
            homes
        });
        homes[symbol.index()]
    }

    fn enum_of(&self, file: FileId, member: SymbolId) -> Option<(&'d Type, usize)> {
        let enums = self.facts[file.index()].enums.get_or_init(|| {
            let hir = &self.design[file];
            let mut enums = FxHashMap::default();
            let mut types: Vec<&'d Type> = Vec::new();
            for (_, symbol) in hir.symbols() {
                match &symbol.kind {
                    SymbolKind::Port(Port { ty, .. })
                    | SymbolKind::Parameter(Parameter { ty, .. })
                    | SymbolKind::Net(Data { ty, .. })
                    | SymbolKind::Variable(Data { ty, .. })
                    | SymbolKind::Typedef(ty)
                    | SymbolKind::Subroutine {
                        returns: Some(ty), ..
                    } => types.push(ty),
                    _ => {}
                }
            }
            for (_, expr) in hir.exprs() {
                if let ExprKind::Type(ty) = &expr.kind {
                    types.push(ty);
                }
            }
            while let Some(ty) = types.pop() {
                match &ty.kind {
                    TypeKind::Enum { base, members } => {
                        types.push(base);
                        for (index, &member) in members.iter().enumerate() {
                            enums.insert(member, (ty, index));
                        }
                    }
                    TypeKind::Struct { fields, .. } => {
                        types.extend(fields.iter().map(|field| &field.ty));
                    }
                    _ => {}
                }
            }
            enums
        });
        enums.get(&member).copied()
    }

    // ------------------------------------------------------------- queries

    fn compute_value(&self, cx: Cx, symbol: SymbolId) -> Value {
        let hir = &self.design[cx.file];
        match &hir[symbol].kind {
            SymbolKind::Parameter(parameter) if !parameter.is_type => {
                let ty = self.type_of(cx.body, symbol);
                match parameter.value {
                    Some(value) => self.eval_assign(cx, value, ty),
                    None => Value::Unknown,
                }
            }
            SymbolKind::EnumMember { value } => {
                let Some((enum_ty, index)) = self.enum_of(cx.file, symbol) else {
                    return Value::Unknown;
                };
                let TypeKind::Enum { base, members } = &enum_ty.kind else {
                    return Value::Unknown;
                };
                let ty = self.enum_base(cx, base);
                match (value, index) {
                    (Some(value), _) => self.eval_assign(cx, *value, ty),
                    (None, 0) => self.convert(Value::Bits(Bits::from_u64(1, false, 0)), ty),
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
            SymbolKind::Parameter(parameter) if parameter.is_type => Ty::Unknown,
            SymbolKind::Parameter(parameter) => match (&parameter.ty, parameter.value) {
                // Neither type nor range: the type of its value.
                (
                    Type {
                        kind: TypeKind::Implicit { signing },
                        packed: 0,
                        ..
                    },
                    Some(value),
                ) => match self.self_type(cx, value) {
                    // `'1` fills a context, and has none here; tools give it
                    // 32 bits.
                    Ty::Integral(_) if self.fills(cx, value) => {
                        let signed = signing == &Some(SIGNED_KW);
                        Ty::Integral(Integral::vector(32, signed, true))
                    }
                    Ty::Integral(own) => {
                        let signed = signing.map_or(own.signed, |it| it == SIGNED_KW);
                        Ty::Integral(Integral::vector(own.width, signed, own.four_state))
                    }
                    own => own,
                },
                (ty, _) => self.resolve_type(cx, ty),
            },
            SymbolKind::Typedef(ty) => self.resolve_type(cx, ty),
            SymbolKind::EnumMember { .. } => match self.enum_of(cx.file, symbol) {
                Some((ty, _)) => self.resolve_type(cx, ty),
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

    /// The type `at` has, seen from `cx`.
    pub(crate) fn symbol_type(&self, cx: Cx, at: SymbolRef) -> Ty {
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
        match self.owner(cx, at) {
            Some(body) => self.value(body, at.symbol),
            None => Value::Unknown,
        }
    }

    // --------------------------------------------------------------- types

    /// The type `ty` spells, its dimensions evaluated in `cx`.
    pub(crate) fn resolve_type(&self, cx: Cx, ty: &Type) -> Ty {
        let base = match &ty.kind {
            TypeKind::Implicit { signing } => Ty::Integral(Integral {
                width: 1,
                signed: *signing == Some(SIGNED_KW),
                four_state: true,
                range: Some((0, 0)),
            }),
            TypeKind::Builtin { keyword, signing } => builtin(*keyword, *signing),
            TypeKind::Named(name) => self.named_type(cx, *name),
            TypeKind::Enum { base, .. } => self.enum_base(cx, base),
            TypeKind::Struct {
                keyword,
                packed: true,
                signing,
                fields,
            } => {
                let mut widths = Vec::new();
                let mut four_state = false;
                for field in fields {
                    let Ty::Integral(field) = self.resolve_type(cx, &field.ty) else {
                        return Ty::Unknown;
                    };
                    widths.push(field.width);
                    four_state |= field.four_state;
                }
                let width = match keyword {
                    UNION_KW => widths.iter().copied().max(),
                    _ => widths.iter().copied().try_fold(0u32, u32::checked_add),
                };
                match width {
                    Some(width) if width > 0 && width <= Bits::MAX_WIDTH => {
                        Ty::Integral(Integral {
                            width,
                            signed: *signing == Some(SIGNED_KW),
                            four_state,
                            range: None,
                        })
                    }
                    _ => Ty::Unknown,
                }
            }
            TypeKind::Struct { .. } | TypeKind::Opaque(_) => Ty::Unknown,
            TypeKind::TypeOf(expr) => self.type_or_expr(cx, *expr),
        };
        if ty.dims.len() > ty.packed {
            return Ty::Unknown;
        }
        let Ty::Integral(mut integral) = base else {
            return if ty.packed == 0 { base } else { Ty::Unknown };
        };
        // A scalar with one dimension is a vector its select indexes; more
        // dimensions select a whole element, which needs types sema does
        // not model yet.
        let scalar = integral.width == 1 && integral.range == Some((0, 0));
        for (index, dim) in ty.dims[..ty.packed].iter().enumerate() {
            let Dim::Range(left, right) = dim else {
                return Ty::Unknown;
            };
            let (Some(left), Some(right)) = (self.integer(cx, *left), self.integer(cx, *right))
            else {
                return Ty::Unknown;
            };
            let size = left.abs_diff(right) + 1;
            let width = u64::from(integral.width) * size;
            if width > u64::from(Bits::MAX_WIDTH) {
                return Ty::Unknown;
            }
            integral.width = width as u32;
            integral.range = match (scalar, index, ty.packed) {
                (true, 0, 1) => Some((left, right)),
                _ => None,
            };
        }
        Ty::Integral(integral)
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

    /// The type a name stands for: a typedef's.
    fn named_type(&self, cx: Cx, name: ExprId) -> Ty {
        let Some(Resolution::Declared(at)) = self.resolution(cx, name) else {
            return Ty::Unknown;
        };
        match self.design.symbol(at).kind {
            SymbolKind::Typedef(_) => self.symbol_type(cx, at),
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
                    SymbolKind::Typedef(_) => self.symbol_type(cx, at),
                    SymbolKind::Parameter(parameter) if parameter.is_type => Ty::Unknown,
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
}

/// The type a keyword names, with `signed` or `unsigned` if written.
pub(crate) fn builtin(keyword: SyntaxKind, signing: Option<SyntaxKind>) -> Ty {
    let (width, signed, four_state) = match keyword {
        LOGIC_KW | REG_KW => (1, false, true),
        BIT_KW => (1, false, false),
        BYTE_KW => (8, true, false),
        SHORTINT_KW => (16, true, false),
        INT_KW => (32, true, false),
        LONGINT_KW => (64, true, false),
        INTEGER_KW => (32, true, true),
        TIME_KW => (64, false, true),
        REAL_KW | REALTIME_KW | SHORTREAL_KW => return Ty::Real,
        STRING_KW => return Ty::String,
        _ => return Ty::Unknown,
    };
    let signed = match signing {
        Some(SIGNED_KW) => true,
        Some(UNSIGNED_KW) => false,
        _ => signed,
    };
    Ty::Integral(Integral::vector(width, signed, four_state))
}

/// The answer to the query `key` in `slots`, computed if not yet; `cycle` if
/// the query is already being computed.
fn memo<K: Hash + Eq + Copy, T: Clone>(
    slots: &RefCell<FxHashMap<K, Slot<T>>>,
    key: K,
    cycle: T,
    compute: impl FnOnce() -> T,
) -> T {
    match slots.borrow().get(&key) {
        Some(Slot::Done(value)) => return value.clone(),
        Some(Slot::Busy) => return cycle,
        None => {}
    }
    slots.borrow_mut().insert(key, Slot::Busy);
    let value = compute();
    slots.borrow_mut().insert(key, Slot::Done(value.clone()));
    value
}
