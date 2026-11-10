//! What a design builds from its tops: which generate blocks exist, which
//! definitions are reached, and the errors only what is built can have.

use astli_text::Diagnostic;
use parking_lot::Mutex;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::design::{FileId, SymbolRef};
use crate::elab::{BodyId, ChildKind, Elaboration};
use crate::hir::*;
use crate::{diagnostics, drivers, typing};

/// How deep the tree is followed; a definition that instantiates itself
/// makes it endless.
const DEPTH: usize = 64;

/// Whether a generate block is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fate {
    /// In some body the tops reach.
    Built,
    /// Under a construct sema cannot decide, and built in none.
    Undecided,
}

/// What elaborating a design from its tops builds.
///
/// Below an instance of a definition more than one file declares, nothing
/// is sure to be built: which one a compiler keeps is a guess, so the
/// generate blocks there are undecided and their drivers not compared.
///
/// It holds no reference to the elaboration that made it, so files are
/// checked against it in parallel.
#[derive(Debug, Default)]
pub struct Built {
    /// Each generate block's scope, by whether it is built.
    scopes: FxHashMap<(FileId, ScopeId), Fate>,
    /// The definitions some instance, or a top, builds a body of.
    definitions: FxHashSet<SymbolRef>,
    /// Each body built for a definition, once however often it is
    /// instantiated, with the generate blocks built inside it.
    bodies: Vec<Frame>,
    /// The tops built for sure.
    tops: Vec<BodyId>,
}

/// A body built, or a generate block built in one, with the blocks built
/// inside it.
#[derive(Debug)]
pub struct Frame {
    /// The instance's name, or the block's.
    pub name: Box<str>,
    /// Its place in the tree, the names from the top down.
    pub path: Box<str>,
    pub body: BodyId,
    pub file: FileId,
    /// The definition's scope, or the block's.
    pub scope: ScopeId,
    pub blocks: Vec<Frame>,
}

/// Elaborates the bodies below `tops` on every core, so that the walk
/// after, which must go in order, finds each one's children made.
fn prefetch(elaboration: &Elaboration, tops: &[BodyId]) {
    let seen = Mutex::new(FxHashSet::default());
    rayon::scope(|scope| {
        for &top in tops {
            visit(scope, elaboration, &seen, top, 0);
        }
    });
}

/// Elaborates `body`, and below it, in tasks of `scope`.
fn visit<'a>(
    scope: &rayon::Scope<'a>,
    elaboration: &'a Elaboration,
    seen: &'a Mutex<FxHashSet<BodyId>>,
    body: BodyId,
    depth: usize,
) {
    if depth > DEPTH || !seen.lock().insert(body) {
        return;
    }
    scope.spawn(move |scope| {
        for child in elaboration.children(body).iter() {
            if let ChildKind::Instance { body, .. } | ChildKind::Block { body } = child.kind {
                visit(scope, elaboration, seen, body, depth + 1);
            }
        }
    });
}

impl Built {
    /// Elaborates from each of `tops`.
    pub fn new(elaboration: &Elaboration, tops: &[BodyId]) -> Built {
        prefetch(elaboration, tops);
        let mut built = Built::default();
        let mut seen = FxHashSet::default();
        for &top in tops {
            let (file, scope) = elaboration.scope(top);
            let hir = &elaboration.design()[file];
            let Some(owner) = hir[scope].owner else {
                continue;
            };
            let name = &hir[owner].name.text;
            let certain = !elaboration.design().is_declared_twice(name);
            if certain {
                built.definitions.insert(SymbolRef {
                    file,
                    symbol: owner,
                });
                built.tops.push(top);
            }
            let path = name.clone();
            built.instance(elaboration, top, name.clone(), path, certain, &mut seen, 0);
        }
        built
    }

    /// Each body the tops build, once however often it is instantiated.
    pub fn frames(&self) -> &[Frame] {
        &self.bodies
    }

    /// The tops built for sure.
    pub(crate) fn tops(&self) -> &[BodyId] {
        &self.tops
    }

    /// Whether the tops build a body of `definition`.
    pub fn reaches(&self, definition: SymbolRef) -> bool {
        self.definitions.contains(&definition)
    }

    pub(crate) fn fate(&self, file: FileId, scope: ScopeId) -> Option<Fate> {
        self.scopes.get(&(file, scope)).copied()
    }

