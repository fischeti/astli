//! File session management and preprocessor configuration.

use std::path::Path;

use svirig_preproc::{Expanded, Includes, MacroTable, Session, TokenSpan};
use svirig_text::Diagnostic;
use svirig_text::SourceId;

use crate::error::{Error, Result};
use crate::sources::Build;

/// Synthetic filename assigned to command-line macro definitions (`-D`).
pub const COMMAND_LINE: &str = "<command-line>";

/// An active preprocessor session containing an open file and initial macro definitions.
pub struct Opened {
    pub session: Session<'static>,
    pub file: SourceId,
    command_line: Option<MacroTable>,
    diagnostics: Vec<Diagnostic>,
}

/// Opens `path` within a new session configured with search paths and defines from `build`.
pub fn open(path: &Path, build: &Build) -> Result<Opened> {
    let text = std::fs::read_to_string(path).map_err(|err| Error::io(path, err))?;
    let includes = Includes {
        quoted: build.incdir.clone(),
        ..Includes::new()
    };
    let mut session = Session::new().searching(includes);
    let command_line = defines(&mut session, &build.define);
    let file = session.add(path, text);
    Ok(Opened {
        session,
        file,
        command_line,
        diagnostics: Vec::new(),
    })
}

impl Opened {
    /// Fully expands macros and file inclusions in the open file against command-line definitions.
    pub fn expand(&mut self) -> Expanded {
        let len = self.session.input(self.file).len();
        let span = TokenSpan::new(self.file, 0, len);
        let table = self.command_line.clone().unwrap_or_default();

        let mut expanded = self.session.expand_span(span, table);
        self.diagnostics.append(&mut expanded.diagnostics);
        expanded
    }

    /// Returns all diagnostics collected during preprocessing so far.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// Synthesizes a macro table from command-line `-D` definitions.
fn defines(session: &mut Session<'static>, defines: &[String]) -> Option<MacroTable> {
    if defines.is_empty() {
        return None;
    }

    let mut text = String::new();
    for define in defines {
        let (name, body) = define.split_once('=').unwrap_or((define, "1"));
        text.push_str("`define ");
        text.push_str(name);
        text.push(' ');
        text.push_str(body);
        text.push('\n');
    }

    let file = session.add(COMMAND_LINE, text);
    Some(session.scan(file).macros)
}
