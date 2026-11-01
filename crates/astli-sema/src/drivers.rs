//! Variables two things drive where the standard allows one: an
//! `always_comb`, `always_ff` or `always_latch` block, a continuous
//! assignment or an instance's output, and anything else that writes the
//! same bits.
//!
//! What the tops build is compared exactly: each body with its parameters,
//! each generate block built a place of its own, and each interface
//! instance over the bodies connected to it. A definition no top
//! reaches is compared at its parameters' defaults, with every generate arm,
//! and only drivers that exist together clash; since it may never be built,
//! or be built with other parameters, what it finds is a warning.

use std::ops::Range;

use astli_syntax::SyntaxKind::{self, *};
use astli_text::{Diagnostic, Label, Severity, Span};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::access::{Access, Driver, accesses};
use crate::built::{Built, Frame};
use crate::design::{Design, FileId, SymbolRef};
use crate::diagnostics;
use crate::elab::{BodyId, ChildKind, Connected, Elaboration};
use crate::hir::*;
use crate::resolve::{Names, Resolution};
use crate::value::Value;

/// How deep the walk through interfaces follows the tree; a definition that
/// instantiates itself makes it endless.
const DEPTH: usize = 64;

/// How many instances the walk through interfaces visits, past which it
/// compares no more: it follows each path, not each body.
const PATHS: usize = 1 << 20;

/// The generate arms a driver stands in, outermost first: each as the scope
/// and place of its `if` or `case` there, and the arm.
type Arms = Vec<(ScopeId, usize, usize)>;

/// The drivers of one file: where each stands, and what each writes.
struct Drivers {
    /// The drivers each definition's or generate block's scope holds.
    located: FxHashMap<ScopeId, Vec<Driver>>,
    /// Each process's keyword.
    kinds: FxHashMap<Driver, SyntaxKind>,
    arms: FxHashMap<Driver, Arms>,
    /// What each writes, certainly, of the file's variables.
    writes: FxHashMap<Driver, Vec<Access>>,
    /// What each writes, certainly, of an interface's variables, as
    /// `base.name`: through an interface port or an interface instance.
    through: FxHashMap<Driver, Vec<Access>>,
}

impl Drivers {
    fn new(design: &Design, file: FileId, names: &Names) -> Drivers {
        let hir = &design[file];
        let mut drivers = Drivers {
            located: FxHashMap::default(),
            kinds: FxHashMap::default(),
            arms: FxHashMap::default(),
            writes: FxHashMap::default(),
            through: FxHashMap::default(),
        };
        drivers.place(hir, hir.root(), &mut Vec::new());
        for access in accesses(design, file, names) {
            if !access.write
                || !access.certain
                || !drivers.arms.contains_key(&access.driver)
                || !variable(&design[access.symbol.file], access.symbol.symbol)
            {
                continue;
            }
            if access.base.is_some() && in_interface(design, access.symbol) {
                drivers
                    .through
                    .entry(access.driver)
                    .or_default()
                    .push(access);
            }
            if access.symbol.file == file {
                drivers
                    .writes
                    .entry(access.driver)
                    .or_default()
                    .push(access);
            }
        }
        drivers
    }

