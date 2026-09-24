//! Opening a file in a preprocessor session configured by the build.

use std::path::Path;

use svirig_preproc::{Build, Expanded, Session};
use svirig_text::{Diagnostic, SourceId};

use crate::error::{Error, Result};

/// A file open in a session, with the diagnostics its expansions reported.
pub struct Opened {
    pub session: Session<'static>,
    pub file: SourceId,
    diagnostics: Vec<Diagnostic>,
}

/// Opens `path` within a new session configured by `build`.
pub fn open(path: &Path, build: &Build) -> Result<Opened> {
    let text = std::fs::read_to_string(path).map_err(|err| Error::io(path, err))?;
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
