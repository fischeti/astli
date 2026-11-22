//! Opening a file in a preprocessor session configured by the build.

use std::path::Path;

use astli_parse::SyntaxTree;
use astli_preproc::{Build, Expanded, Session};
use astli_text::{Diagnostic, Reader, SourceId};

use crate::error::{Error, Result};

/// A file open in a session, with the diagnostics its expansions reported.
pub struct Opened<'r> {
    pub session: Session<'r>,
    pub file: SourceId,
    diagnostics: Vec<Diagnostic>,
}

/// The text of the source file at `path`, as `reader` holds it.
pub fn read(path: &Path, reader: &dyn Reader) -> Result<String> {
    reader.read(path).map_err(|err| Error::io(path, err))
}

/// The source file at `path`, read through `reader` and parsed.
pub fn tree(path: &Path, reader: &dyn Reader) -> Result<SyntaxTree> {
    Ok(SyntaxTree::parse(path, read(path, reader)?))
}

/// Opens `path` within a new session configured by `build`, reading it and
/// what it includes through `reader`.
pub fn open<'r>(path: &Path, build: &Build, reader: &'r dyn Reader) -> Result<Opened<'r>> {
    Ok(opened(path, read(path, reader)?, build, reader))
}

/// Opens `path`, already read as `text`, within a new session configured
/// by `build`, which reads what it includes through `reader`.
pub fn opened<'r>(path: &Path, text: String, build: &Build, reader: &'r dyn Reader) -> Opened<'r> {
    let mut session = Session::reading(reader).building(build.clone());
    let file = session.add(path, text);
    Opened {
        session,
        file,
        diagnostics: Vec::new(),
    }
}

impl Opened<'_> {
    /// Fully expands macros and file inclusions in the open file.
    pub fn expand(&mut self) -> Expanded {
        let mut expanded = self.session.expand(self.file);
        self.diagnostics.append(&mut expanded.diagnostics);
        expanded
    }

    /// Returns all diagnostics collected during preprocessing so far.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
