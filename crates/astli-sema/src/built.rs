//! What a design builds from its tops: which generate blocks exist, which
//! definitions are reached, and the errors only what is built can have.

use astli_text::Diagnostic;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::design::{FileId, SymbolRef};
use crate::drivers;
use crate::elab::{BodyId, ChildKind, Elaboration};
use crate::hir::*;

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

impl Built {
    /// Elaborates from each of `tops`.
    pub fn new(elaboration: &Elaboration, tops: &[BodyId]) -> Built {
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

    /// Each variable two things drive in what is built, where the standard
    /// allows one: an `always_comb`, `always_ff` or `always_latch`, a
    /// continuous assignment or an instance's output, and anything else that
    /// writes all of it. Each generate block built is a place of its own,
    /// so a loop that writes all of a variable drives it once per iteration.
    pub fn multiple_drivers(&self, elaboration: &Elaboration) -> Vec<(FileId, Diagnostic)> {
        drivers::built(self, elaboration)
    }
}
