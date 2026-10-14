//! What a design builds from its tops: which generate blocks exist, which
//! definitions are reached, and the errors only what is built can have.

mod widths;

use astli_syntax::SyntaxKind::{self, *};
use astli_text::{Diagnostic, Label, Span};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::access::{Driver, accesses};
use crate::design::{FileId, SymbolRef};
use crate::diagnostics;
use crate::elab::{BodyId, ChildKind, Elaboration};
use crate::hir::*;

pub use widths::{Width, WidthKind};

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

/// A body and the generate blocks built in it, each its own frame.
#[derive(Debug)]
struct Frame {
    name: Box<str>,
    /// Its place in the tree, the names from the top down.
    path: Box<str>,
    body: BodyId,
    file: FileId,
    scope: ScopeId,
    blocks: Vec<Frame>,
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
                ChildKind::Missing { .. } => {}
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
        let design = elaboration.design();
        let mut files: FxHashMap<FileId, Drivers> = FxHashMap::default();
        let mut found = Vec::new();
        let mut reported = FxHashSet::default();
        for body in &self.bodies {
            let drivers = files
                .entry(body.file)
                .or_insert_with(|| Drivers::new(elaboration, body.file));
            // Each variable's writers in this body: the driver, the frames
            // from the body's down to the one it stands in, whether a write
            // is to all of it, and where it first writes.
            let mut writers: FxHashMap<(Vec<usize>, SymbolId), Vec<Writer>> = FxHashMap::default();
            let mut order = Vec::new();
            body.collect(
                elaboration,
                drivers,
                &mut Vec::new(),
                &mut Vec::new(),
                &mut |owner, symbol, writer| {
                    let key = (owner, symbol);
                    let list = writers.entry(key.clone()).or_insert_with(|| {
                        order.push(key);
                        Vec::new()
                    });
                    match list
                        .iter_mut()
                        .find(|it| it.driver == writer.driver && it.path == writer.path)
                    {
                        Some(it) => it.whole |= writer.whole,
                        None => list.push(writer),
                    }
                },
            );
            let hir = &design[body.file];
            for key in order {
                let list = &writers[&key];
                let clash = list.iter().enumerate().find_map(|(at, second)| {
                    let first = list[..at].iter().find(|first| {
                        (first.whole || second.whole)
                            && (drivers.sole(first.driver) || drivers.sole(second.driver))
                    })?;
                    Some((first, second))
                });
                let Some((first, second)) = clash else {
                    continue;
                };
                if !reported.insert((body.file, key.1, second.at)) {
                    continue;
                }
                let name = &hir[key.1].name.text;
                let message = match first.driver == second.driver {
                    true => format!(
                        "`{name}` is driven by {} in `{}` and again in `{}`",
                        drivers.what(hir, first.driver),
                        first.place,
                        second.place
                    ),
                    false => format!(
                        "`{name}` is driven by {} and by {}",
                        drivers.what(hir, first.driver),
                        drivers.what(hir, second.driver)
                    ),
                };
                let mut diagnostic = diagnostics::multiple_drivers(message, second.at);
                diagnostic.labels.push(Label {
                    at: first.at,
                    message: "driven here first".to_string(),
                });
                found.push((body.file, diagnostic));
            }
        }
        found
    }
}

/// One thing that writes a variable, in one frame.
struct Writer {
    driver: Driver,
    /// The frames from the body's down to the one the driver stands in, by
    /// their place among their parent's blocks.
    path: Vec<usize>,
    /// The frame's name, as a message says where it is.
    place: Box<str>,
    whole: bool,
    at: Span,
}

impl Frame {
    /// Calls `found` for each write a sole driver in this frame or one
    /// inside it makes to a variable a frame around it declares: with the
    /// path of the declaring frame, the variable, and the writer.
    fn collect(
        &self,
        elaboration: &Elaboration,
        drivers: &Drivers,
        path: &mut Vec<usize>,
        frames: &mut Vec<(ScopeId, Vec<usize>)>,
        found: &mut impl FnMut(Vec<usize>, SymbolId, Writer),
    ) {
        frames.push((self.scope, path.clone()));
        let hir = &elaboration.design()[self.file];
        for driver in drivers.located.get(&self.scope).into_iter().flatten() {
            for access in drivers.writes.get(driver).into_iter().flatten() {
                // The frame that declares the variable: the innermost around
                // this one whose scope holds its declaration.
                let home = elaboration.home(self.file, access.symbol);
                let declaring = declaring(hir, home);
                let Some((_, owner)) = frames
                    .iter()
                    .rev()
                    .find(|(scope, _)| Some(*scope) == declaring)
                else {
                    continue;
                };
                found(
                    owner.clone(),
                    access.symbol,
                    Writer {
                        driver: *driver,
                        path: path.clone(),
                        place: self.name.clone(),
                        whole: access.whole,
                        at: access.at,
                    },
                );
            }
        }
        for (index, block) in self.blocks.iter().enumerate() {
            path.push(index);
            block.collect(elaboration, drivers, path, frames, found);
            path.pop();
        }
        frames.pop();
    }
}

