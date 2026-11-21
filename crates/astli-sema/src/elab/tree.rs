//! The instance tree: what each body holds once built. An instance's body is
//! its definition's with the parameters its overrides decide, one for each
//! set of values; a generate construct builds the arm its condition picks,
//! or a block per iteration of its loop.

use std::fmt::Write;
use std::sync::Arc;

use astli_syntax::SyntaxKind::*;
use rustc_hash::{FxHashMap, FxHashSet};

use super::expr::{merge, truth};
use super::{Blocks, Body, BodyId, Cx, Elaboration, Given, Run};
use crate::bind::{self, Bound};
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
        /// Whether `definition` is surely the one built: not where more
        /// than one file declares its name, as which a compiler keeps is a
        /// guess, so nothing below the instance is sure to be built.
        certain: bool,
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

/// What tells two bodies of one definition apart: each parameter's value
/// and type, or the type a type parameter names, then the body of the
/// interface each interface port is connected to, then what `defparam`s and
/// `bind`s pass down through it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Key {
    /// A parameter declared with neither type nor range takes its value's
    /// type, so two enums' members of one value build two bodies. Of an
    /// unknown value it also says whether the body is given one: a body
    /// given what sema cannot evaluate is not the body given nothing, which
    /// a parameter without a default would need.
    Value(Value, Ty, bool),
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

impl Elaboration {
    /// What `body` holds once built: its instances, and its generate blocks
    /// that are built, in the order written.
    pub fn children(&self, body: BodyId) -> Arc<[Child]> {
        self.children
            .get(body, Arc::new([]), || self.build_children(body))
    }

