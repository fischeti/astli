//! Source text storage and token origin tracking across macro expansions and includes.

use std::path::{Path, PathBuf};

use crate::files::Reader;
use crate::span::{FileId, LineCol, Span};

/// Unique identifier for a macro expansion instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExpansionId(u32);

/// Metadata describing a macro expansion event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expansion {
    /// Span of the macro identifier at the call site (e.g. `` `FOO ``).
    pub name: Span,
    /// Span covering the complete macro invocation (e.g. `` `FOO(a, b) ``).
    pub call: Span,
    /// Span of the macro definition body, or `None` for compiler built-in macros
    /// (such as `` `__FILE__ `` or `` `__LINE__ ``).
    pub def: Option<Span>,
    /// Parent expansion identifier if this macro invocation was produced by an enclosing expansion.
    pub parent: Option<ExpansionId>,
}

/// Provenance of a token, identifying where its text is spelled and the expansion that placed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenOrigin {
    /// Physical location of the token's bytes (in a source file or synthesized buffer).
    pub spelled: Span,
    /// Innermost macro expansion that introduced this token, or `None` if written directly in source.
    pub from: Option<ExpansionId>,
}

impl TokenOrigin {
    /// Constructs a token origin for a token appearing directly in source without macro expansion.
    pub fn written(spelled: Span) -> TokenOrigin {
        TokenOrigin {
            spelled,
            from: None,
        }
    }
}

/// Result of resolving and loading an `include` directive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Included {
    /// File was successfully read and registered in the store.
    Opened(FileId),
    /// File could not be found at any candidate search path.
    NotFound,
    /// Include cycle detected (file is already open higher in the include stack).
    Cycle,
}

/// Provenance of a registered text buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    /// Source file loaded from disk or direct input.
    File {
        path: PathBuf,
        included_from: Option<Span>,
    },
    /// In-memory text synthesized by macro token concatenation (`` `` ``) or stringification (`` `" ``).
    Synthesised { by: ExpansionId },
}

/// In-memory text buffer with precomputed line start offsets.
struct Buffer {
    source: Source,
    text: String,
    /// Precomputed byte offsets where lines begin (always starting with 0).
    lines: Vec<u32>,
}

/// Central registry for source text buffers and token origin tracking.
#[derive(Default)]
pub struct Origins {
    buffers: Vec<Buffer>,
    expansions: Vec<Expansion>,
}

impl Origins {
    /// Creates an empty buffer registry.
    pub fn new() -> Origins {
        Origins::default()
    }

    /// Adds a top-level source file directly to the registry.
    pub fn add_file(&mut self, path: impl Into<PathBuf>, text: String) -> FileId {
        self.add(
            Source::File {
                path: path.into(),
                included_from: None,
            },
            text,
        )
    }

    /// Adds a file loaded through an `include` directive at the specified span.
    pub fn add_included(&mut self, path: impl Into<PathBuf>, text: String, from: Span) -> FileId {
        self.add(
            Source::File {
                path: path.into(),
                included_from: Some(from),
            },
            text,
        )
    }

    /// Attempts to read the first existing file from `candidates` and registers it
    /// as the target of the `include` directive at `site`.
    ///
    /// Returns [`Included::Opened`] on success, [`Included::NotFound`] if no candidate
    /// exists, or [`Included::Cycle`] if the target file is already open in the include stack.
    pub fn load_included(
        &mut self,
        reader: &dyn Reader,
        candidates: &[PathBuf],
        site: Span,
    ) -> Included {
        let Some((path, text)) = candidates
            .iter()
            .find_map(|candidate| Some((candidate, reader.read(candidate)?)))
        else {
            return Included::NotFound;
        };
        if self.reenters(path, site.file) {
            return Included::Cycle;
        }
        Included::Opened(self.add_included(path, text, site))
    }

    /// Registers text synthesized during macro expansion by `by`.
    pub fn add_synthesised(&mut self, text: String, by: ExpansionId) -> FileId {
        self.add(Source::Synthesised { by }, text)
    }

