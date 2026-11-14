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

use std::collections::hash_map::Entry;
use std::hash::Hash;
use std::ops::Range;

use astli_syntax::SyntaxKind::{self, *};
use astli_text::{Diagnostic, Label, Severity, Span};
use rayon::prelude::*;
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
    /// What each writes, certainly, of the file's variables, but for those
    /// `through` holds.
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
            } else if access.symbol.file == file {
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
                | Member::Defparam(_)
                | Member::Alias(_)
                | Member::DefaultDisable(_)
                | Member::DefaultClocking(_)
                | Member::ElaborationTask(_)
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
///
/// Two writers write some bit both where one writes all of the variable or
/// the parts they write overlap; a part whose bits sema cannot tell clashes
/// only with a write to all of it. A variable written in each iteration of
/// a loop has a writer per iteration, so rather than comparing every pair,
/// each writer of all of it is paired with the others, and the parts are
/// swept in the order of their bits.
fn clash(
    list: &[Writer],
    sole: impl Fn(&Writer) -> bool,
    together: impl Fn(&Writer, &Writer) -> bool,
) -> Option<(&Writer, &Writer)> {
    let clashes = |(second, first): (usize, usize)| {
        let (a, b) = (&list[first], &list[second]);
        (sole(a) || sole(b)) && together(a, b)
    };
    // The clash found, as (second, first): the earliest second, and for it
    // the earliest first.
    let mut best: Option<(usize, usize)> = None;
    for (whole, _) in list.iter().enumerate().filter(|(_, it)| it.whole) {
        if best.is_some_and(|(second, _)| second < whole) {
            break;
        }
        // In the order `best` ranks them.
        let earlier = (0..whole).map(|first| (whole, first));
        let later = (whole + 1..list.len()).map(|second| (second, whole));
        for pair in earlier.chain(later) {
            if best.is_some_and(|best| best <= pair) {
                break;
            }
            if clashes(pair) {
                best = Some(pair);
                break;
            }
        }
    }
    let mut parts: Vec<(u64, u64, usize)> = (list.iter().enumerate())
        .filter(|(_, it)| !it.whole)
        .flat_map(|(at, it)| (it.reach.iter()).map(move |part| (part.start, part.end, at)))
        .collect();
    parts.sort_unstable();
    // The parts begun before this one that end after it begins, by their
    // end and writer.
    let mut open: Vec<(u64, usize)> = Vec::new();
    for (start, end, at) in parts {
        open.retain(|&(end, _)| end > start);
        for &(_, other) in &open {
            let pair = (at.max(other), at.min(other));
            if other != at && best.is_none_or(|best| pair < best) && clashes(pair) {
                best = Some(pair);
            }
        }
        open.push((end, at));
    }
    best.map(|(second, first)| (&list[first], &list[second]))
}

/// One driver's writes to a variable, in one frame.
struct Writer {
    /// The file the driver stands in.
    file: FileId,
    driver: Driver,
    /// The frame the driver stands in, numbered as the walk reaches it; 0
    /// at the defaults.
    frame: usize,
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

/// Each variable's writers, one per driver and frame, the variables in the
/// order first written.
struct Writers<K> {
    lists: Vec<(K, Vec<Writer>)>,
    /// Each variable's place in `lists`.
    by_key: FxHashMap<K, usize>,
    /// Each writer's place in its list, by the list's place, the driver and
    /// the frame. A variable written in every iteration of a loop has a
    /// writer per iteration, so searching the list would be quadratic.
    by_writer: FxHashMap<(usize, Driver, usize), usize>,
}

impl<K: Clone + Eq + Hash> Writers<K> {
    fn new() -> Self {
        Writers {
            lists: Vec::new(),
            by_key: FxHashMap::default(),
            by_writer: FxHashMap::default(),
        }
    }

    /// Adds `writer` to the writers of `key`, merged with the one of the
    /// same driver and frame.
    fn add(&mut self, key: K, writer: Writer) {
        let list = match self.by_key.get(&key) {
            Some(&list) => list,
            None => {
                self.by_key.insert(key.clone(), self.lists.len());
                self.lists.push((key, Vec::new()));
                self.lists.len() - 1
            }
        };
        let writers = &mut self.lists[list].1;
        match self.by_writer.entry((list, writer.driver, writer.frame)) {
            Entry::Occupied(at) => {
                let it = &mut writers[*at.get()];
                it.whole |= writer.whole;
                it.reach.extend(writer.reach);
            }
            Entry::Vacant(at) => {
                at.insert(writers.len());
                writers.push(writer);
            }
        }
    }
}

/// The clashes in what `built` builds, each an error.
pub(crate) fn built(built: &Built, elaboration: &Elaboration) -> Vec<(FileId, Diagnostic)> {
    let design = elaboration.design();
    let bodies = built.frames();
    let mut files: Vec<FileId> = bodies.iter().map(|body| body.file).collect();
    files.sort_unstable();
    files.dedup();
    let files: FxHashMap<FileId, Drivers> = (files.into_par_iter())
        .map(|file| (file, Drivers::new(design, file, elaboration.names(file))))
        .collect();
    // Each body's clashes are its own, so they are found apart.
    let clashes: Vec<_> = (bodies.par_iter())
        .map(|body| in_body(body, elaboration, &files[&body.file]))
        .collect();
    let mut found = Vec::new();
    // A body built twice, at other parameters, may clash the same way.
    let mut reported = FxHashSet::default();
    for (file, symbol, diagnostic) in clashes.into_iter().flatten() {
        if reported.insert((file, symbol, diagnostic.at)) {
            found.push((file, diagnostic));
        }
    }
    let mut walk = Through {
        elaboration,
        files,
        touches: FxHashMap::default(),
        writers: Writers::new(),
        paths: 0,
        frames: 0,
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
    let Through { files, writers, .. } = walk;
    let sole = |writer: &Writer| files[&writer.file].sole(writer.driver);
    // The interface's own drivers are compared in its body.
    let together = |a: &Writer, b: &Writer| !(a.inside && b.inside);
    let mut paired = FxHashSet::default();
    for ((interface, symbol), list) in &writers.lists {
        let Some((first, second)) = clash(list, sole, together) else {
            continue;
        };
        // A body reached by many paths clashes the same way in each.
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

/// The clashes in `body` and the blocks built in it, each with its file and
/// variable.
fn in_body(
    body: &Frame,
    elaboration: &Elaboration,
    drivers: &Drivers,
) -> Vec<(FileId, SymbolId, Diagnostic)> {
    // Each variable's writers, by the frame declaring it.
    let mut writers = Writers::new();
    collect(
        body,
        elaboration,
        drivers,
        &mut 0,
        &mut Vec::new(),
        &mut |owner, symbol, writer| writers.add((owner, symbol), writer),
    );
    let hir = &elaboration.design()[body.file];
    let mut found = Vec::new();
    for ((_, symbol), list) in &writers.lists {
        // Both are built, so they exist together.
        if let Some((first, second)) = drivers.clash(list, |_, _| true) {
            let name = &hir[*symbol].name.text;
            let diagnostic = drivers.diagnostic(hir, name, first, second);
            found.push((body.file, *symbol, diagnostic));
        }
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
    writers: Writers<(Box<str>, SymbolRef)>,
    /// The instances walked.
    paths: usize,
    /// The frames walked, each instance's body and each block built in one.
    frames: usize,
}

impl Through<'_> {
    /// Walks `body`, built at `path`, its interface ports connected to the
    /// interface instances `ports` holds; `own` the path of
    /// the interface instance it is the body of, if it is one.
    fn instance(
        &mut self,
        body: BodyId,
        path: Box<str>,
        ports: FxHashMap<SymbolId, Target>,
        own: Option<&str>,
        depth: usize,
    ) {
        if depth > DEPTH || self.paths >= PATHS {
            return;
        }
        self.paths += 1;
        let (file, _) = self.elaboration.scope(body);
        let design = self.elaboration.design();
        self.files
            .entry(file)
            .or_insert_with(|| Drivers::new(design, file, self.elaboration.names(file)));
        let mut at = Located {
            file,
            ports: &ports,
            own,
            frames: Vec::new(),
        };
        self.frame(&mut at, body, path, depth);
    }

    /// Walks `body`, a body or a generate block built in one, at `path`.
    fn frame(&mut self, at: &mut Located, body: BodyId, path: Box<str>, depth: usize) {
        let elaboration = self.elaboration;
        let design = elaboration.design();
        let (file, scope) = elaboration.scope(body);
        at.frames.push((scope, path.clone()));
        let frame = self.frames;
        self.frames += 1;
        let drivers = &self.files[&file];
        let mut found = Vec::new();
        for driver in drivers.located.get(&scope).into_iter().flatten() {
            for access in drivers.through.get(driver).into_iter().flatten() {
                let Some(base) = access.base else { continue };
                if let Some(Target::One(interface)) = at.interface(elaboration, body, file, base) {
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
            let writer = Writer {
                file,
                driver: access.driver,
                frame,
                place: path.clone(),
                whole: access.whole,
                reach: (access.place)
                    .and_then(|place| elaboration.reach(body, place))
                    .into_iter()
                    .collect(),
                at: access.at,
                inside,
            };
            self.writers.add((interface, access.symbol), writer);
        }
        for child in elaboration.children(body).iter() {
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
                        let port = SymbolRef {
                            file: definition.file,
                            symbol: port,
                        };
                        if let Some(target) =
                            interface.and_then(|it| it.ported(elaboration, *inner_body, port))
                        {
                            ports.insert(port.symbol, target);
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
                    self.frame(at, *inner_body, inner, depth + 1);
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
    /// The file of its definition.
    file: FileId,
    ports: &'a FxHashMap<SymbolId, Target>,
    own: Option<&'a str>,
    /// The scopes of the frames from its body to the one walked, with their
    /// paths.
    frames: Vec<(ScopeId, Box<str>)>,
}

impl Located<'_> {
    /// The interface instance, or the array of them, `expr`, written in
    /// `file` and evaluated in `body`, names: an interface port, an
    /// interface instance, an element of an array of them, or a modport of
    /// one. `None` where it is none of them, or sema cannot tell which.
    fn interface(
        &self,
        elaboration: &Elaboration,
        body: BodyId,
        file: FileId,
        expr: ExprId,
    ) -> Option<Target> {
        let (at, index) = reference(elaboration.design(), elaboration.names(file), file, expr)?;
        // An index is evaluated in the body of the file it is written in.
        let index = match index {
            Some(index) if file == elaboration.scope(body).0 => Some(index),
            Some(_) => return None,
            None => None,
        };
        self.instance(elaboration, body, at, index)
    }

    /// The interface instance, or the array of them, `at` is, or the
    /// element `index` picks of an array; or that an interface port `at` is
    /// connected to, or the element `index` picks of it.
    fn instance(
        &self,
        elaboration: &Elaboration,
        body: BodyId,
        at: SymbolRef,
        index: Option<ExprId>,
    ) -> Option<Target> {
        let design = elaboration.design();
        if at.file != self.file {
            return None;
        }
        let value = |index: ExprId| match elaboration.eval(body, index) {
            Value::Bits(bits) => bits.to_i64(),
            _ => None,
        };
        match &design.symbol(at).kind {
            SymbolKind::Port(_) => {
                let target = self.ports.get(&at.symbol)?;
                match index {
                    None => Some(target.clone()),
                    Some(at) => target.element(value(at)?).map(Target::One),
                }
            }
            SymbolKind::Instance(instance) => {
                let home = design.home(at);
                let (_, path) = self.frames.iter().rev().find(|(scope, _)| *scope == home)?;
                let name = &design.symbol(at).name.text;
                match (&*instance.dims, index) {
                    ([], None) => Some(Target::One(format!("{path}.{name}").into())),
                    ([_], Some(at)) => {
                        Some(Target::One(format!("{path}.{name}[{}]", value(at)?).into()))
                    }
                    ([dim], None) => {
                        let (left, right) = elaboration.unpacked_bounds(body, dim)?;
                        Some(Target::Array {
                            base: format!("{path}.{name}").into(),
                            left,
                            right,
                            start: left,
                            step: if right < left { -1 } else { 1 },
                        })
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// What an interface port or a name stands for: an interface instance, or
/// an array of them, by its path.
#[derive(Clone)]
enum Target {
    One(Box<str>),
    /// An array, indexed from `left` to `right`, of which the element
    /// `place` places from `left` is `base[start + step * place]`. An
    /// array port indexes the array connected to it by its own bounds.
    Array {
        base: Box<str>,
        left: i64,
        right: i64,
        start: i64,
        step: i64,
    },
}

impl Target {
    /// The path of the element `index` picks; `None` for one out of
    /// bounds, or an index into an interface instance that is no array.
    fn element(&self, index: i64) -> Option<Box<str>> {
        let Target::Array {
            base,
            left,
            right,
            start,
            step,
        } = self
        else {
            return None;
        };
        let place = if right < left {
            left - index
        } else {
            index - left
        };
        if place < 0 || place > (right - left).abs() {
            return None;
        }
        Some(format!("{base}[{}]", start + step * place).into())
    }

    /// `self` as the interface port `port` of `body` sees it: an array
    /// port's elements match the array's left to left, so the port takes
    /// it indexed by its own bounds. `None` where their shapes differ, or
    /// sema cannot tell, as for an array connected to each instance of an
    /// array of them, which share one body.
    fn ported(self, elaboration: &Elaboration, body: BodyId, port: SymbolRef) -> Option<Target> {
        let SymbolKind::Port(Port { ty, .. }) = &elaboration.design().symbol(port).kind else {
            return None;
        };
        match (self, &ty.dims[ty.packed..]) {
            (one @ Target::One(_), []) => Some(one),
            (
                Target::Array {
                    base,
                    left,
                    right,
                    start,
                    step,
                },
                [dim],
            ) => {
                let (port_left, port_right) = elaboration.unpacked_bounds(body, dim)?;
                if (port_right - port_left).abs() != (right - left).abs() {
                    return None;
                }
                Some(Target::Array {
                    base,
                    left: port_left,
                    right: port_right,
                    start,
                    step,
                })
            }
            _ => None,
        }
    }
}

/// What `expr`, written in `file`, names an interface instance by: an
/// interface port or an interface instance, with the index that picks an
/// element of an array of them, and any modport of it dropped. `None` where
/// it is none of those, or more than one index deep.
fn reference(
    design: &Design,
    names: &Names,
    file: FileId,
    expr: ExprId,
) -> Option<(SymbolRef, Option<ExprId>)> {
    let hir = &design[file];
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
    Some((at, index))
}

/// Calls `found` for each write a sole driver in `frame` or one inside it
/// makes to a variable a frame around it declares: with the number of the
/// declaring frame, the variable, and the writer. `next` numbers the frames
/// as they are reached, and `frames` holds those around this one.
fn collect(
    frame: &Frame,
    elaboration: &Elaboration,
    drivers: &Drivers,
    next: &mut usize,
    frames: &mut Vec<(ScopeId, usize)>,
    found: &mut impl FnMut(usize, SymbolId, Writer),
) {
    let id = *next;
    *next += 1;
    frames.push((frame.scope, id));
    let hir = &elaboration.design()[frame.file];
    for driver in drivers.located.get(&frame.scope).into_iter().flatten() {
        for access in drivers.writes.get(driver).into_iter().flatten() {
            // The frame that declares the variable: the innermost around
            // this one whose scope holds its declaration.
            let declaring = declaring(hir, elaboration.design().home(access.symbol));
            let Some(&(_, owner)) = frames
                .iter()
                .rev()
                .find(|(scope, _)| Some(*scope) == declaring)
            else {
                continue;
            };
            found(
                owner,
                access.symbol.symbol,
                Writer {
                    file: frame.file,
                    driver: *driver,
                    frame: id,
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
    for block in &frame.blocks {
        collect(block, elaboration, drivers, next, frames, found);
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
    let mut writers = Writers::new();
    // In the order written, as the first write is the one a message names
    // first.
    let mut written: Vec<&Access> = (drivers.writes.values().flatten())
        .chain(drivers.through.values().flatten())
        .collect();
    written.sort_by_key(|access| (access.at.src_id, access.at.start));
    for access in written {
        let Some(within) = access.within.filter(|&it| !reached(it)) else {
            continue;
        };
        let body = elaboration.body(SymbolRef {
            file,
            symbol: within,
        });
        // A variable of an interface is one per interface instance, and each
        // element of an array of them is an instance of its own; so a write
        // through a port or an instance is to the variable of the one it
        // names. Where the walk cannot tell which, as for an index it cannot
        // evaluate, two writes may reach different instances, and neither
        // is compared. Two ports may be connected to one instance, which
        // only an elaboration tells.
        let through = match access.base {
            Some(base) if in_interface(design, access.symbol) => {
                let names = elaboration.names(file);
                let Some((head, index)) = reference(design, names, file, base) else {
                    continue;
                };
                let index = match (index, body) {
                    (None, _) => None,
                    (Some(index), Some(body)) => match elaboration.eval(body, index) {
                        Value::Bits(bits) => match bits.to_i64() {
                            Some(index) => Some(index),
                            None => continue,
                        },
                        _ => continue,
                    },
                    (Some(_), None) => continue,
                };
                Some((head, index))
            }
            _ => None,
        };
        let reach =
            (access.place.zip(body)).and_then(|(place, body)| elaboration.reach(body, place));
        let writer = Writer {
            file,
            driver: access.driver,
            frame: 0,
            place: "".into(),
            whole: access.whole,
            reach: reach.into_iter().collect(),
            at: access.at,
            inside: false,
        };
        writers.add((access.symbol, through), writer);
    }
    let mut found = Vec::new();
    for ((symbol, through), list) in &writers.lists {
        let together = |a: &Writer, b: &Writer| drivers.together(a.driver, b.driver);
        let Some((first, second)) = drivers.clash(list, together) else {
            continue;
        };
        let name = &design.symbol(*symbol).name.text;
        let name = match through {
            None => name.to_string(),
            Some((head, None)) => format!("{}.{name}", design.symbol(*head).name.text),
            Some((head, Some(index))) => {
                format!("{}[{index}].{name}", design.symbol(*head).name.text)
            }
        };
        let mut diagnostic = drivers.diagnostic(hir, &name, first, second);
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
