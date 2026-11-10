//! The instance tree: what each body holds once built. An instance's body is
//! its definition's with the parameters its overrides decide, one for each
//! set of values; a generate construct builds the arm its condition picks,
//! or a block per iteration of its loop.

use std::fmt::Write;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use astli_syntax::SyntaxKind::*;
use rustc_hash::{FxHashMap, FxHashSet};

use super::expr::{merge, truth};
use super::{Body, BodyId, Cx, Given, Queries, TopError, given_literal};
use crate::design::{FileId, SymbolRef};
use crate::hir::*;
use crate::resolve::Resolution;
use crate::types::Ty;
use crate::value::Literal;
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
/// or the type a type parameter names, then the body of the interface each
/// interface port is connected to, then what `defparam`s and `bind`s pass
/// down through it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Key {
    Value(Value),
    Type(Ty),
    Interface(Option<BodyId>),
    Deferred(Vec<Deferred>),
}

/// A `defparam` or a chosen `bind` on its way down the instance tree to
/// what its path names.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Deferred {
    /// What is left of the path, from a child of the body holding it.
    steps: Vec<Step>,
    action: Action,
}

/// One name of a path.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Step {
    /// An instance, a generate block, or a `defparam`'s parameter.
    symbol: SymbolRef,
    /// The iteration of a loop's block, or an element of an array of
    /// instances.
    index: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
enum Action {
    /// A parameter's value, written in `file` and evaluated in `body`, the
    /// body holding the `defparam`.
    Defparam {
        body: BodyId,
        file: FileId,
        expr: ExprId,
    },
    /// An instance added at the end of the one the path names, if that is
    /// of `definition` where the `bind` names one.
    Bind {
        instance: SymbolRef,
        definition: Option<Box<str>>,
    },
}

