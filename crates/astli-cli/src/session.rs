//! Opening a file in a preprocessor session configured by the build.

use std::path::Path;

use astli_parse::SyntaxTree;
use astli_preproc::{Build, Expanded, Session};
use astli_text::{Diagnostic, SourceId};

use crate::error::{Error, Result};

/// A file open in a session, with the diagnostics its expansions reported.
pub struct Opened {
    pub session: Session<'static>,
    pub file: SourceId,
    diagnostics: Vec<Diagnostic>,
}

/// The text of the source file at `path`, a Git LFS pointer refused.
pub fn read(path: &Path) -> Result<String> {
    astli_package::read_source(path).map_err(|err| Error::io(path, err))
}

/// The source file at `path`, read and parsed.
pub fn tree(path: &Path) -> Result<SyntaxTree> {
    Ok(SyntaxTree::parse(path, read(path)?))
}

/// Opens `path` within a new session configured by `build`.
pub fn open(path: &Path, build: &Build) -> Result<Opened> {
    let text = read(path)?;
    let mut session = Session::new().building(build.clone());
    let file = session.add(path, text);
    Ok(Opened {
        session,
        file,
        diagnostics: Vec::new(),
    })
}

impl Opened {
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