    /// Walks `body`, built for sure if `certain`.
    #[allow(clippy::too_many_arguments)]
    fn instance(
        &mut self,
        elaboration: &Elaboration,
        body: BodyId,
        name: Box<str>,
        path: Box<str>,
        certain: bool,
        seen: &mut FxHashSet<(BodyId, bool)>,
        depth: usize,
    ) {
        if depth > DEPTH || !seen.insert((body, certain)) {
            return;
        }
        let frame = self.frame(elaboration, body, name, path, certain, seen, depth);
        if certain {
            self.bodies.push(frame);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn frame(
        &mut self,
        elaboration: &Elaboration,
        body: BodyId,
        name: Box<str>,
        path: Box<str>,
        certain: bool,
        seen: &mut FxHashSet<(BodyId, bool)>,
        depth: usize,
    ) -> Frame {
        let (file, scope) = elaboration.scope(body);
        let mut blocks = Vec::new();
        for child in elaboration.children(body).iter() {
            match &child.kind {
                ChildKind::Instance {
                    definition, body, ..
                } => {
                    let defined = &elaboration.design().symbol(*definition).name.text;
                    let certain = certain && !elaboration.design().is_declared_twice(defined);
                    if certain {
                        self.definitions.insert(*definition);
                    }
                    let name = child.name.clone();
                    let path = format!("{path}.{name}").into();
                    self.instance(elaboration, *body, name, path, certain, seen, depth + 1);
                }
                ChildKind::Block { body } => {
                    let (_, scope) = elaboration.scope(*body);
                    let fate = if certain {
                        Fate::Built
                    } else {
                        Fate::Undecided
                    };
                    let entry = self.scopes.entry((file, scope)).or_insert(fate);
                    if fate == Fate::Built {
                        *entry = Fate::Built;
                    }
                    let name = child.name.clone();
                    let path = format!("{path}.{name}").into();
                    let frame =
                        self.frame(elaboration, *body, name, path, certain, seen, depth + 1);
                    blocks.push(frame);
                }
                ChildKind::Undecided { scopes } => {
                    for &scope in scopes {
                        self.scopes.entry((file, scope)).or_insert(Fate::Undecided);
                    }
                }
                ChildKind::Missing { .. } | ChildKind::Gate { .. } => {}
            }
        }
        Frame {
            name,
            path,
            body,
            file,
            scope,
            blocks,
        }
    }

    /// Each elaboration system task in what is built, with what its
    /// arguments say in the body it runs in: a `$fatal` or an `$error` an
    /// error, a `$warning` a warning. An `$info` reports nothing wrong, so
    /// it is not told.
    pub fn elaboration_tasks(&self, elaboration: &Elaboration) -> Vec<(FileId, Diagnostic)> {
        let mut found = Vec::new();
        let mut told = FxHashSet::default();
        let mut frames: Vec<&Frame> = self.bodies.iter().collect();
        while let Some(frame) = frames.pop() {
            frames.extend(&frame.blocks);
            let hir = &elaboration.design()[frame.file];
            for member in &hir[frame.scope].members {
                let Member::ElaborationTask(task) = member else {
                    continue;
                };
                let (callee, args) = match &hir[*task].kind {
                    ExprKind::Call { callee, args } => (*callee, args.as_slice()),
                    _ => (*task, &[][..]),
                };
                let ExprKind::System(name) = &hir[callee].kind else {
                    continue;
                };
                // `$fatal`'s first argument says how the simulation ends.
                let args = match &**name {
                    "$info" => continue,
                    "$fatal" => args.get(1..).unwrap_or_default(),
                    _ => args,
                };
                let message = elaboration.message(frame.body, args);
                let at = hir[*task].span;
                // Bodies built with other parameters may say the same.
                if told.insert((frame.file, at, message.clone())) {
                    let diagnostic = diagnostics::elaboration_task(name, message, &frame.path, at);
                    found.push((frame.file, diagnostic));
                }
            }
        }
        found
    }

    /// Each parameter without a default that what is built leaves without
    /// a value: one no instance overrides and no `defparam` sets, and a
    /// top's that is not set from outside the design. A `defparam` whose
    /// path sema cannot follow, through an index it cannot evaluate, may
    /// still set it, so a parameter of a name any `defparam` sets is not
    /// told.
    pub fn unset_parameters(&self, elaboration: &Elaboration) -> Vec<(FileId, Diagnostic)> {
        let design = elaboration.design();
        let defparams = defparam_names(elaboration);
        let mut found = Vec::new();
        for &top in &self.tops {
            let (file, scope) = elaboration.scope(top);
            let Some(owner) = design[file][scope].owner else {
                continue;
            };
            let definition = SymbolRef {
                file,
                symbol: owner,
            };
            let name = &design.symbol(definition).name.text;
            for parameter in unset(elaboration, top, definition, &defparams) {
                let parameter = &design[file][parameter].name;
                let diagnostic =
                    diagnostics::unset_top_parameter(name, &parameter.text, parameter.span);
                found.push((file, diagnostic));
            }
        }
        // Instances built with other parameters around them may leave out
        // the same.
        let mut told = FxHashSet::default();
        let mut frames: Vec<&Frame> = self.bodies.iter().collect();
        while let Some(frame) = frames.pop() {
            frames.extend(&frame.blocks);
            for child in elaboration.children(frame.body).iter() {
                let ChildKind::Instance {
                    symbol,
                    definition,
                    body,
                    ..
                } = child.kind
                else {
                    continue;
                };
                // Which of two definitions a compiler keeps is a guess.
                if design.is_declared_twice(&design.symbol(definition).name.text) {
                    continue;
                }
                let instance = &design.symbol(symbol).name;
                for parameter in unset(elaboration, body, definition, &defparams) {
                    if !told.insert((symbol, parameter)) {
                        continue;
                    }
                    let parameter = &design[definition.file][parameter].name;
                    let diagnostic = diagnostics::unset_parameter(
                        &instance.text,
                        &parameter.text,
                        instance.span,
                        parameter.span,
                    );
                    found.push((symbol.file, diagnostic));
                }
            }
        }
        found
    }

    /// Each variable two things drive in what is built, where the standard
    /// allows one: an `always_comb`, `always_ff` or `always_latch`, a
    /// continuous assignment or an instance's output, and anything else that
    /// writes all of it. Each generate block built is a place of its own,
    /// so a loop that writes all of a variable drives it once per iteration.
    pub fn multiple_drivers(&self, elaboration: &Elaboration) -> Vec<(FileId, Diagnostic)> {
        drivers::built(self, elaboration)
    }

    /// Each value in what is built given where its type does not convert
    /// to the type it is given to, and each operand an operator does not
    /// take, checked in each body with its parameters.
    pub fn mistyped(&self, elaboration: &Elaboration) -> Vec<(FileId, Diagnostic)> {
        typing::built(self, elaboration)
    }
}

/// The parameters of `definition` without a default that `body`, built of
/// it, is given no value for. A prototype's are its definition's, which
/// another file declares.
fn unset(
    elaboration: &Elaboration,
    body: BodyId,
    definition: SymbolRef,
    defparams: &FxHashSet<Box<str>>,
) -> Vec<SymbolId> {
    let design = elaboration.design();
    let SymbolKind::Definition {
        parameters,
        parameters_known: true,
        prototype: false,
        ..
    } = &design.symbol(definition).kind
    else {
        return Vec::new();
    };
    let hir = &design[definition.file];
    (parameters.iter().copied())
        .filter(|&id| {
            matches!(
                &hir[id].kind,
                SymbolKind::Parameter(Parameter {
                    local: false,
                    value: None,
                    ..
                })
            )
        })
        .filter(|&id| !defparams.contains(&hir[id].name.text))
        .filter(|&id| !elaboration.is_given(body, id))
        .collect()
}

/// The name each `defparam` in the design sets, the last of its path.
fn defparam_names(elaboration: &Elaboration) -> FxHashSet<Box<str>> {
    let mut names = FxHashSet::default();
    for (_, hir) in elaboration.design().files() {
        for (_, scope) in hir.scopes() {
            for member in &scope.members {
                let Member::Defparam(assignments) = member else {
                    continue;
                };
                for &assignment in assignments {
                    let ExprKind::Assign { lhs, .. } = hir[assignment].kind else {
                        continue;
                    };
                    match &hir[lhs].kind {
                        ExprKind::Member { name, .. } => names.insert(name.text.clone()),
                        ExprKind::Name(name) => names.insert(name.clone()),
                        _ => continue,
                    };
                }
            }
        }
    }
    names
}
