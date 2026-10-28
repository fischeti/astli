//! The instance tree: what each body holds once built. An instance's body is
//! its definition's with the parameters its overrides decide, one for each
//! set of values; a generate construct builds the arm its condition picks,
//! or a block per iteration of its loop.

use std::fmt::Write;
use std::sync::Arc;

use astli_syntax::SyntaxKind::*;
use rustc_hash::FxHashMap;

use super::expr::{merge, truth};
use super::{Body, BodyId, Cx, Given, Queries};
use crate::design::SymbolRef;
use crate::hir::*;
use crate::resolve::Resolution;
use crate::types::Ty;
use crate::value::{Bit, Bits, Value};

/// How many iterations a generate loop may build before sema calls it
/// undecided: more than any design's, and soon reached by one that never
/// ends.
const ITERATIONS: u32 = 1 << 16;

/// How many generate blocks one elaboration may build in all, past which
/// a construct is undecided: a design's tree times its tops, and a bound on
/// what loops nested in loops can cost.
const BLOCKS: u32 = 1 << 22;

/// How deep [`Elaboration::dump`](crate::Elaboration::dump) follows the tree, which a definition that
/// instantiates itself makes endless.
const DEPTH: usize = 128;

/// One thing a body holds once built.
#[derive(Debug, Clone)]
pub struct Child {
    /// An instance's name, or a generate block's: its label, or `genblk`
    /// and the construct's place among the body's, with `[i]` for a loop's
    /// iteration. An undecided construct's names each arm's block, joined by
    /// `, `.
    pub name: Box<str>,
    pub kind: ChildKind,
}

#[derive(Debug, Clone)]
pub enum ChildKind {
    /// An instance, or an array of them, all with one body.
    Instance {
        /// The instance's symbol: in the file of the body holding it, or a
        /// `bind`'s.
        symbol: SymbolRef,
        definition: SymbolRef,
        body: BodyId,
        /// An array's dimensions; `None` if one cannot be evaluated.
        dims: Option<Vec<(i64, i64)>>,
    },
    /// An instance of a definition no file declares.
    Missing { symbol: SymbolRef },
    /// An instance of a gate or a switch, which has no body.
    Gate { symbol: SymbolRef },
    /// A generate block built.
    Block { body: BodyId },
    /// A generate construct whose condition, selector or bounds sema
    /// cannot evaluate: any of these scopes may be built, or none.
    Undecided { scopes: Vec<ScopeId> },
}

/// A body of a definition, and what tells it from the others.
pub(crate) type Keyed = (Vec<Key>, BodyId);

/// What tells two bodies of one definition apart: each parameter's value,
/// or the type a type parameter names.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Key {
    Value(Value),
    Type(Ty),
}

impl Queries {
    pub(super) fn children(&self, body: BodyId) -> Arc<[Child]> {
        if let Some(children) = self.children.borrow().get(&body) {
            return children.clone();
        }
        let mut children = Vec::new();
        let (file, scope) = self.scope(body);
        self.collect(self.cx(body), scope, &mut children);
        // What a `bind` adds stands at the end of a definition's body.
        let hir = &self.design[file];
        if let Some(owner) = hir[scope].owner
            && let SymbolKind::Definition { .. } = hir[owner].kind
        {
            for bound in self.design.binds(&hir[owner].name.text) {
                let applies = bound.holder.is_none_or(|holder| {
                    (self.elaborated.as_ref()).is_none_or(|elaborated| elaborated.contains(&holder))
                });
                if applies
                    && let SymbolKind::Instance(instance) = &self.design.symbol(bound.instance).kind
                {
                    let cx = Cx {
                        body,
                        file: bound.instance.file,
                        frame: None,
                    };
                    children.push(self.instance(cx, bound.instance.symbol, instance));
                }
            }
        }
        let children: Arc<[Child]> = children.into();
        self.children.borrow_mut().insert(body, children.clone());
        children
    }