    /// Records each driver in `scope`, standing in `arms`.
    fn place(&mut self, hir: &Hir, scope: ScopeId, arms: &mut Arms) {
        for (index, member) in hir[scope].members.iter().enumerate() {
            let found: Vec<Driver> = match member {
                Member::Process(process) => {
                    let driver = Driver::Process(process.body);
                    self.kinds.insert(driver, process.kind);
                    vec![driver]
                }
                Member::Assign(assignments) => {
                    assignments.iter().map(|&it| Driver::Assign(it)).collect()
                }
                Member::Declare(symbol) => match &hir[*symbol].kind {
                    SymbolKind::Instance(_) => vec![Driver::Instance(*symbol)],
                    SymbolKind::Definition { scope, .. } => {
                        self.place(hir, *scope, arms);
                        continue;
                    }
                    _ => continue,
                },
                Member::GenerateIf(choices) | Member::GenerateCase { arms: choices, .. } => {
                    for (arm, choice) in choices.iter().enumerate() {
                        arms.push((scope, index, arm));
                        self.place(hir, choice.body, arms);
                        arms.pop();
                    }
                    continue;
                }
                Member::GenerateFor { body, .. } | Member::Generate(body) => {
                    self.place(hir, *body, arms);
                    continue;
                }
                // What a bound instance drives is in the target's file.
                Member::Import(_)
                | Member::Bind(_)
                | Member::DefaultDisable(_)
                | Member::DefaultClocking(_)
                | Member::Opaque(_) => continue,
            };
            for driver in found {
                self.arms.insert(driver, arms.clone());
                self.located.entry(scope).or_default().push(driver);
            }
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

    /// Whether one of `a` and `b` exists whenever the other does: the
    /// generate arms one stands in are the first of those the other stands
    /// in. Two arms of one `if` never both exist, and two generate
    /// constructs may have conditions that exclude each other.
    fn together(&self, a: Driver, b: Driver) -> bool {
        let (Some(a), Some(b)) = (self.arms.get(&a), self.arms.get(&b)) else {
            return false;
        };
        a.starts_with(b) || b.starts_with(a)
    }

    /// A driver, as a message names it.
    fn what(&self, hir: &Hir, driver: Driver) -> String {
        match driver {
            Driver::Process(_) => match self.kinds.get(&driver) {
                Some(kind) => format!("an `{}`", kind.keyword_text().unwrap_or("always")),
                None => "a process".to_string(),
            },
            Driver::Assign(_) => "a continuous assignment".to_string(),
            Driver::Instance(symbol) => match (&*hir[symbol].name.text, &hir[symbol].kind) {
                ("", SymbolKind::Instance(instance)) => {
                    format!("an unnamed instance of `{}`", instance.definition.text)
                }
                (name, _) => format!("instance `{name}`"),
            },
            _ => "something else".to_string(),
        }
    }

    /// The first writer in `list` that clashes with one before it, and that
    /// one, as [`clash`] has it.
    fn clash<'a>(
        &self,
        list: &'a [Writer],
        together: impl Fn(&Writer, &Writer) -> bool,
    ) -> Option<(&'a Writer, &'a Writer)> {
        clash(list, |writer| self.sole(writer.driver), together)
    }

    /// The error for `name`, written by `first` and then by `second`.
    fn diagnostic(&self, hir: &Hir, name: &str, first: &Writer, second: &Writer) -> Diagnostic {
        let message = match first.driver == second.driver {
            true => format!(
                "`{name}` is driven by {} in `{}` and again in `{}`",
                self.what(hir, first.driver),
                first.place,
                second.place
            ),
            false => format!(
                "`{name}` is driven by {} and by {}",
                self.what(hir, first.driver),
                self.what(hir, second.driver)
            ),
        };
        let mut diagnostic = diagnostics::multiple_drivers(message, second.at);
        diagnostic.labels.push(Label {
            at: first.at,
            message: "driven here first".to_string(),
        });
        diagnostic
    }
}

/// The first writer in `list` that clashes with one before it, and that
/// one: some bit both write, one of them a sole driver, and both existing
/// together as `together` tells.
fn clash(
    list: &[Writer],
    sole: impl Fn(&Writer) -> bool,
    together: impl Fn(&Writer, &Writer) -> bool,
) -> Option<(&Writer, &Writer)> {
    list.iter().enumerate().find_map(|(at, second)| {
        let first = list[..at].iter().find(|first| {
            first.overlaps(second) && (sole(first) || sole(second)) && together(first, second)
        })?;
        Some((first, second))
    })
}

/// One driver's writes to a variable, in one frame.
struct Writer {
    /// The file the driver stands in.
    file: FileId,
    driver: Driver,
    /// The frames from the body's down to the one the driver stands in, by
    /// their place among their parent's blocks; empty at the defaults.
    path: Vec<usize>,
    /// The frame's name, as a message says where it is.
    place: Box<str>,
    whole: bool,
    /// The bits its writes to parts of the variable reach, where sema can
    /// evaluate their selects.
    reach: Vec<Range<u64>>,
    at: Span,
    /// Whether it stands in the interface whose variable it writes.
    inside: bool,
}

impl Writer {
    /// Whether some bit is written by both: one writes all of the
    /// variable, or the parts they write overlap. A part whose bits sema
    /// cannot tell clashes only with a write to all of it.
    fn overlaps(&self, other: &Writer) -> bool {
        self.whole
            || other.whole
            || (self.reach.iter())
                .any(|a| (other.reach.iter()).any(|b| a.start < b.end && b.start < a.end))
    }
}

/// Adds `writer` to its variable's writers in `list`, one per driver and
/// frame, `order` keeping the variables in the order first written.
fn add<K: Clone + Eq + std::hash::Hash>(
    writers: &mut FxHashMap<K, Vec<Writer>>,
    order: &mut Vec<K>,
    key: K,
    writer: Writer,
) {
    let list = writers.entry(key.clone()).or_insert_with(|| {
        order.push(key);
        Vec::new()
    });
    match list
        .iter_mut()
        .find(|it| it.driver == writer.driver && it.path == writer.path)
    {
        Some(it) => {
            it.whole |= writer.whole;
            it.reach.extend(writer.reach);
        }
        None => list.push(writer),
    }
}

/// The clashes in what `built` builds, each an error.
pub(crate) fn built(built: &Built, elaboration: &Elaboration) -> Vec<(FileId, Diagnostic)> {
    let design = elaboration.design();
    let mut files: FxHashMap<FileId, Drivers> = FxHashMap::default();
    let mut found = Vec::new();
    let mut reported = FxHashSet::default();
    for body in built.frames() {
        let drivers = files
            .entry(body.file)
            .or_insert_with(|| Drivers::new(design, body.file, elaboration.names(body.file)));
        // Each variable's writers in this body, by the frame declaring it.
        let mut writers: FxHashMap<(Vec<usize>, SymbolId), Vec<Writer>> = FxHashMap::default();
        let mut order = Vec::new();
        collect(
            body,
            elaboration,
            drivers,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut |owner, symbol, writer| add(&mut writers, &mut order, (owner, symbol), writer),
        );
        let hir = &design[body.file];
        for key in order {
            // Both are built, so they exist together.
            let Some((first, second)) = drivers.clash(&writers[&key], |_, _| true) else {
                continue;
            };
            if !reported.insert((body.file, key.1, second.at)) {
                continue;
            }
            let name = &hir[key.1].name.text;
            found.push((body.file, drivers.diagnostic(hir, name, first, second)));
        }
    }
    let mut walk = Through {
        elaboration,
        files,
        touches: FxHashMap::default(),
        writers: FxHashMap::default(),
        order: Vec::new(),
        paths: 0,
    };
    for &top in built.tops() {
        let (file, scope) = elaboration.scope(top);
        let hir = &design[file];
        let Some(owner) = hir[scope].owner else {
            continue;
        };
        let path = hir[owner].name.text.clone();
        walk.instance(top, path, FxHashMap::default(), None, 0);
    }
    let Through {
        files,
        writers,
        order,
        ..
    } = walk;
    let sole = |writer: &Writer| files[&writer.file].sole(writer.driver);
    // The interface's own drivers are compared in its body.
    let together = |a: &Writer, b: &Writer| !(a.inside && b.inside);
    let mut paired = FxHashSet::default();
    for key in order {
        let Some((first, second)) = clash(&writers[&key], sole, together) else {
            continue;
        };
        // A body reached by many paths clashes the same way in each.
        let (interface, symbol) = &key;
        if !paired.insert((*symbol, first.at, second.at)) {
            continue;
        }
        let what = |writer: &Writer| files[&writer.file].what(&design[writer.file], writer.driver);
        let name = &design.symbol(*symbol).name.text;
        let message = format!(
            "`{interface}.{name}` is driven by {} in `{}` and by {} in `{}`",
            what(first),
            first.place,
            what(second),
            second.place
        );
        let mut diagnostic = diagnostics::multiple_drivers(message, second.at);
        diagnostic.labels.push(Label {
            at: first.at,
            message: "driven here first".to_string(),
        });
        found.push((second.file, diagnostic));
    }
    found
}

/// A walk of the tree from the tops, path by path, through the instances
/// an interface instance is connected to, collecting the writers of each
/// interface instance's variables.
struct Through<'a> {
    elaboration: &'a Elaboration,
    files: FxHashMap<FileId, Drivers>,
    /// Whether below a body an interface is built or connected.
    touches: FxHashMap<BodyId, bool>,
    /// Each variable's writers, by the path of the interface instance.
    writers: FxHashMap<(Box<str>, SymbolRef), Vec<Writer>>,
    order: Vec<(Box<str>, SymbolRef)>,
    paths: usize,
}

