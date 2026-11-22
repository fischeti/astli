//! What a load of a design gives the server: the design analysed as `check`
//! analyses it, and what went wrong on the way, all of it in the library
//! crates' types, so that a request reads it without the driver.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use astli_index::Index;
use astli_sema::{Elaboration, FileId};
use astli_text::{Diagnostic, LineCol, Origins, Severity};

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
