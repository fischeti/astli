//! Which file, and where in it.

use std::fmt;

/// Identifies one buffer of text: a file that was read, or text that expansion
/// produced.
///
/// Synthesised text gets an id of its own rather than a special case, because
/// everything that reads a [`Span`] wants the same answer -- give me the bytes
/// -- and only a diagnostic ever needs to know the difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(pub(crate) u32);

impl FileId {
    /// The index this id was handed out at, for a caller keeping its own table
    /// alongside.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A half-open byte range in one file.
///
/// Spans carry their file. A range on its own is ambiguous the moment a second
/// file exists, and `` `include `` means a second file always eventually does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(file: FileId, start: u32, end: u32) -> Span {
        debug_assert!(start <= end, "a span may not run backwards");
        Span { file, start, end }
    }

    /// A span of no width, for pointing between two things.
    pub fn point(file: FileId, at: u32) -> Span {
        Span::new(file, at, at)
    }

    pub fn len(self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Whether `other` lies within this span. False across files, which is the
    /// useful answer rather than a refusal: two spans in different files never
    /// contain one another.
    pub fn contains(self, other: Span) -> bool {
        self.file == other.file && self.start <= other.start && other.end <= self.end
    }

    /// The smallest span covering both, or `None` if they are in different
    /// files -- which is not an error, only a question with no answer. A macro
    /// call and the body it pulls in are routinely in different files.
    pub fn cover(self, other: Span) -> Option<Span> {
        (self.file == other.file).then(|| Span {
            file: self.file,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        })
    }
}

/// A 1-based line and column, as a message writes them.
///
/// The column counts *characters*, not bytes, so a comment in another script
/// does not push the caret sideways. An editor that wants UTF-16 code units
/// will need its own conversion; nothing here has an editor yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

impl fmt::Display for LineCol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}