impl Through<'_> {
    /// Walks `body`, built at `path`, its interface ports connected to the
    /// interface instances `ports` holds the paths of; `own` the path of
    /// the interface instance it is the body of, if it is one.
    fn instance(
        &mut self,
        body: BodyId,
        path: Box<str>,
        ports: FxHashMap<SymbolId, Box<str>>,
        own: Option<&str>,
        depth: usize,
    ) {
        if depth > DEPTH || self.paths >= PATHS {
            return;
        }
        self.paths += 1;
        let node = self.paths;
        let (file, _) = self.elaboration.scope(body);
        let design = self.elaboration.design();
        self.files
            .entry(file)
            .or_insert_with(|| Drivers::new(design, file, self.elaboration.names(file)));
        let mut at = Located {
            node,
            file,
            ports: &ports,
            own,
            frames: Vec::new(),
            blocks: Vec::new(),
        };
        self.frame(&mut at, body, path, depth);
    }

    /// Walks `body`, a body or a generate block built in one, at `path`.
    fn frame(&mut self, at: &mut Located, body: BodyId, path: Box<str>, depth: usize) {
        let elaboration = self.elaboration;
        let design = elaboration.design();
        let (file, scope) = elaboration.scope(body);
        at.frames.push((scope, path.clone()));
        let drivers = &self.files[&file];
        let mut found = Vec::new();
        for driver in drivers.located.get(&scope).into_iter().flatten() {
            for access in drivers.through.get(driver).into_iter().flatten() {
                let Some(base) = access.base else { continue };
                if let Some(interface) = at.interface(elaboration, body, file, base) {
                    found.push((interface, access, false));
                }
            }
            // What the interface drives of its own variables.
            if let Some(own) = at.own {
                for access in drivers.writes.get(driver).into_iter().flatten() {
                    let declaring = declaring(&design[file], design.home(access.symbol));
                    if access.base.is_none()
                        && (at.frames.iter()).any(|(scope, _)| Some(*scope) == declaring)
                    {
                        found.push((own.into(), access, true));
                    }
                }
            }
        }
        for (interface, access, inside) in found {
            let mut key_path = vec![at.node];
            key_path.extend(&at.blocks);
            let writer = Writer {
                file,
                driver: access.driver,
                path: key_path,
                place: path.clone(),
                whole: access.whole,
                reach: (access.place)
                    .and_then(|place| elaboration.reach(body, place))
                    .into_iter()
                    .collect(),
                at: access.at,
                inside,
            };
            add(
                &mut self.writers,
                &mut self.order,
                (interface, access.symbol),
                writer,
            );
        }
        for (index, child) in elaboration.children(body).iter().enumerate() {
            let inner: Box<str> = format!("{path}.{}", child.name).into();
            match &child.kind {
                ChildKind::Instance {
                    symbol,
                    definition,
                    body: inner_body,
                    dims,
                } => {
                    let defined = design.symbol(*definition);
                    if design.is_declared_twice(&defined.name.text) {
                        continue;
                    }
                    let mut ports = FxHashMap::default();
                    for (port, connected) in elaboration.interface_connections(body, *symbol) {
                        let interface = match connected {
                            Connected::Expr(expr) => {
                                at.interface(elaboration, body, symbol.file, expr)
                            }
                            Connected::Name(Resolution::Declared(found)) => {
                                at.instance(elaboration, body, found, None)
                            }
                            Connected::Name(_) => None,
                        };
                        if let Some(interface) = interface {
                            ports.insert(port, interface);
                        }
                    }
                    // An array's elements share one body, so what it drives
                    // of its own is not told apart by element.
                    let interface = matches!(
                        defined.kind,
                        SymbolKind::Definition {
                            keyword: INTERFACE_KW,
                            ..
                        }
                    ) && dims.as_ref().is_some_and(|dims| dims.is_empty());
                    if ports.is_empty() && !self.touches(*inner_body) {
                        continue;
                    }
                    let own = interface.then_some(&*inner);
                    self.instance(*inner_body, inner.clone(), ports, own, depth + 1);
                }
                ChildKind::Block { body: inner_body } => {
                    at.blocks.push(index);
                    self.frame(at, *inner_body, inner, depth + 1);
                    at.blocks.pop();
                }
                _ => {}
            }
        }
        at.frames.pop();
    }

    /// Whether below `body`, or in it, an interface is built or connected.
    fn touches(&mut self, body: BodyId) -> bool {
        if let Some(&touches) = self.touches.get(&body) {
            return touches;
        }
        // A definition instantiating itself touches nothing more by it.
        self.touches.insert(body, false);
        let elaboration = self.elaboration;
        let (file, scope) = elaboration.scope(body);
        let hir = &elaboration.design()[file];
        let mut touches = hir[scope].owner.is_some_and(|owner| {
            matches!(
                hir[owner].kind,
                SymbolKind::Definition {
                    keyword: INTERFACE_KW,
                    ..
                }
            )
        });
        for child in elaboration.children(body).iter() {
            if touches {
                break;
            }
            touches = match &child.kind {
                ChildKind::Instance {
                    symbol,
                    body: inner,
                    ..
                } => {
                    !elaboration.interface_connections(body, *symbol).is_empty()
                        || self.touches(*inner)
                }
                ChildKind::Block { body: inner } => self.touches(*inner),
                _ => false,
            };
        }
        self.touches.insert(body, touches);
        touches
    }
}