    /// The body an instance of `definition` gets from the overrides
    /// `given`: one already made for the same values, or a new one.
    fn instantiate(&self, definition: SymbolRef, given: FxHashMap<SymbolId, Given>) -> BodyId {
        let SymbolKind::Definition {
            scope, parameters, ..
        } = &self.design.symbol(definition).kind
        else {
            unreachable!("only a definition is instantiated");
        };
        let defaults = self.default_body(definition.file, *scope);
        if given.is_empty() {
            return defaults;
        }
        let body = self.new_body(Body {
            file: definition.file,
            scope: *scope,
            parent: None,
            given,
        });
        let key = |body: BodyId| -> Vec<Key> {
            let hir = &self.design[definition.file];
            (parameters.iter())
                .map(|&parameter| match hir[parameter].kind {
                    SymbolKind::Parameter(Parameter { is_type: true, .. }) => {
                        Key::Type(self.type_of(body, parameter))
                    }
                    _ => Key::Value(self.value(body, parameter)),
                })
                .collect()
        };
        if !self.instances.borrow().contains_key(&definition) {
            let entry = (key(defaults), defaults);
            self.instances.borrow_mut().insert(definition, vec![entry]);
        }
        let own = key(body);
        let mut instances = self.instances.borrow_mut();
        let bodies = instances.entry(definition).or_default();
        if let Some((_, same)) = bodies.iter().find(|(key, _)| *key == own) {
            return *same;
        }
        bodies.push((own, body));
        body
    }

    /// Collects what `scope`, built in `cx`'s body, holds.
    fn collect(&self, cx: Cx<'static>, scope: ScopeId, children: &mut Vec<Child>) {
        let hir = &self.design[cx.file];
        // An unnamed generate block is named for its construct's place.
        let mut constructs = 0;
        let named = |scope: ScopeId, constructs: usize| -> Box<str> {
            match hir[scope].owner {
                Some(owner) => hir[owner].name.text.clone(),
                None => format!("genblk{constructs}").into(),
            }
        };
        // An undecided construct is named for each arm that may be built.
        let undecided = |scopes: &[ScopeId], constructs: usize| -> Box<str> {
            let mut names: Vec<Box<str>> = Vec::new();
            for &scope in scopes {
                let name = named(scope, constructs);
                if !names.contains(&name) {
                    names.push(name);
                }
            }
            names.join(", ").into()
        };
        for member in &hir[scope].members {
            match member {
                // An unnamed instance has no place in the tree.
                Member::Declare(symbol) if hir[*symbol].name.text.is_empty() => {}
                Member::Declare(symbol) => match &hir[*symbol].kind {
                    SymbolKind::Instance(Instance { gate: Some(_), .. }) => children.push(Child {
                        name: hir[*symbol].name.text.clone(),
                        kind: ChildKind::Gate {
                            symbol: SymbolRef {
                                file: cx.file,
                                symbol: *symbol,
                            },
                        },
                    }),
                    SymbolKind::Instance(instance) => {
                        children.push(self.instance(cx, *symbol, instance));
                    }
                    _ => {}
                },
                Member::GenerateIf(arms) => {
                    constructs += 1;
                    match self.decide_if(cx, arms) {
                        Decision::Built(scope) => {
                            children.push(self.generate_block(cx, scope, named(scope, constructs)))
                        }
                        Decision::None => {}
                        Decision::Undecided(scopes) => children.push(Child {
                            name: undecided(&scopes, constructs),
                            kind: ChildKind::Undecided { scopes },
                        }),
                    }
                }
                Member::GenerateCase { selector, arms } => {
                    constructs += 1;
                    match self.decide_case(cx, *selector, arms) {
                        Decision::Built(scope) => {
                            children.push(self.generate_block(cx, scope, named(scope, constructs)))
                        }
                        Decision::None => {}
                        Decision::Undecided(scopes) => children.push(Child {
                            name: undecided(&scopes, constructs),
                            kind: ChildKind::Undecided { scopes },
                        }),
                    }
                }
                Member::GenerateFor { header, body } => {
                    constructs += 1;
                    let name = named(*body, constructs);
                    self.unroll(cx, header, *body, &name, children);
                }
                Member::Generate(scope) => {
                    constructs += 1;
                    children.push(self.generate_block(cx, *scope, named(*scope, constructs)));
                }
                _ => {}
            }
        }
    }

