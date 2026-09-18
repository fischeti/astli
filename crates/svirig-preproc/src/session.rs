//! One compilation: what has been read, what it lexed to, and where an
//! `` `include `` looks.
//!
//! The two modes -- [`scan`](super::scan) over a file as written, and
//! [`Session::expand`] over what it means -- need the same three things and
//! used to be handed them one call at a time: the store the text lives in, the
//! [`Reader`] behind `` `include ``, and each file's tokens. A caller that held
//! them apart lexed a file the store already had, and kept the text alive
//! beside the store to do it.
//!
//! So they are held here instead, and both modes take a [`FileId`]. What a
//! caller gets back is an [`Input`], which is the view the rest of the crate
//! addresses tokens through; nothing else builds one.
//!
//! # What is not here yet
//!
//! A table seeded from `+define+` on a command line, and the include path
//! beside it. Both belong to whatever reads a filelist; `docs/api.md` has the
//! shape they arrive in, and `docs/limitations.md` what their absence costs.

use std::path::PathBuf;
use std::rc::Rc;

use rustc_hash::FxHashMap;
use svirig_text::{Disk, FileId, Origins, Reader};

use super::Scan;
use super::expand::{self, Expanded};
use super::include::Includes;
use super::macros::MacroTable;
use super::tokens::{Input, TokenSpan};
use svirig_syntax::Token;

/// Each file's tokens, shared rather than borrowed: a slice taken out of this
/// map could not be held across a write to the store, and every expansion
/// writes to it.
pub(super) type Lexed = FxHashMap<FileId, Rc<[Token]>>;

/// The store, the reader and the tokens, for one compilation.
pub struct Session<'a> {
    origins: Origins,
    reader: &'a dyn Reader,
    includes: Includes,
    lexed: Lexed,
}

impl Session<'static> {
    /// Reading from the filesystem, with nothing on the include path.
    pub fn new() -> Session<'static> {
        Session::reading(&Disk)
    }
}

impl Default for Session<'static> {
    fn default() -> Session<'static> {
        Session::new()
    }
}

impl<'a> Session<'a> {
    /// Reading through `reader`, with nothing on the include path.
    pub fn reading(reader: &'a dyn Reader) -> Session<'a> {
        Session {
            origins: Origins::new(),
            reader,
            includes: Includes::new(),
            lexed: Lexed::default(),
        }
    }

    /// The same, looking for `` `include ``s through `includes`.
    pub fn searching(self, includes: Includes) -> Session<'a> {
        Session { includes, ..self }
    }

    /// Adds a file the caller already holds the text of, and lexes it.
    ///
    /// Only an `` `include `` goes through the [`Reader`]: a driver has read
    /// the file it was told to open, and an editor has it in a buffer.
    ///
    /// Lexing here rather than on first ask is what lets [`input`] and
    /// [`origins`] both borrow shared: a file is added in order to be read,
    /// and a view of one that could still be lexing would have to take the
    /// session exclusively and lock the store out for as long as it lived.
    ///
    /// [`input`]: Session::input
    /// [`origins`]: Session::origins
    pub fn add(&mut self, path: impl Into<PathBuf>, text: String) -> FileId {
        let file = self.origins.add_file(path, text);
        let tokens: Rc<[Token]> = svirig_syntax::tokenize(self.origins.text(file)).into();
        self.lexed.insert(file, tokens);
        file
    }

    /// Reads `path` through the [`Reader`] and adds what it holds.
    ///
    /// `None` where there is nothing to read, which is the reader's own
    /// answer: probing and reading are one question, so a caller that wanted
    /// to know *why* has to ask the filesystem itself.
    pub fn open(&mut self, path: impl Into<PathBuf>) -> Option<FileId> {
        let path = path.into();
        let text = self.reader.read(&path)?;
        Some(self.add(path, text))
    }

    pub fn origins(&self) -> &Origins {
        &self.origins
    }

    /// The file's text, as it was added.
    pub fn source(&self, file: FileId) -> &str {
        self.origins.text(file)
    }

    /// The file's tokens.
    pub fn tokens(&self, file: FileId) -> Rc<[Token]> {
        Rc::clone(
            self.lexed
                .get(&file)
                .expect("a file is lexed when it is added, and when it is included"),
        )
    }

    /// The file as everything that reads tokens wants it: the id, the text,
    /// and the tokens together.
    pub fn input(&self, file: FileId) -> Input<'_> {
        let tokens = self
            .lexed
            .get(&file)
            .expect("a file is lexed when it is added, and when it is included");
        Input::new(file, self.origins.text(file), tokens)
    }

    /// Every directive and macro reference in `file`, and the table they
    /// build.
    pub fn scan(&self, file: FileId) -> Scan {
        super::scan(&self.input(file))
    }

    /// Expands every macro reference in `file`, following the `` `include ``s
    /// it reaches and evaluating the conditionals it meets.
    ///
    /// The table starts empty each time, which is each file standing as its
    /// own compilation unit (3.12.1). Carrying one file's definitions into the
    /// next is the other reading of 22.3, and wants a driver to say so.
    pub fn expand(&mut self, file: FileId) -> Expanded {
        expand::file(
            &mut self.origins,
            &mut self.lexed,
            &self.includes,
            self.reader,
            file,
        )
    }

    /// Expands one stretch of a file, against the definitions already in
    /// `table`.
    ///
    /// [`expand`](Session::expand) is this over a whole file with an empty
    /// table. The other case is asking what a *piece* of source means -- one
    /// branch of a conditional, say -- where the piece is not the file and the
    /// definitions it needs were made somewhere the piece does not contain.
    pub fn expand_span(&mut self, span: TokenSpan, table: MacroTable) -> Expanded {
        expand::span(
            &mut self.origins,
            &mut self.lexed,
            &self.includes,
            self.reader,
            span,
            table,
        )
    }
}