    /// Records a macro expansion event and returns its unique identifier.
    pub fn expand(&mut self, expansion: Expansion) -> ExpansionId {
        self.expansions.push(expansion);
        ExpansionId(self.expansions.len() as u32 - 1)
    }

    fn add(&mut self, source: Source, text: String) -> FileId {
        let mut lines = vec![0];
        lines.extend(
            text.bytes()
                .enumerate()
                .filter(|&(_, byte)| byte == b'\n')
                .map(|(at, _)| at as u32 + 1),
        );
        self.buffers.push(Buffer {
            source,
            text,
            lines,
        });
        FileId(self.buffers.len() as u32 - 1)
    }

    /// Returns an iterator over all registered file IDs in insertion order.
    pub fn files(&self) -> impl Iterator<Item = FileId> {
        (0..self.buffers.len() as u32).map(FileId)
    }

    /// Returns the text content of the specified buffer.
    pub fn text(&self, file: FileId) -> &str {
        &self.buffers[file.index()].text
    }

    /// Returns the source slice corresponding to `span`.
    pub fn slice(&self, span: Span) -> &str {
        &self.text(span.file)[span.start as usize..span.end as usize]
    }

    /// Returns the filesystem path of `file`, or `None` if it is a synthesized buffer.
    pub fn path(&self, file: FileId) -> Option<&Path> {
        match &self.buffers[file.index()].source {
            Source::File { path, .. } => Some(path),
            Source::Synthesised { .. } => None,
        }
    }

    /// Returns the span of the `include` directive that loaded this file, if any.
    pub fn included_from(&self, file: FileId) -> Option<Span> {
        match &self.buffers[file.index()].source {
            Source::File { included_from, .. } => *included_from,
            Source::Synthesised { .. } => None,
        }
    }

    /// Returns an iterator walking up the include hierarchy from this file.
    pub fn include_trace(&self, file: FileId) -> impl Iterator<Item = Span> {
        std::iter::successors(self.included_from(file), |span| {
            self.included_from(span.file)
        })
    }

    /// Returns the nesting depth of `include` directives for this file.
    pub fn include_depth(&self, file: FileId) -> usize {
        self.include_trace(file).count()
    }

    /// Returns `true` if loading `path` from `file` would create a circular include dependency.
    fn reenters(&self, path: &Path, file: FileId) -> bool {
        std::iter::once(file)
            .chain(self.include_trace(file).map(|site| site.file))
            .any(|open| self.path(open) == Some(path))
    }

    /// Retrieves macro expansion metadata by identifier.
    pub fn expansion(&self, id: ExpansionId) -> &Expansion {
        &self.expansions[id.0 as usize]
    }

    /// Converts a zero-based byte offset into a 1-based line and character column position.
    pub fn line_col(&self, file: FileId, offset: u32) -> LineCol {
        let buffer = &self.buffers[file.index()];
        let line = buffer.lines.partition_point(|&start| start <= offset) - 1;
        let start = buffer.lines[line] as usize;
        let col = buffer.text[start..offset as usize].chars().count() + 1;
        LineCol {
            line: line as u32 + 1,
            col: col as u32,
        }
    }

    /// Returns an iterator walking outward through the macro expansion chain for a token.
    pub fn trace(&self, origin: TokenOrigin) -> impl Iterator<Item = &Expansion> {
        std::iter::successors(origin.from.map(|id| self.expansion(id)), |expansion| {
            expansion.parent.map(|id| self.expansion(id))
        })
    }

    /// Determines the primary source span to report in diagnostics for this token.
    ///
    /// For tokens produced by macro expansion, this returns the outermost macro call site;
    /// otherwise it returns the physical location where the token was spelled.
    pub fn reported_at(&self, origin: TokenOrigin) -> Span {
        self.trace(origin)
            .last()
            .map_or(origin.spelled, |outermost| outermost.call)
    }
}
