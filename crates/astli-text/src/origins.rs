//! Source text storage and token origin tracking across macro expansions and includes.

use std::path::{Path, PathBuf};

use crate::lines::Lines;
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
    /// Span of the whole `` `define `` directive the call read, from
    /// `` `define `` to the end of its body, as written. A predefined macro's
    /// or a `+define+`'s is in the buffer the session made for them.
    ///
    /// `None` for a built-in, `` `__FILE__ `` or `` `__LINE__ ``, which no
    /// directive defines; its `name` and `call` are both the built-in's own
    /// token. A call to an undefined macro, one that reaches itself, or one
    /// whose arguments do not bind records no expansion at all: its tokens
    /// stand as written.
    pub def: Option<Span>,
}

/// Provenance of a registered text buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    /// Source file loaded from disk or direct input.
    File {
        path: PathBuf,
        included_from: Option<Span>,
        /// The name a quoted `include` searched for, as it expanded.
        included_as: Option<String>,
    },
    /// In-memory text synthesized by macro token concatenation (`` `` ``) or stringification (`` `" ``).
    Synthesised,
}

/// In-memory text buffer with precomputed line start offsets.
struct Buffer {
    source: Source,
    text: String,
    /// Where each line begins.
    lines: Lines,
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
                included_as: None,
            },
            text,
        )
    }

    /// Registers `text`, read from `path`, as the file the `` `include `` at
    /// `site` names, which searched for the name `named` when it is a quoted
    /// one.
    ///
    /// Returns `None` if `path` is already open in the chain of includes that
    /// reaches `site`, since following it would not terminate.
    pub fn add_included(
        &mut self,
        path: PathBuf,
        text: String,
        site: Span,
        named: Option<&str>,
    ) -> Option<SourceId> {
        if self.reenters(&path, site.src_id) {
            return None;
        }
        let source = Source::File {
            path,
            included_from: Some(site),
            included_as: named.map(str::to_string),
        };
        Some(self.add(source, text))
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
        let lines = Lines::new(&text);
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
        let buffer = self.views[span.src_id.index()].buffer;
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
        Span {
            src_id: file,
            ..span
        }
    }

    /// Returns `span` with its expansion dropped: where its bytes are written.
    pub fn spelled(&self, span: Span) -> Span {
        Span {
            src_id: self.buffer(span.src_id).written,
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

    /// Returns where each line of the specified buffer begins.
    pub fn lines(&self, file: SourceId) -> &Lines {
        &self.buffer(file).lines
    }

    /// Returns the source slice corresponding to `span`.
    pub fn slice(&self, span: Span) -> &str {
        &self.text(span.src_id)[span.start as usize..span.end as usize]
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

    /// Returns the name the quoted `include` that loaded this file searched
    /// for, as it expanded: what a compiler given the same include
    /// directories looks for.
    pub fn included_as(&self, file: SourceId) -> Option<&str> {
        match &self.buffer(file).source {
            Source::File { included_as, .. } => included_as.as_deref(),
            Source::Synthesised => None,
        }
    }

    /// Returns an iterator walking up the include hierarchy from this file.
    pub fn include_trace(&self, file: SourceId) -> impl Iterator<Item = Span> {
        std::iter::successors(self.included_from(file), |span| {
            self.included_from(span.src_id)
        })
    }

    /// Returns the nesting depth of `include` directives for this file.
    pub fn include_depth(&self, file: SourceId) -> usize {
        self.include_trace(file).count()
    }

    /// Returns `true` if loading `path` from `file` would create a circular include dependency.
    fn reenters(&self, path: &Path, file: SourceId) -> bool {
        std::iter::once(file)
            .chain(self.include_trace(file).map(|site| site.src_id))
            .any(|open| self.path(open) == Some(path))
    }

    /// Retrieves macro expansion metadata by identifier.
    pub fn expansion(&self, id: ExpansionId) -> &Expansion {
        &self.expansions[id.0 as usize].0
    }

    /// Converts a zero-based byte offset into a 1-based line and character column position.
    pub fn line_col(&self, file: SourceId, offset: u32) -> LineCol {
        let buffer = self.buffer(file);
        let line = buffer.lines.line(offset);
        let start = buffer.lines.start(line).unwrap_or_default() as usize;
        let col = buffer.text[start..offset as usize].chars().count() + 1;
        LineCol {
            line: line as u32 + 1,
            col: col as u32,
        }
    }

    /// Returns an iterator walking outward through the expansions that placed
    /// `file`'s tokens.
    pub fn trace(&self, file: SourceId) -> impl Iterator<Item = &Expansion> {
        self.placed_through(file).map(|id| self.expansion(id))
    }

    /// The ids of [`trace`](Self::trace)'s expansions, innermost first, so
    /// that a token can be asked whether one call placed it.
    pub fn placed_through(&self, file: SourceId) -> impl Iterator<Item = ExpansionId> {
        std::iter::successors(self.placed_by(file), |&inner| {
            self.placed_by(self.expansion(inner).call.src_id)
        })
    }

    /// Determines the primary source span to report in diagnostics for this token.
    ///
    /// For tokens produced by macro expansion, this returns the outermost macro call site;
    /// otherwise it returns the span itself.
    pub fn reported_at(&self, span: Span) -> Span {
        self.trace(span.src_id)
            .last()
            .map_or(span, |outermost| outermost.call)
    }
}
