//! Opening a file the way the command line asked for it.
//!
//! One place, because every command past `lex` needs the same three steps --
//! read the file, put the include path on the session, seed what `-D` defined
//! -- and because this is where a filelist will be read when one is. Filelist
//! syntax has nothing to do with SystemVerilog, so it belongs to the driver
//! and not to `svirig-preproc`.

use std::path::Path;

use svirig_preproc::{ExpandedToken, Includes, MacroTable, Session, TokenSpan};
use svirig_text::FileId;

use crate::cli::BuildArgs;
use crate::error::{Error, Result};

/// A session with one file in it, and whatever `-D` defined before it.
pub struct Opened {
    pub session: Session<'static>,
    pub file: FileId,
    /// The synthesised buffer `-D` was written into, and the table it built.
    /// `None` when nothing was defined, which is the common case and is worth
    /// not paying a buffer for.
    command_line: Option<(FileId, MacroTable)>,
}

/// Reads `path` and puts it in a session configured by `build`.
///
/// The file is read here rather than through the session's `Reader`, which
/// answers `Option`: a tool handed a path on a command line can say *why* it
/// could not open it, and that is the whole difference between the two.
pub fn open(path: &Path, build: &BuildArgs) -> Result<Opened> {
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
    })
}

impl Opened {
    /// The file with every macro expanded and every `` `include `` followed,
    /// against the command-line definitions.
    pub fn expand(&mut self) -> Vec<ExpandedToken> {
        // The whole file is one span, which is what `Session::expand` is over
        // an empty table. Going through the span form is what lets `-D` in.
        let len = self.session.input(self.file).len();
        let span = TokenSpan::new(self.file, 0, len);
        let table = match &self.command_line {
            Some((_, table)) => table.clone(),
            None => MacroTable::new(),
        };
        self.session.expand_span(span, table)
    }

    /// What `-D` defined, and the buffer whose text those definitions are in.
    ///
    /// Two values because an entry's spans address the buffer it was read
    /// from, and that one is not the file.
    pub fn defines(&self) -> Option<(FileId, &MacroTable)> {
        self.command_line
            .as_ref()
            .map(|(file, table)| (*file, table))
    }
}

/// `-D NAME=VALUE` is text, so it is lexed as text: the definitions become a
/// synthesised `<command-line>` buffer that the ordinary scan reads. Nothing
/// new in the preprocessor, and a macro defined on the command line then has
/// somewhere for a diagnostic to point at.
///
/// A name given without a value is `1` rather than nothing, so that `-DWIDTH`
/// is usable in the expression it usually guards and not only by `` `ifdef ``.
fn defines(session: &mut Session<'static>, defines: &[String]) -> Option<(FileId, MacroTable)> {
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

    let file = session.add("<command-line>", text);
    let table = session.scan(file).macros;
    Some((file, table))
}
