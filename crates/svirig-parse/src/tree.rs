//! A file, its tree, and what explains it -- for a caller that has a path.
//!
//! [`parse`](super::parse) takes a [`Session`] and a [`FileId`], which is what
//! a driver holding many files has. A tool that reads one file at a time has
//! neither, and making it build a session first is asking it to learn the
//! shape of a compilation in order to parse a file.
//!
//! So this owns one. What it buys beyond the two lines it saves is that the
//! tree stops arriving detached: a bare [`SyntaxNode`] cannot answer what its
//! source text was or where byte 4000 is, and a caller that wants either has
//! to keep the session and the id alive itself.
//!
//! # One file, and the filesystem
//!
//! The session inside is always [`Session::new`]'s, reading from disk. A
//! caller that wants a [`Reader`](svirig_text::Reader) of its own is holding
//! unsaved buffers or several files, and wants the session directly.
//!
//! Which is also why this reads the file rather than going through the
//! session's reader: `Reader` answers `Option` on purpose, and a tool that was
//! handed a path can say *why* it could not be opened.

use std::io;
use std::path::{Path, PathBuf};

use svirig_preproc::Session;
use svirig_syntax::SyntaxNode;
use svirig_text::{Diagnostic, FileId, LineCol, Origins};

/// One file's tree, with the session that explains it.
pub struct SyntaxTree {
    session: Session<'static>,
    file: FileId,
    root: SyntaxNode,
    diagnostics: Vec<Diagnostic>,
}

impl SyntaxTree {
    /// Reads `path` and parses it.
    pub fn read(path: impl AsRef<Path>) -> io::Result<SyntaxTree> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        Ok(SyntaxTree::parse(path, text))
    }

    /// Parses text the caller already holds. `path` is what a message about
    /// this file will name, and need not exist.
    pub fn parse(path: impl Into<PathBuf>, text: String) -> SyntaxTree {
        let mut session = Session::new();
        let file = session.add(path, text);
        let parsed = super::parse(&session, file);
        SyntaxTree {
            session,
            file,
            root: parsed.root,
            diagnostics: parsed.diagnostics,
        }
    }

    pub fn root(&self) -> &SyntaxNode {
        &self.root
    }

    /// What the rules found wrong. Usually empty -- see
    /// [`diagnostics`](mod@crate::diagnostics). Rendering one wants
    /// [`origins`](Self::origins) as well, which is `svirig-diag`'s business.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The file's text. The tree's is the same, byte for byte.
    pub fn source(&self) -> &str {
        self.session.source(self.file)
    }

    /// Where `offset` is, for a message that has to point somewhere.
    pub fn line_col(&self, offset: u32) -> LineCol {
        self.session.origins().line_col(self.file, offset)
    }

    pub fn file(&self) -> FileId {
        self.file
    }

    pub fn origins(&self) -> &Origins {
        self.session.origins()
    }

    /// The session, for what only it can answer -- the tokens as lexed, the
    /// directives found, expansion.
    pub fn session(&self) -> &Session<'static> {
        &self.session
    }

    /// Gives up the tree and keeps the session, for a caller that wants to go
    /// on reading the file this was built from.
    pub fn into_session(self) -> (Session<'static>, FileId) {
        (self.session, self.file)
    }
}
