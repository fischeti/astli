//! File identifiers, source byte spans, and line/column positions.

use std::fmt;

/// Unique identifier for a loaded source file or synthesized macro expansion buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(pub(crate) u32);

impl FileId {
    /// Returns the zero-based numeric index of the file.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A half-open byte range `[start, end)` within a specific file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    /// Creates a new byte span within the specified file.
    pub fn new(file: FileId, start: u32, end: u32) -> Span {
        debug_assert!(start <= end, "span start cannot exceed end");
        Span { file, start, end }
    }

    /// Creates an empty zero-width span at the specified byte offset.
    pub fn point(file: FileId, at: u32) -> Span {
        Span::new(file, at, at)
    }

    /// Returns the length of the span in bytes.
    pub fn len(self) -> u32 {
        self.end - self.start
    }

    /// Returns `true` if the span has zero length.
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Returns `true` if `other` is entirely contained within this span.
    ///
    /// Returns `false` if `self` and `other` belong to different files.
    pub fn contains(self, other: Span) -> bool {
        self.file == other.file && self.start <= other.start && other.end <= self.end
    }

    /// Returns the smallest span covering both `self` and `other`.
    ///
    /// Returns `None` if `self` and `other` belong to different files.
    pub fn cover(self, other: Span) -> Option<Span> {
        (self.file == other.file).then(|| Span {
            file: self.file,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        })
    }
}

/// 1-based line and character column position in source text.
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
