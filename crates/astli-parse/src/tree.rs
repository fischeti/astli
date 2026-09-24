//! Standalone syntax tree container for single-file parsing workflows.
//!
//! While [`parse`](super::parse) operates directly on an existing compilation [`Session`]
//! and [`SourceId`], [`SyntaxTree`] provides a self-contained wrapper that bundles the
//! parsed syntax tree together with its backing session, source text, and file metadata.

use std::io;
use std::path::{Path, PathBuf};

use astli_preproc::{MacroTable, Session};
use astli_syntax::SyntaxNode;
use astli_text::{Diagnostic, LineCol, Origins, SourceId};

/// A parsed syntax tree bundled with its compilation session and source origins.
pub struct SyntaxTree {
    session: Session<'static>,
    file: SourceId,
    root: SyntaxNode,
    diagnostics: Vec<Diagnostic>,
}

impl SyntaxTree {
    /// Reads a file from `path` and parses its syntax tree.
    pub fn read(path: impl AsRef<Path>) -> io::Result<SyntaxTree> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        Ok(SyntaxTree::parse(path, text))
    }

    /// Parses the provided source text, associating it with `path`.
    pub fn parse(path: impl Into<PathBuf>, text: String) -> SyntaxTree {
        let mut session = Session::new();
        let file = session.add(path, text);
        let parsed = super::parse(&session, file, MacroTable::new());
        SyntaxTree {
            session,
            file,
            root: parsed.root,
            diagnostics: parsed.diagnostics,
        }
    }

    /// Returns a reference to the root syntax node.
    pub fn root(&self) -> &SyntaxNode {
        &self.root
    }

    /// Returns the diagnostics produced during parsing.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Returns the source text of the parsed file.
    pub fn source(&self) -> &str {
        self.session.source(self.file)
    }

    /// Computes the 1-based line and column coordinates for a byte offset.
    pub fn line_col(&self, offset: u32) -> LineCol {
        self.session.origins().line_col(self.file, offset)
    }

    /// Returns the store the diagnostics' spans index, which is what
    /// rendering them takes.
    pub fn origins(&self) -> &Origins {
        self.session.origins()
    }
}
