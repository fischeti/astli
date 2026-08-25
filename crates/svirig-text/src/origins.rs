//! The store: every buffer of text in a compilation, and how each got there.

use std::path::{Path, PathBuf};

use crate::span::{FileId, LineCol, Span};

/// Identifies one macro expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExpansionId(u32);

/// One macro expansion: a call, and the definition it pulled in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expansion {
    /// The name as written at the call, `` `FOO `` included. Kept as a span so
    /// that a message quotes the source rather than a reconstruction.
    pub name: Span,
    /// The whole call, `` `FOO(a, b) ``.
    pub call: Span,
    /// The `` `define `` that supplied the text.
    pub def: Span,
    /// The expansion this one happened inside, when the call was itself
    /// produced by expanding something else.
    pub parent: Option<ExpansionId>,
}

/// Where one token's text was written, and how it reached where it is used.
///
/// This is per *token*, which is the whole reason the awkward case is not
/// awkward. In `` `define M(x) f(x) `` used as `` `M(a+b) ``, the `f` is
/// spelled in the body and the `a` is spelled in the argument at the call site
/// -- two different files, potentially -- yet both are placed by the same
/// expansion. A byte-oriented map has to swap the roles of "written here" and
/// "expanded there" to express that. Recording the spelling on each token
/// instead makes it fall out: the two tokens simply have different `spelled`
/// spans and the same `from`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Origin {
    /// Where the bytes are. Always a real location -- a file, or a buffer
    /// synthesised by pasting or stringification.
    pub spelled: Span,
    /// The innermost expansion that placed it, if any. `None` means the token
    /// is used where it was written.
    pub from: Option<ExpansionId>,
}

impl Origin {
    /// A token written where it is used.
    pub fn written(spelled: Span) -> Origin {
        Origin {
            spelled,
            from: None,
        }
    }
}

/// Why a buffer exists.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Provenance {
    /// Read from disk, or handed over as text. `included_from` is the
    /// `` `include `` that pulled it in, and `None` for a file named on the
    /// command line.
    File {
        path: PathBuf,
        included_from: Option<Span>,
    },
    /// Text that is in no file, because ``` `` ``` pasted it together or
    /// `` `" `` stringified it.
    Synthesised { by: ExpansionId },
}

struct Buffer {
    provenance: Provenance,
    text: String,
    /// The byte offset each line starts at. Always begins with 0, so the count
    /// is the number of lines and a binary search never comes back empty.
    lines: Vec<u32>,
}

/// Every buffer of text in a compilation, and where each byte of it came from.
///
/// Named for the question it answers. Ask it for the bytes behind a [`Span`],
/// for the line and column to print, or -- given an [`Origin`] -- for the chain
/// of macro calls a token arrived through.
///
/// One store holds files and expansions together because they refer to each
/// other: an expansion's spans point into files, and a synthesised buffer is
/// created *by* an expansion. Splitting them would only move the cycle.
#[derive(Default)]
pub struct Origins {
    buffers: Vec<Buffer>,
    expansions: Vec<Expansion>,
}

impl Origins {
    pub fn new() -> Origins {
        Origins::default()
    }

    /// Adds a file named directly, rather than reached through an
    /// `` `include ``.
    pub fn add_file(&mut self, path: impl Into<PathBuf>, text: String) -> FileId {
        self.add(
            Provenance::File {
                path: path.into(),
                included_from: None,
            },
            text,
        )
    }

    /// Adds a file reached through the `` `include `` at `from`.
    pub fn add_included(&mut self, path: impl Into<PathBuf>, text: String, from: Span) -> FileId {
        self.add(
            Provenance::File {
                path: path.into(),
                included_from: Some(from),
            },
            text,
        )
    }

    /// Adds text that no file contains, produced by `by`.
    pub fn add_synthesised(&mut self, text: String, by: ExpansionId) -> FileId {
        self.add(Provenance::Synthesised { by }, text)
    }

    /// Records an expansion, so that tokens it places can point back at it.
    pub fn expand(&mut self, expansion: Expansion) -> ExpansionId {
        self.expansions.push(expansion);
        ExpansionId(self.expansions.len() as u32 - 1)
    }

    fn add(&mut self, provenance: Provenance, text: String) -> FileId {
        let mut lines = vec![0];
        lines.extend(
            text.bytes()
                .enumerate()
                .filter(|&(_, byte)| byte == b'\n')
                .map(|(at, _)| at as u32 + 1),
        );
        self.buffers.push(Buffer {
            provenance,
            text,
            lines,
        });
        FileId(self.buffers.len() as u32 - 1)
    }

    pub fn text(&self, file: FileId) -> &str {
        &self.buffers[file.index()].text
    }

    /// The bytes a span covers.
    pub fn slice(&self, span: Span) -> &str {
        &self.text(span.file)[span.start as usize..span.end as usize]
    }

    /// The file's path, or `None` for a buffer expansion synthesised.
    pub fn path(&self, file: FileId) -> Option<&Path> {
        match &self.buffers[file.index()].provenance {
            Provenance::File { path, .. } => Some(path),
            Provenance::Synthesised { .. } => None,
        }
    }

    /// The `` `include `` that pulled this file in, if one did.
    pub fn included_from(&self, file: FileId) -> Option<Span> {
        match &self.buffers[file.index()].provenance {
            Provenance::File { included_from, .. } => *included_from,
            Provenance::Synthesised { .. } => None,
        }
    }

    /// The chain of `` `include `` sites above this file, innermost first.
    pub fn include_trace(&self, file: FileId) -> impl Iterator<Item = Span> {
        std::iter::successors(self.included_from(file), |span| {
            self.included_from(span.file)
        })
    }

    pub fn expansion(&self, id: ExpansionId) -> &Expansion {
        &self.expansions[id.0 as usize]
    }

    /// Where a byte offset falls, counting from 1.
    pub fn line_col(&self, file: FileId, offset: u32) -> LineCol {
        let buffer = &self.buffers[file.index()];
        // `lines` starts at 0 and is sorted, so this never underflows.
        let line = buffer.lines.partition_point(|&start| start <= offset) - 1;
        let start = buffer.lines[line] as usize;
        let col = buffer.text[start..offset as usize].chars().count() + 1;
        LineCol {
            line: line as u32 + 1,
            col: col as u32,
        }
    }

    /// The chain of expansions a token came through, innermost first.
    ///
    /// Empty when the token was written where it is used, which is the common
    /// case and the reason this is an iterator rather than a `Vec`.
    pub fn trace(&self, origin: Origin) -> impl Iterator<Item = &Expansion> {
        std::iter::successors(origin.from.map(|id| self.expansion(id)), |expansion| {
            expansion.parent.map(|id| self.expansion(id))
        })
    }

    /// Where a message about this token should point.
    ///
    /// For a token that came out of a macro that is the outermost call site --
    /// the `` `FOO `` the reader actually wrote — because the inside of a macro
    /// body is somewhere they cannot see and usually did not write.
    pub fn reported_at(&self, origin: Origin) -> Span {
        self.trace(origin)
            .last()
            .map_or(origin.spelled, |outermost| outermost.call)
    }
}
