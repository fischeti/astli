//! Preprocessor compilation session managing files, token caches, and the build.

use std::path::PathBuf;
use std::rc::Rc;

use astli_text::{Disk, Origins, Reader, SourceId};
use rustc_hash::FxHashMap;

use super::Scan;
use super::build::{Build, COMMAND_LINE};
use super::expand::{self, Expanded};
use super::macros::MacroTable;
use super::tokens::{Input, TokenSpan};
use astli_syntax::Token;

/// Token cache mapping each file to its lexed tokens.
pub(super) type Lexed = FxHashMap<SourceId, Rc<[Token]>>;

/// Compilation session coordinating source files, the build, and macro expansions.
pub struct Session<'a> {
    origins: Origins,
    reader: &'a dyn Reader,
    build: Build,
    /// The build's definitions, which every expansion starts from.
    defined: MacroTable,
    lexed: Lexed,
}

impl Session<'static> {
    /// Creates a new session reading from the local filesystem, with an empty build.
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
    /// Creates a new session using the provided [`Reader`] implementation.
    pub fn reading(reader: &'a dyn Reader) -> Session<'a> {
        Session {
            origins: Origins::new(),
            reader,
            build: Build::new(),
            defined: MacroTable::new(),
            lexed: Lexed::default(),
        }
    }

    /// Configures the session with a build's include directories and
    /// definitions.
    pub fn building(mut self, build: Build) -> Session<'a> {
        if let Some(text) = build.command_line() {
            let file = self.add(COMMAND_LINE, text);
            self.defined = self.scan(file).macros;
        }
        Session { build, ..self }
    }

    /// Adds in-memory file content to the session and tokenizes it immediately.
    pub fn add(&mut self, path: impl Into<PathBuf>, text: String) -> SourceId {
        let file = self.origins.add_file(path, text);
        let tokens: Rc<[Token]> = astli_syntax::tokenize(self.origins.text(file)).into();
        self.lexed.insert(file, tokens);
        file
    }

    /// Reads `path` using the session's [`Reader`] and adds it to the session if found.
    pub fn open(&mut self, path: impl Into<PathBuf>) -> Option<SourceId> {
        let path = path.into();
        let text = self.reader.read(&path)?;
        Some(self.add(path, text))
    }

    /// Returns a reference to the underlying [`Origins`] tracker.
    pub fn origins(&self) -> &Origins {
        &self.origins
    }

    /// Returns the source text of the specified file.
    pub fn source(&self, file: SourceId) -> &str {
        self.origins.text(file)
    }

    /// Returns the token stream of the specified file.
    pub fn tokens(&self, file: SourceId) -> Rc<[Token]> {
        Rc::clone(
            self.lexed
                .get(&file)
                .expect("file must be lexed upon addition to session"),
        )
    }

    /// Returns an [`Input`] view containing the file's ID, source text, and tokens.
    pub fn input(&self, file: SourceId) -> Input<'_> {
        let tokens = self
            .lexed
            .get(&file)
            .expect("file must be lexed upon addition to session");
        Input::new(file, self.origins.text(file), tokens)
    }

    /// Performs a preprocessor scan on `file`, discovering directives and macro references.
    pub fn scan(&self, file: SourceId) -> Scan {
        super::scan(&self.input(file))
    }

    /// Fully expands `file` from the build's definitions, resolving
    /// conditionals and following `` `include `` directives.
    pub fn expand(&mut self, file: SourceId) -> Expanded {
        expand::file(
            &mut self.origins,
            &mut self.lexed,
            &self.build.includes,
            self.reader,
            file,
            self.defined.clone(),
        )
    }

    /// Expands a specific token range within a file against a given [`MacroTable`].
    pub fn expand_span(&mut self, span: TokenSpan, table: MacroTable) -> Expanded {
        expand::span(
            &mut self.origins,
            &mut self.lexed,
            &self.build.includes,
            self.reader,
            span,
            table,
        )
    }
}