/// Where a walk through interfaces stands within one instance.
struct Located<'a> {
    /// The instance's place in the walk.
    node: usize,
    /// The file of its definition.
    file: FileId,
    ports: &'a FxHashMap<SymbolId, Box<str>>,
    own: Option<&'a str>,
    /// The scopes of the frames from its body to the one walked, with their
    /// paths.
    frames: Vec<(ScopeId, Box<str>)>,
    /// The frames' places among their parent's children.
    blocks: Vec<usize>,
}

impl Located<'_> {
    /// The path of the interface instance `expr`, written in `file` and
    /// evaluated in `body`, names: an interface port, an interface
    /// instance, an element of an array of them, or a modport of one.
    /// `None` where it is none of them, or sema cannot tell which.
    fn interface(
        &self,
        elaboration: &Elaboration,
        body: BodyId,
        file: FileId,
        expr: ExprId,
    ) -> Option<Box<str>> {
        let design = elaboration.design();
        let hir = &design[file];
        let names = elaboration.names(file);
        let modport = |expr: ExprId| match names.expr(expr) {
            Some(Resolution::Declared(at)) => {
                matches!(design.symbol(at).kind, SymbolKind::Modport(_))
            }
            _ => false,
        };
        let mut head = expr;
        let mut index = None;
        let mut selects = 0;
        loop {
            head = match hir[head].kind {
                ExprKind::Select {
                    base,
                    index: at,
                    range: None,
                } => {
                    index = Some(at);
                    selects += 1;
                    base
                }
                ExprKind::Member { base, .. } if modport(head) => base,
                _ => break,
            };
        }
        let Some(Resolution::Declared(at)) = names.expr(head) else {
            return None;
        };
        if !matches!(hir[head].kind, ExprKind::Name(_)) || selects > 1 {
            return None;
        }
        // An index is evaluated in the body of the file it is written in.
        let index = match index {
            Some(index) if file == elaboration.scope(body).0 => Some(index),
            Some(_) => return None,
            None => None,
        };
        self.instance(elaboration, body, at, index)
    }

    /// The path of the interface instance `at` is, or the element `index`
    /// picks of an array of them, or that an interface port `at` is
    /// connected to.
    fn instance(
        &self,
        elaboration: &Elaboration,
        body: BodyId,
        at: SymbolRef,
        index: Option<ExprId>,
    ) -> Option<Box<str>> {
        let design = elaboration.design();
        if at.file != self.file {
            return None;
        }
        match &design.symbol(at).kind {
            SymbolKind::Port(_) if index.is_none() => self.ports.get(&at.symbol).cloned(),
            SymbolKind::Instance(instance) => {
                let home = design.home(at);
                let (_, path) = self.frames.iter().rev().find(|(scope, _)| *scope == home)?;
                let name = &design.symbol(at).name.text;
                match (instance.dims.len(), index) {
                    (0, None) => Some(format!("{path}.{name}").into()),
                    (1, Some(index)) => match elaboration.eval(body, index) {
                        Value::Bits(bits) => {
                            Some(format!("{path}.{name}[{}]", bits.to_i64()?).into())
                        }
                        _ => None,
                    },
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// Calls `found` for each write a sole driver in `frame` or one inside it
/// makes to a variable a frame around it declares: with the path of the
/// declaring frame, the variable, and the writer.
fn collect(
    frame: &Frame,
    elaboration: &Elaboration,
    drivers: &Drivers,
    path: &mut Vec<usize>,
    frames: &mut Vec<(ScopeId, Vec<usize>)>,
    found: &mut impl FnMut(Vec<usize>, SymbolId, Writer),
) {
    frames.push((frame.scope, path.clone()));
    let hir = &elaboration.design()[frame.file];
    for driver in drivers.located.get(&frame.scope).into_iter().flatten() {
        for access in drivers.writes.get(driver).into_iter().flatten() {
            // The frame that declares the variable: the innermost around
            // this one whose scope holds its declaration.
            let declaring = declaring(hir, elaboration.design().home(access.symbol));
            let Some((_, owner)) = frames
                .iter()
                .rev()
                .find(|(scope, _)| Some(*scope) == declaring)
            else {
                continue;
            };
            found(
                owner.clone(),
                access.symbol.symbol,
                Writer {
                    file: frame.file,
                    driver: *driver,
                    path: path.clone(),
                    place: frame.name.clone(),
                    whole: access.whole,
                    reach: (access.place)
                        .and_then(|place| elaboration.reach(frame.body, place))
                        .into_iter()
                        .collect(),
                    at: access.at,
                    inside: false,
                },
            );
        }
    }
    for (index, block) in frame.blocks.iter().enumerate() {
        path.push(index);
        collect(block, elaboration, drivers, path, frames, found);
        path.pop();
    }
    frames.pop();
}

/// The clashes in the definitions of `file` that `reached` does not name,
/// at their parameters' defaults, each a warning.
pub(crate) fn at_defaults(
    elaboration: &Elaboration,
    file: FileId,
    reached: impl Fn(SymbolId) -> bool,
) -> Vec<Diagnostic> {
    let design = elaboration.design();
    let hir = &design[file];
    let drivers = Drivers::new(design, file, elaboration.names(file));
    let mut writers: FxHashMap<SymbolId, Vec<Writer>> = FxHashMap::default();
    let mut order = Vec::new();
    // In the order written, as the first write is the one a message names
    // first.
    let mut written: Vec<&Access> = drivers.writes.values().flatten().collect();
    written.sort_by_key(|access| (access.at.src_id, access.at.start));
    for access in written {
        let Some(within) = access.within.filter(|&it| !reached(it)) else {
            continue;
        };
        let body = elaboration.body(SymbolRef {
            file,
            symbol: within,
        });
        let reach =
            (access.place.zip(body)).and_then(|(place, body)| elaboration.reach(body, place));
        let writer = Writer {
            file,
            driver: access.driver,
            path: Vec::new(),
            place: "".into(),
            whole: access.whole,
            reach: reach.into_iter().collect(),
            at: access.at,
            inside: false,
        };
        add(&mut writers, &mut order, access.symbol.symbol, writer);
    }
    let mut found = Vec::new();
    for symbol in order {
        let together = |a: &Writer, b: &Writer| drivers.together(a.driver, b.driver);
        let Some((first, second)) = drivers.clash(&writers[&symbol], together) else {
            continue;
        };
        let name = &hir[symbol].name.text;
        let mut diagnostic = drivers.diagnostic(hir, name, first, second);
        diagnostic.severity = Severity::Warning;
        let note = "at its definition's parameter defaults, with every generate branch, \
                    so an error only if elaboration builds both";
        diagnostic.notes.push(note.to_string());
        found.push(diagnostic);
    }
    found
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

/// Whether `at` is declared in an interface, or in a generate block of one.
fn in_interface(design: &Design, at: SymbolRef) -> bool {
    let hir = &design[at.file];
    let mut scope = Some(design.home(at));
    while let Some(id) = scope {
        if let ScopeKind::Definition = hir[id].kind {
            return hir[id].owner.is_some_and(|owner| {
                matches!(
                    hir[owner].kind,
                    SymbolKind::Definition {
                        keyword: INTERFACE_KW,
                        ..
                    }
                )
            });
        }
        scope = hir[id].parent;
    }
    false
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
