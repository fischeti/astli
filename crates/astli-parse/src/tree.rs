//! One file's tree, with the text and file store it was parsed from.

use std::io;
use std::path::{Path, PathBuf};

use astli_preproc::{MacroTable, Session};
use astli_syntax::SyntaxNode;
use astli_text::{Diagnostic, LineCol, Origins, SourceId};

/// One file, parsed in raw mode, with the text and file store the tree and
/// its diagnostics point into.
///
/// It owns its own session, so it is the tool for one file at a time. To
/// seed macro arities from a build, or parse several files against one file
/// store, use [`parse`](crate::parse) instead.
pub struct SyntaxTree {
    session: Session<'static>,
    file: SourceId,
    root: SyntaxNode,
    diagnostics: Vec<Diagnostic>,
}

impl SyntaxTree {
    /// Reads `path` and parses it.
    ///
    /// Fails only if the file cannot be read; a file that does not parse
    /// still gives a tree, with [`diagnostics`](SyntaxTree::diagnostics).
    pub fn read(path: impl AsRef<Path>) -> io::Result<SyntaxTree> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        Ok(SyntaxTree::parse(path, text))
    }

    /// Parses `text`, naming it `path` in diagnostics. Nothing is read.
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

    /// The `SOURCE_FILE` node, whose text is [`source`](SyntaxTree::source).
    pub fn root(&self) -> &SyntaxNode {
        &self.root
    }

    /// What the grammar found malformed. Empty does not mean every construct
    /// was understood: one the grammar does not cover yet becomes a
    /// `VERBATIM` node without a diagnostic.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The text the tree was parsed from.
    pub fn source(&self) -> &str {
        self.session.source(self.file)
    }

    /// The 1-based line and column of byte `offset`, such as a diagnostic's
    /// `at.start` or `u32::from(node.text_range().start())`.
    pub fn line_col(&self, offset: u32) -> LineCol {
        self.session.origins().line_col(self.file, offset)
    }

    /// Returns the store the diagnostics' spans index, which is what
    /// rendering them takes.
    pub fn origins(&self) -> &Origins {
        self.session.origins()
    }
}
