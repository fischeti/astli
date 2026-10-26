//! Variables two things drive where the standard allows one: an
//! `always_comb`, `always_ff` or `always_latch` block, a continuous
//! assignment or an instance's output, and anything else that writes the
//! same bits.
//!
//! What the tops build is compared exactly: each body with its parameters,
//! each generate block built a place of its own. A definition no top
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
use crate::elab::Elaboration;
use crate::hir::*;
use crate::resolve::Names;

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
}

impl Drivers {
    fn new(design: &Design, file: FileId, names: &Names) -> Drivers {
        let hir = &design[file];
        let mut drivers = Drivers {
            located: FxHashMap::default(),
            kinds: FxHashMap::default(),
            arms: FxHashMap::default(),
            writes: FxHashMap::default(),
        };
        drivers.place(hir, hir.root(), &mut Vec::new());
        for access in accesses(design, file, names) {
            if access.write
                && access.certain
                && access.symbol.file == file
                && drivers.arms.contains_key(&access.driver)
                && variable(hir, access.symbol.symbol)
            {
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
    /// one: some bit both write, one of them a sole driver, and both
    /// existing together as `together` tells.
    fn clash<'a>(
        &self,
        list: &'a [Writer],
        together: impl Fn(&Writer, &Writer) -> bool,
    ) -> Option<(&'a Writer, &'a Writer)> {
        list.iter().enumerate().find_map(|(at, second)| {
            let first = list[..at].iter().find(|first| {
                first.overlaps(second)
                    && (self.sole(first.driver) || self.sole(second.driver))
                    && together(first, second)
            })?;
            Some((first, second))
        })
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

/// One driver's writes to a variable, in one frame.
struct Writer {
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
    found
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
                    driver: *driver,
                    path: path.clone(),
                    place: frame.name.clone(),
                    whole: access.whole,
                    reach: (access.place)
                        .and_then(|place| elaboration.reach(frame.body, place))
                        .into_iter()
                        .collect(),
                    at: access.at,
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
            driver: access.driver,
            path: Vec::new(),
            place: "".into(),
            whole: access.whole,
            reach: reach.into_iter().collect(),
            at: access.at,
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
