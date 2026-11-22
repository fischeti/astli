//! From a position in an open file to what stands there in its design's
//! snapshot, which every request that starts at a cursor reads, and from a
//! span of the snapshot back to a place the client knows.
//!
//! A file is read through each unit whose origins hold it: its own, or
//! each that includes it, once for each time it does. A request asks each
//! of them, since what a name means can differ from one inclusion to the
//! next, and answers what they agree on once.

use std::collections::HashMap;
use std::ops::Range as Bytes;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use astli_sema::Site;
use astli_text::{ExpansionId, Origins, SourceId, Span};
use lsp_server::{ErrorCode, ResponseError};
use lsp_types::{Location, Position, Range, TextDocumentPositionParams, Uri};
use serde::Deserialize;
use serde_json::Value;

use crate::buffers::{self, key};
use crate::position::Encoding;
use crate::server::{Server, error};
use crate::tasks::Cancel;
use crate::{Analysed, Snapshot, Unit};

/// A position in an open file, and what a lookup there reads, as it was
/// when the request came: the snapshot current for the file's design, the
/// buffer's text, and the URI each open file was opened with.
pub struct Cursor {
    pub snapshot: Arc<Snapshot>,
    pub path: PathBuf,
    pub encoding: Encoding,
    text: Arc<str>,
    position: Position,
    opened: HashMap<PathBuf, Uri>,
}

/// What stands at a cursor, as one unit of one design read the file.
pub struct Here<'a> {
    pub design: &'a Analysed,
    pub unit: &'a Unit,
    /// The file in the unit's origins: its own text, or one inclusion of a
    /// header.
    pub source: SourceId,
    pub offset: u32,
    /// Each name spelled there, as [`Sites::at`](astli_sema::Sites::at)
    /// finds it: several where a macro wrote it more than once, or where
    /// the cursor is in a positional connection or override.
    pub sites: Vec<&'a Site>,
    /// The macro call whose name the cursor is on, `` `NAME `` alone and
    /// not its arguments, among those the file writes outside any other
    /// call.
    pub call: Option<ExpansionId>,
    /// The header that the `` `include `` the cursor is on read.
    pub include: Option<SourceId>,
    /// The name a `` `define `` the cursor is on defines, outside the
    /// branches the expansion skipped.
    pub define: Option<Span>,
}

impl Server {
    /// The cursor the params of a request at a position name; `None` where
    /// they name no open file, or one whose design has not loaded.
    pub fn cursor(&self, params: &Value) -> Option<Cursor> {
        let at = TextDocumentPositionParams::deserialize(params).ok()?;
        self.cursor_at(&at.text_document.uri, at.position)
    }

    /// The cursor at `position` of the file `uri` names, as
    /// [`cursor`](Self::cursor) finds it.
    pub fn cursor_at(&self, uri: &Uri, position: Position) -> Option<Cursor> {
        let path = buffers::path(uri)?;
        let text = self.buffers.get(&path)?.text.clone();
        Some(Cursor {
            snapshot: self.snapshot(&path)?,
            path,
            encoding: self.encoding,
            text,
            position,
            opened: self.buffers.uris(),
        })
    }
}