    fn build_children(&self, body: BodyId) -> Arc<[Child]> {
        let mut children = Vec::new();
        let (file, scope) = self.scope(body);
        self.collect(self.cx(body), scope, &mut children);
        // What a `bind` adds stands at the end of a definition's body.
        let hir = &self.design[file];
        if let Some(owner) = hir[scope].owner
            && let SymbolKind::Definition { .. } = hir[owner].kind
        {
            // What a `bind` names is a design element, which a definition
            // declared inside another is not.
            let named = (hir[owner].scope == hir.root()).then_some(&hir[owner].name.text);
            for bound in named.map_or(&[][..], |name| self.design.binds(name)) {
                let applies = bound.holder.is_none_or(|holder| {
                    (self.elaborated.as_ref()).is_none_or(|elaborated| elaborated.contains(&holder))
                });
                if applies
                    && let SymbolKind::Instance(instance) = &self.design.symbol(bound.instance).kind
                {
                    let cx = Cx::new(body, bound.instance.file);
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
                if deferred.steps.is_empty()
                    && definition.as_ref().is_none_or(|it| Some(it) == named)
                    && let SymbolKind::Instance(instance) = &self.design.symbol(*at).kind
                {
                    let cx = Cx::new(body, at.file);
                    children.push(self.instance(cx, at.symbol, instance));
                }
            }
        }
        // What `child` asks of each instance is what building it gave.
        for child in &children {
            if let ChildKind::Instance {
                symbol,
                definition,
                body: built,
                ..
            } = child.kind
            {
                self.held
                    .get((body, symbol), None, || Some((definition, built)));
            }
        }
        children.into()
    }

    /// The body of the module, interface or program `name` as a top: its
    /// parameters at their defaults, and what the `defparam`s and `bind`s
    /// whose paths start at it change.
    pub fn top(&self, name: &str) -> Option<BodyId> {
        self.top_set(self.design.definition(name)?, Vec::new())
    }

    /// The body of `definition` as the top, with the parameters `set` given
    /// the literal each is set to, by the text that spells it.
    pub(super) fn top_set(
        &self,
        definition: SymbolRef,
        set: Vec<(SymbolId, String, Literal)>,
    ) -> Option<BodyId> {
        let SymbolKind::Definition { scope, .. } = &self.design.symbol(definition).kind else {
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
        let passing = (absolute.into_iter().flatten())
            .map(|passing| (&passing.action, passing.steps.as_slice()));
        let passed = self.split_passing(definition, passing, &mut given);
        let mut tops = self.tops.shard(&key);
        let body = tops.entry(key).or_insert_with(|| {
            self.new_body(Body {
                file: definition.file,
                scope: *scope,
                parent: None,
                given,
                passed,
            })
        });
        Some(*body)
    }

    /// What the `defparam`s and `bind`s `passing`, each with what is left
    /// of its path below an instance or a top of `definition`, do there. A
    /// `defparam` of one of its parameters gives the parameter in `given`,
    /// which wins over an override; one of anything else is lost. The rest
    /// pass down through its body.
    fn split_passing<'p>(
        &self,
        definition: SymbolRef,
        passing: impl Iterator<Item = (&'p Action, &'p [Step])>,
        given: &mut FxHashMap<SymbolId, Given>,
    ) -> Vec<Deferred> {
        let SymbolKind::Definition { overridable, .. } = &self.design.symbol(definition).kind
        else {
            unreachable!("only a definition is instantiated");
        };
        let mut passed = Vec::new();
        for (action, steps) in passing {
            match (action, steps) {
                (&Action::Defparam { body, file, expr }, [parameter]) => {
                    if parameter.symbol.file == definition.file
                        && parameter.index.is_none()
                        && overridable.contains(&parameter.symbol.symbol)
                    {
                        given.insert(
                            parameter.symbol.symbol,
                            Given::Override { body, file, expr },
                        );
                    }
                }
                (Action::Defparam { .. }, []) => {}
                _ => passed.push(Deferred {
                    steps: steps.to_vec(),
                    action: action.clone(),
                }),
            }
        }
        passed
    }

    /// The `defparam`s and chosen `bind`s on their way down through `body`:
    /// those the body above passes it, and those written in its scope, or
    /// in a generate block there, whose path starts at an instance or a
    /// generate block its scope declares.
    fn deferred(&self, body: BodyId) -> Arc<[Deferred]> {
        self.deferred.get(body, Arc::from([]), || {
            let mut deferred = self.bodies.get(body.index()).passed.clone();
            let cx = self.cx(body);
            let (file, scope) = self.scope(body);
            let mut written = Vec::new();
            self.written(cx, scope, &mut written);
            for (steps, action) in written {
                let head = steps[0].symbol;
                let declared = head.file == file && self.design.symbol(head).scope == scope;
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
    fn passed_to_block(&self, cx: Cx, scope: ScopeId, index: Option<i64>) -> Vec<Deferred> {
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
        passed: Vec<Deferred>,
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
        if given.is_empty() && passed.is_empty() {
            return defaults;
        }
        let body = self.new_body(Body {
            file: definition.file,
            scope: *scope,
            parent: None,
            given,
            passed,
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
                    _ => {
                        let value = self.value(body, parameter);
                        let given = matches!(value, Value::Unknown)
                            && self.given(body, parameter).is_some();
                        Key::Value(value, self.type_of(body, parameter), given)
                    }
                });
            let interfaces = interfaces.iter().map(|&port| match self.given(body, port) {
                Some(Given::Interface(interface)) => Key::Interface(Some(interface)),
                _ => Key::Interface(None),
            });
            let passed = self.bodies.get(body.index()).passed.clone();
            let passed = (!passed.is_empty()).then_some(Key::Deferred(passed));
            parameters.chain(interfaces).chain(passed).collect()
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
            passed: self.passed_to_block(cx, scope, None),
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
        let Some(definition) = self.design.instantiated(symbol) else {
            return Child {
                name,
                kind: ChildKind::Missing { symbol },
            };
        };
        let mut given = FxHashMap::default();
        for (parameter, expr) in bind::overrides(&self.design, symbol) {
            given.insert(
                parameter,
                Given::Override {
                    body: cx.body,
                    file: cx.file,
                    expr,
                },
            );
        }
        // An interface port sees the interface instance connected to it.
        for (port, found) in bind::connections(&self.design, symbol) {
            let port = SymbolRef {
                file: definition.file,
                symbol: port,
            };
            let Some(wanted) = self.interface_port(port) else {
                continue;
            };
            let interface = match found {
                Bound::Expr(expr) => self.interface(cx, expr),
                Bound::Name(Resolution::Declared(at), _) => self.interface_at(cx, at),
                Bound::Name(..) => None,
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
        let deferred = self.deferred(cx.body);
        let passing = deferred.iter().filter_map(|passing| {
            let (head, rest) = passing.steps.split_first()?;
            (head.symbol == symbol && head.index.is_none()).then_some((&passing.action, rest))
        });
        let passed = self.split_passing(definition, passing, &mut given);
        let body = self.instantiate(definition, given, passed);
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
        let certain = !(self
            .design
            .is_declared_twice(&self.design.symbol(definition).name.text));
        Child {
            name,
            kind: ChildKind::Instance {
                symbol,
                definition,
                body,
                dims,
                certain,
            },
        }
    }

    /// What each interface port of the instance `at` is connected to.
    pub(crate) fn interface_connections(&self, at: SymbolRef) -> Vec<(SymbolId, Bound)> {
        let Some(definition) = self.design.instantiated(at) else {
            return Vec::new();
        };
        let mut connected = bind::connections(&self.design, at);
        connected.retain(|&(port, _)| {
            let port = SymbolRef {
                file: definition.file,
                symbol: port,
            };
            self.interface_port(port).is_some()
        });
        connected
    }

    /// Whether the port `port` is an interface port, and the interface it
    /// names: `None` for a generic one, `interface bus`, which takes any.
    pub fn interface_port(&self, port: SymbolRef) -> Option<Option<SymbolRef>> {
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
    fn interface(&self, cx: Cx, expr: ExprId) -> Option<BodyId> {
        let (head, _) = self.design.head(cx.file, expr);
        match self.resolution(cx, head)? {
            Resolution::Declared(at) => self.interface_at(cx, at),
            _ => None,
        }
    }

    /// The body of the interface instance `at` is, or an interface port of
    /// `cx`'s body is connected to. Unlike [`hierarchy`](Self::hierarchy),
    /// `at` is looked for in the bodies around `cx`, not through the names
    /// before it, so a port connected to `u.bus` sees no interface.
    fn interface_at(&self, cx: Cx, at: SymbolRef) -> Option<BodyId> {
        let body = self.scope_in(cx, self.owner(cx, at)?, at, None)?;
        let definition = self.definition_of(body)?;
        matches!(
            self.design.symbol(definition).kind,
            SymbolKind::Definition {
                keyword: INTERFACE_KW,
                ..
            }
        )
        .then_some(body)
    }

    /// The definition of the instance `at`, and the body it builds standing
    /// in `holder`: one of `holder`'s children. `None` for a gate, and an
    /// instance of a definition no file declares.
    pub fn child(&self, holder: BodyId, at: SymbolRef) -> Option<(SymbolRef, BodyId)> {
        let SymbolKind::Instance(instance @ Instance { gate: None, .. }) =
            &self.design.symbol(at).kind
        else {
            return None;
        };
        self.held.get((holder, at), None, || {
            let cx = Cx::new(holder, at.file);
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
        let (head, selects) = self.design.head(cx.file, expr);
        let Some(Resolution::Declared(at)) = self.resolution(cx, head) else {
            return None;
        };
        let holder = match hir[head].kind {
            ExprKind::Member { base, .. } => self.hierarchy(cx, base)?,
            _ => self.owner(cx, at)?,
        };
        // The index right after the name picks a loop's iteration; an
        // array's elements share one body.
        let index = selects.first().and_then(|&select| match hir[select].kind {
            ExprKind::Select {
                index, range: None, ..
            } => Some(index),
            _ => None,
        });
        self.scope_in(cx, holder, at, index)
    }

    /// The body `at`, the last name of a hierarchical name written in
    /// `cx`, names as a scope of `holder`: an instance's, a generate
    /// block's or the iteration's `index` picks of a loop's, or that of the
    /// interface instance an interface port is connected to.
    fn scope_in(
        &self,
        cx: Cx,
        holder: BodyId,
        at: SymbolRef,
        index: Option<ExprId>,
    ) -> Option<BodyId> {
        match &self.design.symbol(at).kind {
            SymbolKind::Port(_) => match self.given(holder, at.symbol)? {
                Given::Interface(interface) => Some(interface),
                _ => None,
            },
            SymbolKind::Instance(_) => Some(self.child(holder, at)?.1),
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
                match self.case_matches(cx, CASE_KW, &selector, label, &ty) {
                    Some(true) => return Decision::Built(arm.body),
                    Some(false) => {}
                    None => return undecided(),
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
        // Each step runs over one variable, the genvar.
        let run = Run::default();
        for _ in 0..ITERATIONS {
            // The header's own body holds the genvar's value for this
            // iteration, and the block is built inside it.
            let mut given = FxHashMap::default();
            given.insert(genvar, Given::Genvar(Value::Bits(value.clone())));
            let iteration = self.new_body(Body {
                file: cx.file,
                scope: header.scope,
                parent: Some(cx.body),
                given,
                passed: Vec::new(),
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
                passed: self.passed_to_block(cx, scope, value.to_i64()),
            });
            let index = value.to_i64().map_or("x".to_string(), |it| it.to_string());
            built.push(Child {
                name: format!("{name}[{index}]").into(),
                kind: ChildKind::Block { body },
            });
            let Some(next) = self.next(at, header, &run, genvar, &value) else {
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
        for member in &hir[header.scope].members {
            if let Member::Declare(symbol) = member
                && let SymbolKind::Genvar(Some(init)) = hir[*symbol].kind
            {
                let value = self.eval_assign(cx, init, &Ty::INTEGER);
                return Some((*symbol, self.genvar_value(value)?));
            }
        }
        let [init] = header.init.as_slice() else {
            return None;
        };
        let ExprKind::Assign { lhs, .. } = hir[*init].kind else {
            return None;
        };
        let Some(Resolution::Declared(at)) = self.resolution(cx, lhs) else {
            return None;
        };
        if at.file != cx.file || !matches!(hir[at.symbol].kind, SymbolKind::Genvar(_)) {
            return None;
        }
        Some((
            at.symbol,
            self.stepped(cx, &Run::default(), at.symbol, Value::Unknown, *init)?,
        ))
    }

    /// The genvar's value after the loop's step, evaluated in `cx`, the
    /// iteration that has `value`.
    fn next(
        &self,
        cx: Cx,
        header: &For,
        run: &Run,
        genvar: SymbolId,
        value: &Bits,
    ) -> Option<Bits> {
        let [step] = header.step.as_slice() else {
            return None;
        };
        self.stepped(cx, run, genvar, Value::Bits(value.clone()), *step)
    }

    /// The value of `genvar` once `expr`, an assignment or an increment of
    /// it, runs in `cx` from `value`, as a constant function's statement
    /// would: over `run`, whose one variable is the genvar.
    fn stepped(
        &self,
        cx: Cx,
        run: &Run,
        genvar: SymbolId,
        value: Value,
        expr: ExprId,
    ) -> Option<Bits> {
        let hir = &self.design[cx.file];
        run.locals.borrow_mut().insert(genvar, value);
        let running = Cx {
            body: cx.body,
            file: cx.file,
            run: Some(run),
            kept: None,
            steps: None,
        };
        // The genvar is the run's only variable, so the step writes it or
        // nothing; but not a part of it.
        let named = |target: ExprId| matches!(hir[target].kind, ExprKind::Name(_));
        match hir[expr].kind {
            ExprKind::Assign { op, lhs, rhs, .. } if named(lhs) => {
                self.assign(running, op, lhs, rhs)?;
            }
            ExprKind::Unary {
                op: op @ (PLUS_PLUS | MINUS_MINUS),
                operand,
            }
            | ExprKind::Postfix { op, operand }
                if named(operand) =>
            {
                self.step(running, operand, op, true);
            }
            _ => return None,
        }
        let value = run.locals.borrow().get(&genvar)?.clone();
        self.genvar_value(value)
    }

    /// `value` as a genvar's: an integer, without an `x` or `z`.
    fn genvar_value(&self, value: Value) -> Option<Bits> {
        match self.convert(value, &Ty::INTEGER) {
            Value::Bits(bits) if !bits.has_unknown() => Some(bits),
            _ => None,
        }
    }

    /// The tree from `top` down, an indented line each: an instance's name,
    /// its definition, and the value of each parameter of its parameter
    /// port list; a generate block's name; what sema cannot decide or find,
    /// and each block past what the walk may enter, [`Elaboration::BLOCKS`].
    pub fn dump(&self, top: BodyId) -> String {
        let mut out = String::new();
        let (file, scope) = self.scope(top);
        let name = match self.design[file][scope].owner {
            Some(owner) => {
                let definition = SymbolRef {
                    file,
                    symbol: owner,
                };
                let parameters = self.parameters(definition, top);
                format!("{}{parameters}", self.design[file][owner].name.text)
            }
            None => "$unit".into(),
        };
        writeln!(out, "{name}").unwrap();
        self.dump_children(top, 1, &mut Blocks::default(), &mut out);
        out
    }

    fn dump_children(&self, body: BodyId, depth: usize, blocks: &mut Blocks, out: &mut String) {
        let indent = "  ".repeat(depth);
        if depth > Elaboration::DEPTH {
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
                    let parameters = self.parameters(*definition, *body);
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
                        "{indent}{}{dims}: {defined}{parameters}{apart}",
                        child.name
                    )
                    .unwrap();
                    self.dump_children(*body, depth + 1, blocks, out);
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
                ChildKind::Block { body } if blocks.enter(*body) => {
                    writeln!(out, "{indent}{}", child.name).unwrap();
                    self.dump_children(*body, depth + 1, blocks, out);
                }
                ChildKind::Block { .. } => {
                    writeln!(out, "{indent}{}: undecided", child.name).unwrap();
                }
                ChildKind::Undecided { .. } => {
                    writeln!(out, "{indent}{}: undecided", child.name).unwrap();
                }
            }
        }
    }

    /// ` #(name = value, …)` for each parameter of `definition`'s parameter
    /// port list, a `localparam` there too, or for each `parameter` of its
    /// body when it has none, with its value in `body`.
    fn parameters(&self, definition: SymbolRef, body: BodyId) -> String {
        let SymbolKind::Definition { parameters, .. } = &self.design.symbol(definition).kind else {
            return String::new();
        };
        let hir = &self.design[definition.file];
        let mut listed = Vec::new();
        for &parameter in parameters {
            let name = &hir[parameter].name.text;
            match hir[parameter].kind {
                SymbolKind::Parameter(Parameter { is_type: true, .. }) => {
                    listed.push(format!("{name} = {}", self.type_of(body, parameter)));
                }
                _ => listed.push(format!("{name} = {}", self.value(body, parameter))),
            }
        }
        match listed.is_empty() {
            true => String::new(),
            false => format!(" #({})", listed.join(", ")),
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
