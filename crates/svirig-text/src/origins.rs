//! Source text storage and token origin tracking across macro expansions and includes.

use std::path::{Path, PathBuf};

use crate::files::Reader;
use crate::span::{LineCol, SourceId, Span};

/// Unique identifier for a macro expansion instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExpansionId(u32);

/// Metadata describing a macro expansion event.
///
/// `name` and `call` are placed like any token, so a call written inside
/// another macro's body is seen through that expansion, and the chain of
/// enclosing expansions is read off their files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expansion {
    /// Span of the macro identifier at the call site (e.g. `` `FOO ``).
    pub name: Span,
    /// Span covering the complete macro invocation (e.g. `` `FOO(a, b) ``).
    pub call: Span,
    /// Span of the macro definition body, or `None` for compiler built-in macros
    /// (such as `` `__FILE__ `` or `` `__LINE__ ``).
    pub def: Option<Span>,
}

/// Result of resolving and loading an `include` directive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Included {
    /// File was successfully read and registered in the store.
    Opened(SourceId),
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
    Synthesised,
}

/// In-memory text buffer with precomputed line start offsets.
struct Buffer {
    source: Source,
    text: String,
    /// Precomputed byte offsets where lines begin (always starting with 0).
    lines: Vec<u32>,
    /// The view of this buffer outside any expansion.
    written: SourceId,
}

/// What a [`SourceId`] names: a buffer, as placed by an expansion or by none.
///
/// A token's provenance is where its bytes are and which expansion put them
/// where they are used. Giving each pair its own id folds both into a
/// [`Span`] that keeps the buffer's offsets.
#[derive(Debug, Clone, Copy)]
struct View {
    buffer: u32,
    from: Option<ExpansionId>,
}

/// Central registry for source text buffers and token origin tracking.
#[derive(Default)]
pub struct Origins {
    buffers: Vec<Buffer>,
    views: Vec<View>,
    /// Each expansion, with the views it has placed tokens through. An
    /// expansion reads from a few buffers at most, so a scan finds one.
    expansions: Vec<(Expansion, Vec<SourceId>)>,
}

impl Origins {
    /// Creates an empty buffer registry.
    pub fn new() -> Origins {
        Origins::default()
    }

    /// Adds a top-level source file directly to the registry.
    pub fn add_file(&mut self, path: impl Into<PathBuf>, text: String) -> SourceId {
        self.add(
            Source::File {
                path: path.into(),
                included_from: None,
            },
            text,
        )
    }

