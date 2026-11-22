//! What a load of a design gives the server: the design analysed as `check`
//! analyses it, and what went wrong on the way, all of it in the library
//! crates' types, so that a request reads it without the driver.

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use astli_index::Index;
use astli_lint::Config;
use astli_preproc::Build;
use astli_sema::{BodyId, Built, Design, Elaboration, FileId, Sites};
use astli_text::{Diagnostic, ExpansionId, LineCol, Origins, Severity, Span};

/// A design root's design as one load found it, never changed once built.
///
/// A design root is the root of the package a file is in, as `check` with
/// no arguments finds it from the file's directory, or the file itself
/// outside any package. Open files with the same root share a snapshot. A
/// workspace member is a root of its own, read with its dependencies, its
/// siblings among them, as `check` in its directory reads it; a sibling's
/// file opened is in the sibling's snapshot, not the member's.
pub struct Snapshot {
    /// The design root it was loaded from.
    pub root: PathBuf,
    /// Each design `check` analyses of the package: one for each set of
    /// defines its tops give, usually one. None if the load failed before
    /// it analysed any, which `problems` then says why.
    pub designs: Vec<Analysed>,
    /// What the load found wrong that is no unit's parse: a manifest, a
    /// dependency, a file that could not be read. In the order found.
    pub problems: Vec<Problem>,
    /// How many units the load expanded, parsed and lowered; it took the
    /// rest from an earlier load.
    pub lowered: usize,
    /// Whether the load read the package without its dependencies, since it
    /// could not settle or read them, as `problems` says: every name they
    /// declare is then unknown, so what the design makes of that is no
    /// finding to show.
    pub lacks_dependencies: bool,
    /// The levels `lint` reads each file of the root package at, headers
    /// too, by path, as its units name it; a file not here is not linted.
    /// Files at the same levels share them. Empty if the configuration
    /// does not read, which `problems` then says.
    pub levels: HashMap<PathBuf, Arc<Config>>,
}

impl Snapshot {
    /// How many units its designs hold, a unit counted once for each design
    /// that has it.
    pub fn units(&self) -> usize {
        self.designs.iter().map(|it| it.units.len()).sum()
    }
}

/// One design of a snapshot, lowered and elaborated.
pub struct Analysed {
    pub elaboration: Elaboration,
    /// What each unit declares and uses.
    pub index: Index,
    /// Each unit, in the order of the design's files.
    pub units: Vec<Unit>,
    /// The body of each top the design is elaborated from, which what is
    /// built is walked down from; none where the manifest names no top.
    pub tops: Vec<BodyId>,
    /// What the tops build, made when first asked for.
    built: OnceLock<Built>,
}

impl Analysed {
    pub fn new(
        elaboration: Elaboration,
        index: Index,
        units: Vec<Unit>,
        tops: Vec<BodyId>,
    ) -> Self {
        Analysed {
            elaboration,
            index,
            units,
            tops,
            built: OnceLock::new(),
        }
    }

    /// What the tops build, walked down from them the first time anything
    /// asks, since that walks the whole design: diagnostics on the load's
    /// thread, or a request. `None` where there are no tops.
    pub fn built(&self) -> Option<&Built> {
        (!self.tops.is_empty())
            .then(|| (self.built).get_or_init(|| Built::new(&self.elaboration, &self.tops)))
    }

    /// The unit of `file`.
    pub fn unit(&self, file: FileId) -> Option<&Unit> {
        // The units are in the order of the design's files.
        match self.units.get(file.index()) {
            Some(unit) if unit.file == file => Some(unit),
            _ => self.units.iter().find(|it| it.file == file),
        }
    }
}

/// A file compiled on its own, as its design holds it.
pub struct Unit {
    pub path: PathBuf,
    /// Its file in the design.
    pub file: FileId,
    /// What its spans resolve against, the unit's alone.
    pub origins: Arc<Origins>,
    /// What its expansion and its parse reported, in the order found.
    pub parsed: Vec<Diagnostic>,
    /// Whether `check` reports its findings: a file of the root package, or
    /// of a dependency read from disk, among those the tops reach if there
    /// are tops.
    pub checked: bool,
    /// Whether the rules of `lint` that read a design report its findings:
    /// a file of the root package with its levels in
    /// [`Snapshot::levels`], among those the tops reach if there are tops.
    pub linted: bool,
    /// Each macro call its files write, its own or a header's, as its
    /// expansion listed them: the outermost, in the order expanded. Shared,
    /// as a unit kept is handed to each load.
    pub calls: Arc<[ExpansionId]>,
    /// The body of each branch its expansion did not take, as written.
    pub skipped: Arc<[Span]>,
    /// The include directories and defines it was expanded with, which
    /// expanding it again from the texts its origins hold takes. Units of
    /// one build share it.
    pub build: Arc<Build>,
    /// What requests find of it and keep for the next; a load makes it
    /// empty, with `Memo::default()`.
    pub memo: Memo,
}

impl Unit {
    /// Each name the unit's HIR holds, by where it is spelled, made on the
    /// first ask from `design`, the one its [`Analysed`] holds.
    pub fn sites(&self, design: &Design) -> &Sites {
        (self.memo.sites).get_or_init(|| Sites::new(design, self.file, &self.origins))
    }
}

/// What requests have found of a unit, kept with it: a snapshot outlives a
/// request, and finding it again would cost each request a walk of the
/// unit's HIR.
#[derive(Default)]
pub struct Memo {
    sites: OnceLock<Sites>,
}

/// Something wrong with a load that is no finding of a unit, as plain text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub severity: Severity,
    pub message: String,
    /// What explains it.
    pub notes: Vec<String>,
    /// What to do about it.
    pub help: Vec<String>,
    /// Where it is, if in a file.
    pub at: Option<At>,
}

/// Where in a file a problem is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct At {
    /// What the problem is called, as a diagnostic's code.
    pub code: String,
    pub path: PathBuf,
    pub place: Place,
    /// What is at the place, if it says more than the message.
    pub label: Option<String>,
}

/// A place in a file, in the terms whatever found it knows it by: the file
/// it is in is read again to find it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// A name of so many characters, starting at the position.
    Name(LineCol, usize),
    /// A whole line, counted from one.
    Line(usize),
    /// A run of bytes.
    Bytes(Range<usize>),
}