    fn generate_block(&self, cx: Cx<'static>, scope: ScopeId, name: Box<str>) -> Child {
        let body = self.new_body(Body {
            file: cx.file,
            scope,
            parent: Some(cx.body),
            given: FxHashMap::default(),
        });
        Child {
            name,
            kind: ChildKind::Block { body },
        }
    }

    /// The child `instance` makes, its symbol in `cx`'s file, which a bound
    /// instance's is, though its overrides are evaluated in `cx`'s body.
    fn instance(&self, cx: Cx<'static>, symbol: SymbolId, instance: &Instance) -> Child {
        let hir = &self.design[cx.file];
        let name = hir[symbol].name.text.clone();
        let symbol = SymbolRef {
            file: cx.file,
            symbol,
        };
        let Some(definition) = self.design.definition(&instance.definition.text) else {
            return Child {
                name,
                kind: ChildKind::Missing { symbol },
            };
        };
        let SymbolKind::Definition { parameters, .. } = &self.design.symbol(definition).kind else {
            unreachable!("a definition's name names a definition");
        };
        let declared = &self.design[definition.file];
        let mut given = FxHashMap::default();
        for (index, arg) in instance.parameters.iter().enumerate() {
            let (parameter, value) = match arg {
                Arg::Positional(Some(value)) => (parameters.get(index).copied(), *value),
                Arg::Named {
                    name,
                    value: Some(value),
                } => {
                    let parameter = (parameters.iter())
                        .find(|&&it| declared[it].name.text == name.text)
                        .copied();
                    (parameter, *value)
                }
                _ => continue,
            };
            // A local parameter cannot be overridden; `check` says so.
            if let Some(parameter) = parameter
                && let SymbolKind::Parameter(Parameter { local: false, .. }) =
                    declared[parameter].kind
            {
                let expr = value;
                given.insert(
                    parameter,
                    Given::Override {
                        body: cx.body,
                        file: cx.file,
                        expr,
                    },
                );
            }
        }
        let body = self.instantiate(definition, given);
        let dims = (instance.dims.iter())
            .map(|dim| match dim {
                Dim::Range(left, right) => Some((self.bound(cx, *left)?, self.bound(cx, *right)?)),
                Dim::Single(size) => {
                    let size = self.integer(cx, *size).filter(|&size| size > 0)?;
                    Some((0, size - 1))
                }
                Dim::Empty => None,
            })
            .collect();
        Child {
            name,
            kind: ChildKind::Instance {
                symbol,
                definition,
                body,
                dims,
            },
        }
    }

    /// Which arm of a generate `if` is built: the first whose condition
    /// holds, or the `else`. A condition with an `x` or `z` does not hold.
    fn decide_if(&self, cx: Cx, arms: &[GenerateArm]) -> Decision {
        for (index, arm) in arms.iter().enumerate() {
            let Some(&condition) = arm.conditions.first() else {
                return Decision::Built(arm.body);
            };
            match truth(&self.eval_self(cx, condition)) {
                Some(Bit::One) => return Decision::Built(arm.body),
                Some(_) => {}
                None => {
                    let scopes = arms[index..].iter().map(|arm| arm.body).collect();
                    return Decision::Undecided(scopes);
                }
            }
        }
        Decision::None
    }

    /// Which arm of a generate `case` is built: the first with a label equal
    /// to the selector, `x` and `z` included, or the `default`.
    fn decide_case(&self, cx: Cx, selector: ExprId, arms: &[GenerateArm]) -> Decision {
        let undecided = || Decision::Undecided(arms.iter().map(|arm| arm.body).collect());
        let labels = arms.iter().flat_map(|arm| &arm.conditions);
        let ty = labels.fold(self.self_type(cx, selector), |ty, &label| {
            merge(&ty, &self.self_type(cx, label))
        });
        let selector = self.eval_at(cx, selector, &ty);
        let mut default = None;
        for arm in arms {
            if arm.conditions.is_empty() {
                default = Some(arm.body);
            }
            for &label in &arm.conditions {
                let matched = match (&selector, self.eval_at(cx, label, &ty)) {
                    (Value::Bits(selector), Value::Bits(label)) => selector.case_eq(&label),
                    (Value::Real(selector), Value::Real(label)) => *selector == label,
                    (Value::String(selector), Value::String(label)) => *selector == label,
                    _ => return undecided(),
                };
                if matched {
                    return Decision::Built(arm.body);
                }
            }
        }
        match default {
            Some(body) => Decision::Built(body),
            None => Decision::None,
        }
    }

