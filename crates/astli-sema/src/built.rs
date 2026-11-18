//! What a design builds from its tops: which generate blocks exist, which
//! definitions are reached, and each body built with the blocks in it,
//! which [`check_built`](crate::check_built) and the width lints walk.

use parking_lot::Mutex;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::design::{FileId, SymbolRef};
use crate::elab::{BodyId, ChildKind, Elaboration};
use crate::hir::*;

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
    /// instantiated, followed by the generate blocks built inside it.
    frames: Vec<Frame>,
    /// The tops built for sure.
    tops: Vec<BodyId>,
}

/// A body built, or a generate block built in one.
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
    /// The frame a generate block is built in, by its place in
    /// [`Built::frames`]; `None` for a body.
    pub parent: Option<usize>,
}

/// An instance a body holds, to walk once the body's frames are made: its
/// body, name and path, whether it is built for sure, and its depth.
type Pending = (BodyId, Box<str>, Box<str>, bool, usize);

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
    if depth > Elaboration::DEPTH || !seen.lock().insert(body) {
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

    /// Each of `definitions` built at its parameters' defaults, with the
    /// generate blocks built in it but not what its instances build: of a
    /// definition no top reaches, the one body sure to be its own.
    pub(crate) fn at_defaults(elaboration: &Elaboration, definitions: &[SymbolRef]) -> Built {
        let mut built = Built::default();
        for &definition in definitions {
            let Some(body) = elaboration.body(definition) else {
                continue;
            };
            let name = &elaboration.design().symbol(definition).name.text;
            let instances = &mut Vec::new();
            built.frame(
                elaboration,
                body,
                name.clone(),
                name.clone(),
                None,
                true,
                instances,
                0,
            );
        }
        built
    }

    /// Each body the tops build, once however often it is instantiated,
    /// followed by the generate blocks built inside it, each after the
    /// frame it is built in.
    pub fn frames(&self) -> &[Frame] {
        &self.frames
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
        if depth > Elaboration::DEPTH || !seen.insert((body, certain)) {
            return;
        }
        // The body's frames stand together, before those of the bodies it
        // instantiates.
        let mut instances = Vec::new();
        self.frame(
            elaboration,
            body,
            name,
            path,
            None,
            certain,
            &mut instances,
            depth,
        );
        for (body, name, path, certain, depth) in instances {
            self.instance(elaboration, body, name, path, certain, seen, depth);
        }
    }

    /// Walks `body`, a body or a generate block built in the frame
    /// `parent`, adding to `instances` each instance it holds.
    #[allow(clippy::too_many_arguments)]
    fn frame(
        &mut self,
        elaboration: &Elaboration,
        body: BodyId,
        name: Box<str>,
        path: Box<str>,
        parent: Option<usize>,
        certain: bool,
        instances: &mut Vec<Pending>,
        depth: usize,
    ) {
        let (file, scope) = elaboration.scope(body);
        let id = self.frames.len();
        if certain {
            self.frames.push(Frame {
                name,
                path: path.clone(),
                body,
                file,
                scope,
                parent,
            });
        }
        for child in elaboration.children(body).iter() {
            match &child.kind {
                ChildKind::Instance {
                    definition,
                    body,
                    certain: sure,
                    ..
                } => {
                    let certain = certain && *sure;
                    if certain {
                        self.definitions.insert(*definition);
                    }
                    let name = child.name.clone();
                    let path = format!("{path}.{name}").into();
                    instances.push((*body, name, path, certain, depth + 1));
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
                    let parent = Some(id);
                    self.frame(
                        elaboration,
                        *body,
                        name,
                        path,
                        parent,
                        certain,
                        instances,
                        depth + 1,
                    );
                }
                ChildKind::Undecided { scopes } => {
                    for &scope in scopes {
                        self.scopes.entry((file, scope)).or_insert(Fate::Undecided);
                    }
                }
                ChildKind::Missing { .. } | ChildKind::Gate { .. } => {}
            }
        }
    }
}