/// The definition's or generate block's scope a declaration in `scope`
/// belongs to; `None` for one in a subroutine or a procedural block, which
/// nothing continuous drives.
fn declaring(hir: &Hir, scope: ScopeId) -> Option<ScopeId> {
    match hir[scope].kind {
        ScopeKind::Definition | ScopeKind::Generate => Some(scope),
        _ => None,
    }
}

/// One write a driver makes.
struct Write {
    symbol: SymbolId,
    whole: bool,
    at: Span,
}

/// The drivers of one file: where each stands, and the variables each
/// writes.
struct Drivers {
    /// The drivers each definition's or generate block's scope holds.
    located: FxHashMap<ScopeId, Vec<Driver>>,
    /// Each process's keyword.
    kinds: FxHashMap<Driver, SyntaxKind>,
    writes: FxHashMap<Driver, Vec<Write>>,
}

impl Drivers {
    fn new(elaboration: &Elaboration, file: FileId) -> Drivers {
        let design = elaboration.design();
        let hir = &design[file];
        let mut located: FxHashMap<ScopeId, Vec<Driver>> = FxHashMap::default();
        let mut kinds = FxHashMap::default();
        for (scope, body) in hir.scopes() {
            for member in &body.members {
                let found: Vec<Driver> = match member {
                    Member::Process(process) => {
                        let driver = Driver::Process(process.body);
                        kinds.insert(driver, process.kind);
                        vec![driver]
                    }
                    Member::Assign(assignments) => {
                        assignments.iter().map(|&it| Driver::Assign(it)).collect()
                    }
                    Member::Declare(symbol)
                        if matches!(hir[*symbol].kind, SymbolKind::Instance(_)) =>
                    {
                        vec![Driver::Instance(*symbol)]
                    }
                    _ => continue,
                };
                located.entry(scope).or_default().extend(found);
            }
        }
        let mut writes: FxHashMap<Driver, Vec<Write>> = FxHashMap::default();
        for access in accesses(design, file, elaboration.names(file)) {
            if access.write
                && access.certain
                && access.symbol.file == file
                && variable(hir, access.symbol.symbol)
            {
                writes.entry(access.driver).or_default().push(Write {
                    symbol: access.symbol.symbol,
                    whole: access.whole,
                    at: access.at,
                });
            }
        }
        Drivers {
            located,
            kinds,
            writes,
        }
    }

    /// Whether `driver` must be a variable's only one: a continuous
    /// assignment, an instance's output, or an `always_comb`, `always_ff` or
    /// `always_latch`.
    fn sole(&self, driver: Driver) -> bool {
        match driver {
            Driver::Process(_) => matches!(
                self.kinds.get(&driver),
                Some(ALWAYS_COMB_KW | ALWAYS_FF_KW | ALWAYS_LATCH_KW)
            ),
            _ => true,
        }
    }

    /// A driver, as a message names it.
    fn what(&self, hir: &Hir, driver: Driver) -> String {
        match driver {
            Driver::Process(_) => match self.kinds.get(&driver) {
                Some(kind) => format!("an `{}`", kind.keyword_text().unwrap_or("always")),
                None => "a process".to_string(),
            },
            Driver::Assign(_) => "a continuous assignment".to_string(),
            Driver::Instance(symbol) => format!("instance `{}`", hir[symbol].name.text),
            _ => "something else".to_string(),
        }
    }
}

/// Whether `symbol` is a variable, of which the standard allows one driver
/// of these kinds; a net may have many.
fn variable(hir: &Hir, symbol: SymbolId) -> bool {
    match &hir[symbol].kind {
        SymbolKind::Variable(_) => true,
        // An output with a data type and no net type is a variable.
        SymbolKind::Port(port) => {
            port.direction == Some(OUTPUT_KW)
                && matches!(port.keyword, None | Some(VAR_KW))
                && !matches!(port.ty.kind, TypeKind::Implicit { .. })
        }
        _ => false,
    }
}