    /// Builds a block for each iteration of a generate loop, `name[i]`.
    fn unroll(
        &self,
        cx: Cx<'static>,
        header: &For,
        scope: ScopeId,
        name: &str,
        children: &mut Vec<Child>,
    ) {
        let undecided = |children: &mut Vec<Child>| {
            children.push(Child {
                name: name.into(),
                kind: ChildKind::Undecided {
                    scopes: vec![scope],
                },
            })
        };
        let Some((genvar, mut value)) = self.genvar(cx, header) else {
            return undecided(children);
        };
        let mut built = Vec::new();
        for _ in 0..ITERATIONS {
            if self.blocks.get() >= BLOCKS {
                return undecided(children);
            }
            self.blocks.set(self.blocks.get() + 1);
            // The header's own body holds the genvar's value for this
            // iteration, and the block is built inside it.
            let mut given = FxHashMap::default();
            given.insert(genvar, Given::Genvar(Value::Bits(value.clone())));
            let iteration = self.new_body(Body {
                file: cx.file,
                scope: header.scope,
                parent: Some(cx.body),
                given,
            });
            let at = self.cx(iteration);
            if let Some(condition) = header.condition {
                match truth(&self.eval_self(at, condition)) {
                    Some(Bit::One) => {}
                    Some(_) => {
                        children.extend(built);
                        return;
                    }
                    None => return undecided(children),
                }
            }
            let body = self.new_body(Body {
                file: cx.file,
                scope,
                parent: Some(iteration),
                given: FxHashMap::default(),
            });
            let index = value.to_i64().map_or("x".to_string(), |it| it.to_string());
            built.push(Child {
                name: format!("{name}[{index}]").into(),
                kind: ChildKind::Block { body },
            });
            let Some(next) = self.next(at, header, genvar, &value) else {
                return undecided(children);
            };
            value = next;
        }
        undecided(children)
    }

    /// The genvar a loop counts with and its first value: one its header
    /// declares, or one it assigns.
    fn genvar(&self, cx: Cx, header: &For) -> Option<(SymbolId, Bits)> {
        let hir = &self.design[cx.file];
        let integer = |value: Value| match self.convert(value, &Ty::INTEGER) {
            Value::Bits(bits) if !bits.has_unknown() => Some(bits),
            _ => None,
        };
        for member in &hir[header.scope].members {
            if let Member::Declare(symbol) = member
                && let SymbolKind::Genvar(Some(init)) = hir[*symbol].kind
            {
                return Some((*symbol, integer(self.eval_assign(cx, init, &Ty::INTEGER))?));
            }
        }
        let [init] = header.init.as_slice() else {
            return None;
        };
        let ExprKind::Assign {
            op: EQ, lhs, rhs, ..
        } = hir[*init].kind
        else {
            return None;
        };
        let Some(Resolution::Declared(at)) = self.resolution(cx, lhs) else {
            return None;
        };
        if at.file != cx.file || !matches!(hir[at.symbol].kind, SymbolKind::Genvar(_)) {
            return None;
        }
        Some((at.symbol, integer(self.eval_assign(cx, rhs, &Ty::INTEGER))?))
    }

    /// The genvar's value after the loop's step, evaluated in `cx`, the
    /// iteration that has `value`.
    fn next(&self, cx: Cx, header: &For, genvar: SymbolId, value: &Bits) -> Option<Bits> {
        let hir = &self.design[cx.file];
        let [step] = header.step.as_slice() else {
            return None;
        };
        let is_genvar = |expr: ExprId| matches!(self.resolution(cx, expr), Some(Resolution::Declared(at)) if at.file == cx.file && at.symbol == genvar);
        let one = Bits::from_u64(32, true, 1);
        let next = match hir[*step].kind {
            ExprKind::Unary { op, operand } | ExprKind::Postfix { op, operand }
                if is_genvar(operand) =>
            {
                match op {
                    PLUS_PLUS => value.add(&one),
                    MINUS_MINUS => value.sub(&one),
                    _ => return None,
                }
            }
            ExprKind::Assign { op, lhs, rhs, .. } if is_genvar(lhs) => {
                let rhs = match op {
                    EQ => self.eval_assign(cx, rhs, &Ty::INTEGER),
                    _ => {
                        let op = super::expr::compound(op)?;
                        let rhs = self.eval_assign(cx, rhs, &Ty::INTEGER);
                        super::expr::apply(op, Value::Bits(value.clone()), rhs)
                    }
                };
                match self.convert(rhs, &Ty::INTEGER) {
                    Value::Bits(bits) => bits,
                    _ => return None,
                }
            }
            _ => return None,
        };
        (!next.has_unknown()).then_some(next)
    }