impl Cursor {
    /// What stands at the cursor in each unit of each design that holds
    /// the file, in the order of the designs and their units; none where
    /// no unit does. `ContentModified` if a unit read other text of the
    /// file than the buffer holds now, since the position is in the text
    /// the buffer holds: the design is due again, and the client asks
    /// again.
    pub fn lookup(&self, cancel: &Cancel) -> Result<Vec<Here<'_>>, ResponseError> {
        let mut found = Vec::new();
        for design in &self.snapshot.designs {
            for unit in &design.units {
                cancel.check()?;
                for source in held(&unit.origins, &self.path) {
                    found.push(self.here(design, unit, source)?);
                }
            }
        }
        Ok(found)
    }

    fn here<'a>(
        &self,
        design: &'a Analysed,
        unit: &'a Unit,
        source: SourceId,
    ) -> Result<Here<'a>, ResponseError> {
        let origins = &unit.origins;
        let text = origins.text(source);
        if text != &*self.text {
            let said = "the file changed since its design was loaded";
            return Err(error(ErrorCode::ContentModified, said));
        }
        // The text is the buffer's, so its line table is the buffer's too.
        let offset = (self.encoding).offset(text, origins.lines(source), self.position);
        let holds = |span: Span| span.src_id == source && span.start <= offset;
        let sites = unit.sites(design.elaboration.design());
        let call = (unit.calls.iter().copied()).find(|&id| {
            let name = origins.expansion(id).name;
            holds(name) && offset <= name.end
        });
        let include = (origins.files()).find(|&file| {
            let directive = origins.included_from(file);
            directive.is_some_and(|it| holds(it) && offset < it.end)
        });
        let define = defined_at(text, offset)
            .map(|name| Span::new(source, name.start, name.end))
            .filter(|name| !(unit.skipped.iter()).any(|it| it.contains(*name)));
        Ok(Here {
            design,
            unit,
            source,
            offset,
            sites: sites.at(source, offset).collect(),
            call,
            include,
            define,
        })
    }

    /// The place of `span`, whose file `origins` holds, as the client is
    /// told it: under the URI the client opened the file with, or its
    /// path's, and as a range of the text the origins hold, in the
    /// negotiated encoding. `None` for text no file holds, such as a
    /// `+define+`'s.
    pub fn location(&self, origins: &Origins, span: Span) -> Option<Location> {
        let span = origins.spelled(span);
        let uri = self.uri(origins.path(span.src_id)?)?;
        let range = self.range(origins, span);
        Some(Location { uri, range })
    }

    /// The range of `span` in the text `origins` holds of its file.
    pub fn range(&self, origins: &Origins, span: Span) -> Range {
        let span = origins.spelled(span);
        let (text, lines) = (origins.text(span.src_id), origins.lines(span.src_id));
        (self.encoding).range(text, lines, span.start..span.end)
    }

    /// The URI a file is told under: the one the client opened it with, or
    /// its path's.
    pub fn uri(&self, path: &Path) -> Option<Uri> {
        let path = key(path);
        match self.opened.get(&path) {
            Some(uri) => Some(uri.clone()),
            None => Uri::from_file_path(&path).ok(),
        }
    }
}

/// Each file of `origins` at `path`: a unit's own, or each inclusion of a
/// header.
pub fn held(origins: &Origins, path: &Path) -> Vec<SourceId> {
    // A unit reads many files and few are asked about, so a name, which
    // compares cheaply, is asked first.
    let mut held: Vec<SourceId> = (origins.files())
        .filter(|&file| {
            let at = origins.path(file);
            at.is_some_and(|it| it.file_name() == path.file_name() && key(it) == path)
        })
        .collect();
    // A file is listed once for each buffer a macro placed in it.
    held.sort();
    held.dedup();
    held
}

/// Whether `origins` holds a file at `path`.
pub fn holds_path(origins: &Origins, path: &Path) -> bool {
    !held(origins, path).is_empty()
}

/// The bytes of the name the cursor at `offset` of `text` is on, where it
/// is the name of a `` `define ``, which the HIR does not hold, so the
/// text is read for it.
fn defined_at(text: &str, offset: u32) -> Option<Bytes<u32>> {
    let name = |it: &&u8| it.is_ascii_alphanumeric() || **it == b'_' || **it == b'$';
    let (before, after) = text.as_bytes().split_at((offset as usize).min(text.len()));
    let start = before.len() - before.iter().rev().take_while(name).count();
    let end = before.len() + after.iter().take_while(name).count();
    let first =
        (text.as_bytes().get(start)).is_some_and(|it| it.is_ascii_alphabetic() || *it == b'_');
    let defines = text[..start]
        .trim_end_matches([' ', '\t'])
        .ends_with("`define");
    (first && defines).then_some(start as u32..end as u32)
}

/// The name a `` `define `` directive at `directive` defines, or the whole
/// directive where it does not start as one plainly does.
pub fn defined(origins: &Origins, directive: Span) -> Span {
    let text = origins.slice(directive);
    let Some(rest) = text.strip_prefix("`define") else {
        return directive;
    };
    let name = rest.trim_start_matches([' ', '\t']);
    let length = (name.find(|it: char| !(it.is_ascii_alphanumeric() || it == '_' || it == '$')))
        .unwrap_or(name.len());
    if length == 0 {
        return directive;
    }
    let start = directive.start + (text.len() - name.len()) as u32;
    Span {
        start,
        end: start + length as u32,
        ..directive
    }
}