    /// Adds a file loaded through an `include` directive at the specified span.
    pub fn add_included(&mut self, path: impl Into<PathBuf>, text: String, from: Span) -> SourceId {
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

    /// Registers text synthesized during macro expansion.
    ///
    /// Its tokens are placed [`through`](Self::through) the expansion that
    /// made it, like any other.
    pub fn add_synthesised(&mut self, text: String) -> SourceId {
        self.add(Source::Synthesised, text)
    }

    /// Records a macro expansion event and returns its unique identifier.
    pub fn expand(&mut self, expansion: Expansion) -> ExpansionId {
        self.expansions.push((expansion, Vec::new()));
        ExpansionId(self.expansions.len() as u32 - 1)
    }

    fn add(&mut self, source: Source, text: String) -> SourceId {
        let mut lines = vec![0];
        lines.extend(
            text.bytes()
                .enumerate()
                .filter(|&(_, byte)| byte == b'\n')
                .map(|(at, _)| at as u32 + 1),
        );
        let buffer = self.buffers.len() as u32;
        let written = self.view(buffer, None);
        self.buffers.push(Buffer {
            source,
            text,
            lines,
            written,
        });
        written
    }

    fn view(&mut self, buffer: u32, from: Option<ExpansionId>) -> SourceId {
        self.views.push(View { buffer, from });
        SourceId(self.views.len() as u32 - 1)
    }

    fn buffer(&self, file: SourceId) -> &Buffer {
        &self.buffers[self.views[file.index()].buffer as usize]
    }

    /// Returns `span`'s bytes as placed by `expansion`, or as written when it
    /// is `None`.
    pub fn through(&mut self, span: Span, expansion: Option<ExpansionId>) -> Span {
        let buffer = self.views[span.file.index()].buffer;
        let file = match expansion {
            None => self.buffers[buffer as usize].written,
            Some(id) => {
                let placed = &self.expansions[id.0 as usize].1;
                match placed
                    .iter()
                    .find(|view| self.views[view.index()].buffer == buffer)
                {
                    Some(&view) => view,
                    None => {
                        let view = self.view(buffer, expansion);
                        self.expansions[id.0 as usize].1.push(view);
                        view
                    }
                }
            }
        };
        Span { file, ..span }
    }

    /// Returns `span` with its expansion dropped: where its bytes are written.
    pub fn spelled(&self, span: Span) -> Span {
        Span {
            file: self.buffer(span.file).written,
            ..span
        }
    }

    /// Returns the innermost expansion that placed `file`'s tokens, or `None`
    /// if they are used where they are written.
    pub fn placed_by(&self, file: SourceId) -> Option<ExpansionId> {
        self.views[file.index()].from
    }

    /// Returns an iterator over the files and synthesised buffers, as
    /// written, in insertion order.
    pub fn files(&self) -> impl Iterator<Item = SourceId> {
        self.buffers.iter().map(|buffer| buffer.written)
    }

    /// Returns the text content of the specified buffer.
    pub fn text(&self, file: SourceId) -> &str {
        &self.buffer(file).text
    }

    /// Returns the source slice corresponding to `span`.
    pub fn slice(&self, span: Span) -> &str {
        &self.text(span.file)[span.start as usize..span.end as usize]
    }

    /// Returns the filesystem path of `file`, or `None` if it is a synthesized buffer.
    pub fn path(&self, file: SourceId) -> Option<&Path> {
        match &self.buffer(file).source {
            Source::File { path, .. } => Some(path),
            Source::Synthesised => None,
        }
    }

    /// Returns the span of the `include` directive that loaded this file, if any.
    pub fn included_from(&self, file: SourceId) -> Option<Span> {
        match &self.buffer(file).source {
            Source::File { included_from, .. } => *included_from,
            Source::Synthesised => None,
        }
    }

    /// Returns an iterator walking up the include hierarchy from this file.
    pub fn include_trace(&self, file: SourceId) -> impl Iterator<Item = Span> {
        std::iter::successors(self.included_from(file), |span| {
            self.included_from(span.file)
        })
    }

    /// Returns the nesting depth of `include` directives for this file.
    pub fn include_depth(&self, file: SourceId) -> usize {
        self.include_trace(file).count()
    }

    /// Returns `true` if loading `path` from `file` would create a circular include dependency.
    fn reenters(&self, path: &Path, file: SourceId) -> bool {
        std::iter::once(file)
            .chain(self.include_trace(file).map(|site| site.file))
            .any(|open| self.path(open) == Some(path))
    }

    /// Retrieves macro expansion metadata by identifier.
    pub fn expansion(&self, id: ExpansionId) -> &Expansion {
        &self.expansions[id.0 as usize].0
    }

    /// Converts a zero-based byte offset into a 1-based line and character column position.
    pub fn line_col(&self, file: SourceId, offset: u32) -> LineCol {
        let buffer = self.buffer(file);
        let line = buffer.lines.partition_point(|&start| start <= offset) - 1;
        let start = buffer.lines[line] as usize;
        let col = buffer.text[start..offset as usize].chars().count() + 1;
        LineCol {
            line: line as u32 + 1,
            col: col as u32,
        }
    }

    /// Returns an iterator walking outward through the expansions that placed
    /// `file`'s tokens.
    pub fn trace(&self, file: SourceId) -> impl Iterator<Item = &Expansion> {
        let expansion = |file| self.placed_by(file).map(|id| self.expansion(id));
        std::iter::successors(expansion(file), move |inner| expansion(inner.call.file))
    }

    /// Determines the primary source span to report in diagnostics for this token.
    ///
    /// For tokens produced by macro expansion, this returns the outermost macro call site;
    /// otherwise it returns the span itself.
    pub fn reported_at(&self, span: Span) -> Span {
        self.trace(span.file)
            .last()
            .map_or(span, |outermost| outermost.call)
    }
}