    pub(super) fn dump(&self, top: BodyId) -> String {
        let mut out = String::new();
        let (file, scope) = self.scope(top);
        let name = match self.design[file][scope].owner {
            Some(owner) => self.design[file][owner].name.text.clone(),
            None => "$unit".into(),
        };
        writeln!(out, "{name}").unwrap();
        self.dump_children(top, 1, &mut out);
        out
    }

    fn dump_children(&self, body: BodyId, depth: usize, out: &mut String) {
        let indent = "  ".repeat(depth);
        if depth > DEPTH {
            writeln!(out, "{indent}...").unwrap();
            return;
        }
        for child in self.children(body).iter() {
            match &child.kind {
                ChildKind::Instance {
                    definition,
                    body,
                    dims,
                    ..
                } => {
                    let dims = match dims {
                        None => " [?]".to_string(),
                        Some(dims) => (dims.iter())
                            .map(|(left, right)| format!(" [{left}:{right}]"))
                            .collect(),
                    };
                    let defined = &self.design.symbol(*definition).name.text;
                    let overrides = self.overrides(*definition, *body);
                    writeln!(out, "{indent}{}{dims}: {defined}{overrides}", child.name).unwrap();
                    self.dump_children(*body, depth + 1, out);
                }
                ChildKind::Missing { symbol } => {
                    let SymbolKind::Instance(instance) = &self.design.symbol(*symbol).kind else {
                        continue;
                    };
                    let defined = &instance.definition.text;
                    writeln!(out, "{indent}{}: {defined}, declared nowhere", child.name).unwrap();
                }
                ChildKind::Gate { symbol } => {
                    let SymbolKind::Instance(instance) = &self.design.symbol(*symbol).kind else {
                        continue;
                    };
                    writeln!(out, "{indent}{}: {}", child.name, instance.definition.text).unwrap();
                }
                ChildKind::Block { body } => {
                    writeln!(out, "{indent}{}", child.name).unwrap();
                    self.dump_children(*body, depth + 1, out);
                }
                ChildKind::Undecided { .. } => {
                    writeln!(out, "{indent}{}: undecided", child.name).unwrap();
                }
            }
        }
    }

    /// ` #(name = value, …)` for each parameter whose value in `body`
    /// differs from its default.
    fn overrides(&self, definition: SymbolRef, body: BodyId) -> String {
        let SymbolKind::Definition {
            scope, parameters, ..
        } = &self.design.symbol(definition).kind
        else {
            return String::new();
        };
        let defaults = self.default_body(definition.file, *scope);
        if defaults == body {
            return String::new();
        }
        let hir = &self.design[definition.file];
        let mut changed = Vec::new();
        for &parameter in parameters {
            let name = &hir[parameter].name.text;
            match hir[parameter].kind {
                SymbolKind::Parameter(Parameter { is_type: true, .. }) => {
                    let ty = self.type_of(body, parameter);
                    if ty != self.type_of(defaults, parameter) {
                        changed.push(format!("{name} = {ty}"));
                    }
                }
                _ => {
                    let value = self.value(body, parameter);
                    if value != self.value(defaults, parameter) {
                        changed.push(format!("{name} = {value}"));
                    }
                }
            }
        }
        match changed.is_empty() {
            true => String::new(),
            false => format!(" #({})", changed.join(", ")),
        }
    }
}

/// Which arm of a generate `if` or `case` is built.
enum Decision {
    Built(ScopeId),
    /// No arm: an `if` without `else` whose conditions all fail.
    None,
    Undecided(Vec<ScopeId>),
}