impl Queries {
    pub(super) fn children(&self, body: BodyId) -> Arc<[Child]> {
        if let Some(children) = self.children.get(&body) {
            return children;
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
                        kept: None,
                    };
                    children.push(self.instance(cx, bound.instance.symbol, instance));
                }
            }
            // A chosen `bind` whose path ends at this instance.
            for deferred in self.deferred(body).iter() {
                let Action::Bind {
                    instance: at,
                    definition,
                } = &deferred.action
                else {
                    continue;
                };
                let named = definition.as_ref();
                if deferred.steps.is_empty()
                    && named.is_none_or(|named| *named == hir[owner].name.text)
                    && let SymbolKind::Instance(instance) = &self.design.symbol(*at).kind
                {
                    let cx = Cx {
                        body,
                        file: at.file,
                        frame: None,
                        kept: None,
                    };
                    children.push(self.instance(cx, at.symbol, instance));
                }
            }
        }
        // Another thread may have built them meanwhile, with bodies of its
        // own: the first kept is the one every thread sees.
        self.children.keep(body, children.into())
    }

    /// The body of `definition` as the top: its defaults, but for what the
    /// `defparam`s and chosen `bind`s whose paths start at it change.
    pub(super) fn top(&self, definition: SymbolRef) -> Option<BodyId> {
        self.top_set(definition, Vec::new())
    }

    /// The body of `definition` as the top, with each of `parameters` set
    /// by name to the value its text spells.
    pub(super) fn top_with(
        &self,
        definition: SymbolRef,
        parameters: &[(impl AsRef<str>, impl AsRef<str>)],
    ) -> Result<BodyId, TopError> {
        let SymbolKind::Definition {
            parameters: declared,
            ..
        } = &self.design.symbol(definition).kind
        else {
            return Err(TopError::NoTop);
        };
        let hir = &self.design[definition.file];
        let mut set = Vec::new();
        for (name, text) in parameters {
            let (name, text) = (name.as_ref(), text.as_ref());
            let Some(&parameter) = (declared.iter()).find(|&&it| *hir[it].name.text == *name)
            else {
                return Err(TopError::NoParameter(name.into()));
            };
            match hir[parameter].kind {
                SymbolKind::Parameter(Parameter { local: true, .. }) => {
                    return Err(TopError::Local(name.into()));
                }
                SymbolKind::Parameter(Parameter { is_type: true, .. }) => {
                    return Err(TopError::Type(name.into()));
                }
                _ => {}
            }
            let Some(literal) = given_literal(text) else {
                return Err(TopError::NotAValue(name.into(), text.into()));
            };
            set.push((parameter, text.to_string(), literal));
        }
        self.top_set(definition, set).ok_or(TopError::NoTop)
    }

    /// The body of `definition` as the top, with the parameters `set` given
    /// the literal each is set to, by the text that spells it.
    fn top_set(
        &self,
        definition: SymbolRef,
        set: Vec<(SymbolId, String, Literal)>,
    ) -> Option<BodyId> {
        let SymbolKind::Definition {
            scope, parameters, ..
        } = &self.design.symbol(definition).kind
        else {
            return None;
        };
        let absolute = self.absolute().get(&definition);
        if absolute.is_none() && set.is_empty() {
            return Some(self.default_body(definition.file, *scope));
        }
        let key = (
            definition,
            (set.iter())
                .map(|(at, text, _)| (*at, text.clone()))
                .collect(),
        );
        if let Some(body) = self.tops.get(&key) {
            return Some(body);
        }
        // A `defparam` comes after, and wins, as it would over an instance's
        // override.
        let mut given: FxHashMap<SymbolId, Given> = (set.into_iter())
            .map(|(at, _, literal)| (at, Given::Literal(literal)))
            .collect();
        let mut deferred = Vec::new();
        for passing in absolute.into_iter().flatten() {
            match (&passing.action, passing.steps.as_slice()) {
                (Action::Defparam { body, file, expr }, [parameter]) => {
                    if self.overridable(definition, parameters, parameter) {
                        let (body, file, expr) = (*body, *file, *expr);
                        given.insert(
                            parameter.symbol.symbol,
                            Given::Override { body, file, expr },
                        );
                    }
                }
                (Action::Defparam { .. }, []) => {}
                _ => deferred.push(passing.clone()),
            }
        }
        let mut tops = self.tops.shard(&key);
        let body = tops.entry(key).or_insert_with(|| {
            self.new_body(Body {
                file: definition.file,
                scope: *scope,
                parent: None,
                given,
                deferred,
            })
        });
        Some(*body)
    }

    /// Whether `parameter`, a path's last step, is one of `definition`'s
    /// `parameters` that an override may change.
    fn overridable(
        &self,
        definition: SymbolRef,
        parameters: &[SymbolId],
        parameter: &Step,
    ) -> bool {
        parameter.symbol.file == definition.file
            && parameter.index.is_none()
            && parameters.contains(&parameter.symbol.symbol)
            && matches!(
                self.design.symbol(parameter.symbol).kind,
                SymbolKind::Parameter(Parameter { local: false, .. })
            )
    }

    /// The `defparam`s and chosen `bind`s on their way down through `body`:
    /// those the body above passes it, and those written in its scope, or
    /// in a generate block there, whose path starts at an instance or a
    /// generate block its scope declares.
    fn deferred(&self, body: BodyId) -> Arc<[Deferred]> {
        self.deferred.get(body, Arc::from([]), || {
            let mut deferred = self.bodies.get(body.index()).deferred.clone();
            let cx = self.cx(body);
            let (file, scope) = self.scope(body);
            let mut written = Vec::new();
            self.written(cx, scope, &mut written);
            for (steps, action) in written {
                let head = steps[0].symbol;
                let declared = head.file == file && self.design.home(head) == scope;
                if declared
                    && matches!(
                        self.design.symbol(head).kind,
                        SymbolKind::Instance(_) | SymbolKind::Block(_)
                    )
                {
                    deferred.push(Deferred { steps, action });
                }
            }
            deferred.into()
        })
    }

    /// The `defparam`s and chosen `bind`s whose path starts at a top, by
    /// the top's definition: those a file's top level holds, or a
    /// definition elaborated, evaluated where the holder has its defaults.
    fn absolute(&self) -> &FxHashMap<SymbolRef, Vec<Deferred>> {
        self.absolute.get_or_init(|| {
            let mut absolute: FxHashMap<SymbolRef, Vec<Deferred>> = FxHashMap::default();
            for (file, hir) in self.design.files() {
                for (scope, held) in hir.scopes() {
                    let holds = match (held.kind, held.owner) {
                        (ScopeKind::File, _) => true,
                        (ScopeKind::Definition, Some(owner)) => {
                            let owner = SymbolRef {
                                file,
                                symbol: owner,
                            };
                            (self.elaborated.as_ref()).is_none_or(|it| it.contains(&owner))
                        }
                        _ => false,
                    };
                    if !holds || !self.defers(file, scope) {
                        continue;
                    }
                    let mut written = Vec::new();
                    self.written(self.cx(self.default_body(file, scope)), scope, &mut written);
                    for (steps, action) in written {
                        let head = steps[0];
                        let top = matches!(
                            self.design.symbol(head.symbol).kind,
                            SymbolKind::Definition { keyword, .. } if keyword != PACKAGE_KW
                        );
                        if top && head.index.is_none() {
                            let steps = steps[1..].to_vec();
                            let deferred = Deferred { steps, action };
                            absolute.entry(head.symbol).or_default().push(deferred);
                        }
                    }
                }
            }
            absolute
        })
    }

    /// Whether `scope`, or a scope inside it, holds a `defparam` or a
    /// chosen `bind`.
    fn defers(&self, file: FileId, scope: ScopeId) -> bool {
        let defers = self.facts[file.index()].defers.get_or_init(|| {
            let hir = &self.design[file];
            let mut defers = FxHashSet::default();
            for (id, held) in hir.scopes() {
                let writes = held.members.iter().any(|member| match member {
                    Member::Defparam(_) => true,
                    Member::Bind(bind) => !bind.paths.is_empty(),
                    _ => false,
                });
                let mut around = writes.then_some(id);
                while let Some(id) = around
                    && defers.insert(id)
                {
                    around = hir[id].parent;
                }
            }
            defers
        });
        defers.contains(&scope)
    }

    /// The `defparam`s and chosen `bind`s written in `scope`, built in
    /// `cx`'s body, and in the generate blocks there that are built but a
    /// loop's, each with its path. A `defparam` in a generate block cannot
    /// reach out of it, but a `bind` can.
    fn written(&self, cx: Cx<'static>, scope: ScopeId, written: &mut Vec<(Vec<Step>, Action)>) {
        if !self.defers(cx.file, scope) {
            return;
        }
        let hir = &self.design[cx.file];
        for member in &hir[scope].members {
            match member {
                Member::Defparam(assignments) => {
                    for &assignment in assignments {
                        if let ExprKind::Assign { lhs, rhs, .. } = hir[assignment].kind
                            && let Some(steps) = self.path(cx, lhs)
                        {
                            let action = Action::Defparam {
                                body: cx.body,
                                file: cx.file,
                                expr: rhs,
                            };
                            written.push((steps, action));
                        }
                    }
                }
                // A target that names no definition is an instance.
                Member::Bind(bind)
                    if bind.chosen
                        || (bind.target.as_ref())
                            .is_some_and(|it| self.design.definition(&it.text).is_none()) =>
                {
                    let definition = match bind.chosen {
                        true => bind.target.as_ref().map(|it| it.text.clone()),
                        false => None,
                    };
                    for &path in &bind.paths {
                        let Some(steps) = self.path(cx, path) else {
                            continue;
                        };
                        for &instance in &bind.instances {
                            let action = Action::Bind {
                                instance: SymbolRef {
                                    file: cx.file,
                                    symbol: instance,
                                },
                                definition: definition.clone(),
                            };
                            written.push((steps.clone(), action));
                        }
                    }
                }
                // The block's own body is not built yet, so its conditions
                // are evaluated in `cx`'s.
                Member::GenerateIf(arms) => {
                    if let Decision::Built(scope) = self.decide_if(cx, arms) {
                        self.written(cx, scope, written);
                    }
                }
                Member::GenerateCase { selector, arms } => {
                    if let Decision::Built(scope) = self.decide_case(cx, *selector, arms) {
                        self.written(cx, scope, written);
                    }
                }
                Member::Generate(scope) => self.written(cx, *scope, written),
                _ => {}
            }
        }
    }

    /// The steps of the hierarchical name `expr`, written in `cx`: what
    /// each name in it refers to, and the index after it. `None` if a name
    /// is not resolved, or an index not known.
    fn path(&self, cx: Cx, expr: ExprId) -> Option<Vec<Step>> {
        let hir = &self.design[cx.file];
        let mut steps = match hir[expr].kind {
            ExprKind::Name(_) => Vec::new(),
            ExprKind::Member { base, .. } => self.path(cx, base)?,
            ExprKind::Select {
                base,
                index,
                range: None,
            } => {
                let mut steps = self.path(cx, base)?;
                let last = steps.last_mut()?;
                if last.index.is_some() {
                    return None;
                }
                last.index = Some(self.integer(cx, index)?);
                return Some(steps);
            }
            _ => return None,
        };
        let Some(Resolution::Declared(symbol)) = self.resolution(cx, expr) else {
            return None;
        };
        steps.push(Step {
            symbol,
            index: None,
        });
        Some(steps)
    }

    /// What `cx`'s body passes down to the generate block `scope` it
    /// builds, `index` the loop's iteration it is.
    fn passed(&self, cx: Cx, scope: ScopeId, index: Option<i64>) -> Vec<Deferred> {
        let Some(owner) = self.design[cx.file][scope].owner else {
            return Vec::new();
        };
        let block = SymbolRef {
            file: cx.file,
            symbol: owner,
        };
        (self.deferred(cx.body).iter())
            .filter_map(|passing| {
                let (head, rest) = passing.steps.split_first()?;
                // A path ends at an instance, or a parameter of one.
                let passes = head.symbol == block && head.index == index && !rest.is_empty();
                passes.then(|| Deferred {
                    steps: rest.to_vec(),
                    action: passing.action.clone(),
                })
            })
            .collect()
    }

    /// The body an instance of `definition` gets from the overrides and
    /// interfaces `given`, and what `defparam`s and `bind`s pass down
    /// through it: one already made for the same, or a new one.
    fn instantiate(
        &self,
        definition: SymbolRef,
        given: FxHashMap<SymbolId, Given>,
        deferred: Vec<Deferred>,
    ) -> BodyId {
        let SymbolKind::Definition {
            scope,
            parameters,
            ports,
            ..
        } = &self.design.symbol(definition).kind
        else {
            unreachable!("only a definition is instantiated");
        };
        let defaults = self.default_body(definition.file, *scope);
        if given.is_empty() && deferred.is_empty() {
            return defaults;
        }
        let body = self.new_body(Body {
            file: definition.file,
            scope: *scope,
            parent: None,
            given,
            deferred,
        });
        let interfaces: Vec<SymbolId> = (ports.iter().copied())
            .filter(|&symbol| {
                let port = SymbolRef {
                    file: definition.file,
                    symbol,
                };
                self.interface_port(port).is_some()
            })
            .collect();
        let key = |body: BodyId| -> Vec<Key> {
            let hir = &self.design[definition.file];
            let parameters = parameters
                .iter()
                .map(|&parameter| match hir[parameter].kind {
                    SymbolKind::Parameter(Parameter { is_type: true, .. }) => {
                        Key::Type(self.type_of(body, parameter))
                    }
                    _ => Key::Value(self.value(body, parameter)),
                });
            let interfaces = interfaces.iter().map(|&port| match self.given(body, port) {
                Some(Given::Interface(interface)) => Key::Interface(Some(interface)),
                _ => Key::Interface(None),
            });
            let deferred = self.bodies.get(body.index()).deferred.clone();
            let deferred = (!deferred.is_empty()).then_some(Key::Deferred(deferred));
            parameters.chain(interfaces).chain(deferred).collect()
        };
        // The keys are computed before the lock is taken, since computing
        // one asks queries that may come back here.
        let first = (self.instances.get(&definition).is_none()).then(|| (key(defaults), defaults));
        let own = key(body);
        let mut instances = self.instances.shard(&definition);
        let bodies = (instances.entry(definition)).or_insert_with(|| first.into_iter().collect());
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
            deferred: self.passed(cx, scope, None),
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
        // An interface port sees the interface instance connected to it.
        for (port, found) in self.connections(cx, symbol, instance, definition) {
            let port = SymbolRef {
                file: definition.file,
                symbol: port,
            };
            let Some(wanted) = self.interface_port(port) else {
                continue;
            };
            let interface = match found {
                Connected::Expr(expr) => self.interface(cx, expr),
                Connected::Name(Resolution::Declared(at)) => self.interface_at(cx, at),
                Connected::Name(_) => None,
            };
            // An instance of another interface than the port names is
            // connected in error, and tells the port nothing.
            if let Some(interface) = interface
                && wanted.is_none_or(|wanted| Some(wanted) == self.definition_of(interface))
            {
                given.insert(port.symbol, Given::Interface(interface));
            }
        }
        // What a path names below the instance passes down to its body, but
        // a `defparam` of its own parameter, which wins over the instance's
        // override. One element of an array has the body the others have,
        // so none is told apart.
        let mut deferred = Vec::new();
        for passing in self.deferred(cx.body).iter() {
            let Some((head, rest)) = passing.steps.split_first() else {
                continue;
            };
            if head.symbol != symbol || head.index.is_some() {
                continue;
            }
            match (&passing.action, rest) {
                (Action::Defparam { body, file, expr }, [parameter]) => {
                    if self.overridable(definition, parameters, parameter) {
                        let (body, file, expr) = (*body, *file, *expr);
                        given.insert(
                            parameter.symbol.symbol,
                            Given::Override { body, file, expr },
                        );
                    }
                }
                (Action::Defparam { .. }, []) => {}
                _ => deferred.push(Deferred {
                    steps: rest.to_vec(),
                    action: passing.action.clone(),
                }),
            }
        }
        let body = self.instantiate(definition, given, deferred);
        let dims = (instance.dims.iter())
            .map(|dim| match dim {
                Dim::Range(left, right) => Some((self.bound(cx, *left)?, self.bound(cx, *right)?)),
                Dim::Single(size) => {
                    let size = self.integer(cx, *size).filter(|&size| size > 0)?;
                    Some((0, size - 1))
                }
                Dim::Empty | Dim::Wildcard => None,
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

    /// What each port of `definition` that `instance`, the symbol `symbol`,
    /// connects is connected to: by position, by name, by `.name` or by
    /// `.*`.
    fn connections(
        &self,
        cx: Cx,
        symbol: SymbolRef,
        instance: &Instance,
        definition: SymbolRef,
    ) -> Vec<(SymbolId, Connected)> {
        let SymbolKind::Definition { ports, .. } = &self.design.symbol(definition).kind else {
            unreachable!("a definition's name names a definition");
        };
        let declared = &self.design[definition.file];
        let names = self.design.names(cx.file);
        let mut connected = Vec::new();
        let mut position = 0;
        for arg in &instance.connections {
            let (port, found) = match arg {
                Arg::Positional(value) => {
                    position += 1;
                    (ports.get(position - 1).copied(), value.map(Connected::Expr))
                }
                Arg::Named { name, value } => {
                    let port = (ports.iter()).find(|&&it| declared[it].name.text == name.text);
                    (port.copied(), value.map(Connected::Expr))
                }
                Arg::Implicit(name) => {
                    let port = (ports.iter()).find(|&&it| declared[it].name.text == name.text);
                    let found = names.loose_at(name.span).map(Connected::Name);
                    (port.copied(), found)
                }
                Arg::Wildcard(_) => continue,
            };
            if let (Some(port), Some(found)) = (port, found) {
                connected.push((port, found));
            }
        }
        for &(port, found) in names.wildcard(symbol.symbol) {
            connected.push((port.symbol, Connected::Name(found)));
        }
        connected
    }

    /// What each interface port of the instance `at`, built in `holder`, is
    /// connected to.
    pub(super) fn interface_connections(
        &self,
        holder: BodyId,
        at: SymbolRef,
    ) -> Vec<(SymbolId, Connected)> {
        let SymbolKind::Instance(instance) = &self.design.symbol(at).kind else {
            return Vec::new();
        };
        let Some(definition) = self.design.definition(&instance.definition.text) else {
            return Vec::new();
        };
        let cx = Cx {
            body: holder,
            file: at.file,
            frame: None,
            kept: None,
        };
        let mut connected = self.connections(cx, at, instance, definition);
        connected.retain(|&(port, _)| {
            let port = SymbolRef {
                file: definition.file,
                symbol: port,
            };
            self.interface_port(port).is_some()
        });
        connected
    }

    /// Whether `port` is an interface port, and the interface it names:
    /// `None` for a generic one, `interface bus`, which takes any.
    fn interface_port(&self, port: SymbolRef) -> Option<Option<SymbolRef>> {
        let SymbolKind::Port(Port {
            ty:
                Type {
                    kind: TypeKind::Named(named),
                    ..
                },
            ..
        }) = self.design.symbol(port).kind
        else {
            return None;
        };
        let hir = &self.design[port.file];
        // `bus_if.mp bus` names the interface `bus_if`.
        let named = match hir[named].kind {
            ExprKind::Member { base, .. } => base,
            _ => named,
        };
        if let ExprKind::Keyword(INTERFACE_KW) = hir[named].kind {
            return Some(None);
        }
        match self.design.names(port.file).expr(named) {
            Some(Resolution::Declared(at)) => match self.design.symbol(at).kind {
                SymbolKind::Definition {
                    keyword: INTERFACE_KW,
                    ..
                } => Some(Some(at)),
                _ => None,
            },
            _ => None,
        }
    }

    /// The body of the interface instance `expr`, written in `cx`, names:
    /// an instance of an interface, an element of an array of them, a
    /// modport of one, or an interface port of `cx`'s body.
    pub(super) fn interface(&self, cx: Cx, expr: ExprId) -> Option<BodyId> {
        let hir = &self.design[cx.file];
        let mut head = expr;
        loop {
            let modport = |expr: ExprId| match self.resolution(cx, expr) {
                Some(Resolution::Declared(at)) => {
                    matches!(self.design.symbol(at).kind, SymbolKind::Modport(_))
                }
                _ => false,
            };
            head = match hir[head].kind {
                // One body is every element's.
                ExprKind::Select { base, .. } => base,
                ExprKind::Member { base, .. } if modport(head) => base,
                _ => break,
            };
        }
        match self.resolution(cx, head)? {
            Resolution::Declared(at) => self.interface_at(cx, at),
            _ => None,
        }
    }

    /// The body of the interface instance `at` is, or an interface port of
    /// `cx`'s body is connected to.
    fn interface_at(&self, cx: Cx, at: SymbolRef) -> Option<BodyId> {
        match &self.design.symbol(at).kind {
            SymbolKind::Port(_) => match self.given(self.owner(cx, at)?, at.symbol)? {
                Given::Interface(interface) => Some(interface),
                _ => None,
            },
            SymbolKind::Instance(_) => {
                let (definition, body) = self.held(self.owner(cx, at)?, at)?;
                match self.design.symbol(definition).kind {
                    SymbolKind::Definition {
                        keyword: INTERFACE_KW,
                        ..
                    } => Some(body),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The definition and body of the instance `at`, built in `holder`.
    fn held(&self, holder: BodyId, at: SymbolRef) -> Option<(SymbolRef, BodyId)> {
        let SymbolKind::Instance(instance @ Instance { gate: None, .. }) =
            &self.design.symbol(at).kind
        else {
            return None;
        };
        self.held.get((holder, at), None, || {
            let cx = Cx {
                body: holder,
                file: at.file,
                frame: None,
                kept: None,
            };
            match self.instance(cx, at.symbol, instance).kind {
                ChildKind::Instance {
                    definition, body, ..
                } => Some((definition, body)),
                _ => None,
            }
        })
    }

    /// The body `expr`, written in `cx`, names as a scope of the hierarchy:
    /// an instance's, an element's of an array of them, or a modport's of
    /// one; a generate block's, or an iteration's of a loop's; or that of
    /// the interface instance an interface port is connected to. `None`
    /// where it is not built, or sema cannot tell which is.
    pub(super) fn hierarchy(&self, cx: Cx, expr: ExprId) -> Option<BodyId> {
        let hir = &self.design[cx.file];
        let modport = |expr: ExprId| match self.resolution(cx, expr) {
            Some(Resolution::Declared(at)) => {
                matches!(self.design.symbol(at).kind, SymbolKind::Modport(_))
            }
            _ => false,
        };
        // The index right after the name picks a loop's iteration; an
        // array's elements share one body.
        let mut head = expr;
        let mut index = None;
        loop {
            head = match hir[head].kind {
                ExprKind::Select {
                    base,
                    index: at,
                    range,
                } => {
                    index = range.is_none().then_some(at);
                    base
                }
                ExprKind::Member { base, .. } if modport(head) => base,
                _ => break,
            };
        }
        let Some(Resolution::Declared(at)) = self.resolution(cx, head) else {
            return None;
        };
        let holder = match hir[head].kind {
            ExprKind::Member { base, .. } => self.hierarchy(cx, base)?,
            _ => self.owner(cx, at)?,
        };
        match &self.design.symbol(at).kind {
            SymbolKind::Port(_) => match self.given(holder, at.symbol)? {
                Given::Interface(interface) => Some(interface),
                _ => None,
            },
            SymbolKind::Instance(_) => Some(self.held(holder, at)?.1),
            SymbolKind::Block(scope) => {
                let name = &self.design.symbol(at).name.text;
                let name: Box<str> = match index {
                    Some(index) => format!("{name}[{}]", self.integer(cx, index)?).into(),
                    None => name.clone(),
                };
                let children = self.children(holder);
                children.iter().find_map(|child| match child.kind {
                    ChildKind::Block { body }
                        if child.name == name && self.scope(body) == (at.file, *scope) =>
                    {
                        Some(body)
                    }
                    _ => None,
                })
            }
            _ => None,
        }
    }

    /// The definition `body` is a body of; `None` for a generate block's or
    /// a file's.
    fn definition_of(&self, body: BodyId) -> Option<SymbolRef> {
        let (file, scope) = self.scope(body);
        let owner = self.design[file][scope].owner?;
        match self.design[file][owner].kind {
            SymbolKind::Definition { .. } => Some(SymbolRef {
                file,
                symbol: owner,
            }),
            _ => None,
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
            if self.blocks.fetch_add(1, Ordering::Relaxed) >= BLOCKS {
                return undecided(children);
            }
            // The header's own body holds the genvar's value for this
            // iteration, and the block is built inside it.
            let mut given = FxHashMap::default();
            given.insert(genvar, Given::Genvar(Value::Bits(value.clone())));
            let iteration = self.new_body(Body {
                file: cx.file,
                scope: header.scope,
                parent: Some(cx.body),
                given,
                deferred: Vec::new(),
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
                deferred: self.passed(cx, scope, value.to_i64()),
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
            Some(owner) => {
                let definition = SymbolRef {
                    file,
                    symbol: owner,
                };
                let overrides = self.overrides(definition, top);
                format!("{}{overrides}", self.design[file][owner].name.text)
            }
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
                    let defined = self.design.symbol(*definition);
                    let overrides = self.overrides(*definition, *body);
                    // Its body is compiled apart, so nothing below it is known.
                    let apart = match defined.kind {
                        SymbolKind::Definition {
                            prototype: true, ..
                        } => ", extern",
                        _ => "",
                    };
                    let defined = &defined.name.text;
                    writeln!(
                        out,
                        "{indent}{}{dims}: {defined}{overrides}{apart}",
                        child.name
                    )
                    .unwrap();
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

/// What a port is connected to: an expression, or what the name of a `.name`
/// or a `.*` refers to.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Connected {
    Expr(ExprId),
    Name(Resolution),
}

/// Which arm of a generate `if` or `case` is built.
enum Decision {
    Built(ScopeId),
    /// No arm: an `if` without `else` whose conditions all fail.
    None,
    Undecided(Vec<ScopeId>),
}
