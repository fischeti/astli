//! The text of each file the client has open, as it last sent it, which
//! every read of a source file prefers to the disk.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use astli_text::{Disk, Lines, Reader, clean};
use lsp_types::{Position, TextDocumentContentChangeEvent, Uri};

use crate::position::Encoding;

/// An open file.
#[derive(Debug)]
pub struct Buffer {
    /// Shared, so that a load takes the buffers as they are without
    /// copying any text.
    pub text: Arc<str>,
    pub lines: Lines,
    /// The version the client gave the text, which what is published for
    /// the file names.
    pub version: i32,
    /// The URI the client opened the file with, which what is published for
    /// it is sent under: a client may escape a character, or write a drive
    /// letter, otherwise than the URI of its path does, and it matches a
    /// URI to its documents as written.
    pub uri: Uri,
}

/// The open files, by path, as [`key`] makes it.
#[derive(Default)]
pub struct Buffers(HashMap<PathBuf, Buffer>);

impl Buffers {
    /// Holds `text` at `version` as the file at `path`, which the client
    /// opened as `uri`, replacing any buffer it had.
    pub fn open(&mut self, path: &Path, uri: Uri, text: String, version: i32) {
        let lines = Lines::new(&text);
        let buffer = Buffer {
            text: text.into(),
            lines,
            version,
            uri,
        };
        self.0.insert(key(path), buffer);
    }

    /// Applies `changes` to the buffer at `path` in order, each range read
    /// in the text the one before it left, and gives it `version`. Returns
    /// where the first character any of them changed is, before which the
    /// text is as it was: the start for a change of the whole text, and
    /// past every line for no change. `None` if the file is not open, and
    /// nothing changes.
    pub fn change(
        &mut self,
        path: &Path,
        changes: Vec<TextDocumentContentChangeEvent>,
        version: i32,
        encoding: Encoding,
    ) -> Option<Position> {
        let buffer = self.0.get_mut(&key(path))?;
        let mut text = buffer.text.to_string();
        // A change leaves the text before its start as the change before it
        // left it, so the earliest start is where the text first differs.
        let mut earliest = Position::new(u32::MAX, u32::MAX);
        for change in changes {
            match change {
                TextDocumentContentChangeEvent::TextDocumentContentChangePartial(change) => {
                    earliest = earliest.min(change.range.start);
                    let bytes = encoding.bytes(&text, &buffer.lines, change.range);
                    let bytes = bytes.start as usize..bytes.end as usize;
                    text.replace_range(bytes, &change.text);
                }
                TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(change) => {
                    earliest = Position::default();
                    text = change.text;
                }
            }
            buffer.lines = Lines::new(&text);
        }
        buffer.text = text.into();
        buffer.version = version;
        Some(earliest)
    }

    /// Drops the buffer at `path`, so that the file is read from the disk
    /// again. Returns `false` if it was not open.
    pub fn close(&mut self, path: &Path) -> bool {
        self.0.remove(&key(path)).is_some()
    }

    /// The buffer at `path`, if the file is open.
    pub fn get(&self, path: &Path) -> Option<&Buffer> {
        self.0.get(&key(path))
    }

    /// The text of every buffer as it is now, for a load that reads them
    /// while the loop goes on taking edits.
    pub fn texts(&self) -> Texts {
        let texts = (self.0.iter())
            .map(|(path, buffer)| (path.clone(), buffer.text.clone()))
            .collect();
        Texts(texts)
    }
}

/// The buffers' texts at one moment, read before the disk.
#[derive(Clone, Default)]
pub struct Texts(HashMap<PathBuf, Arc<str>>);

impl Texts {
    /// The text of the buffer at `path`, if the file was open.
    pub fn get(&self, path: &Path) -> Option<&Arc<str>> {
        self.0.get(&key(path))
    }
}

impl Reader for Texts {
    fn exists(&self, path: &Path) -> bool {
        self.get(path).is_some() || Disk.exists(path)
    }

    fn read(&self, path: &Path) -> io::Result<String> {
        match self.get(path) {
            Some(text) => Ok(text.to_string()),
            None => Disk.read(path),
        }
    }
}

/// A path as a buffer is held under: absolute and without `.` or `..`, so
/// that a file the loader names from an include directory finds the buffer
/// the client opened. A symbolic link is not followed, so a buffer opened
/// through one is not found through the other.
pub fn key(path: &Path) -> PathBuf {
    match std::path::absolute(path) {
        Ok(path) => clean(&path),
        Err(_) => clean(path),
    }
}

/// The path a `file:` URI names, or `None` for another scheme, such as a
/// client's unsaved `untitled:` buffer, which no read of the design asks for.
pub fn path(uri: &Uri) -> Option<PathBuf> {
    match uri.scheme() {
        "file" => uri.to_file_path().ok().map(|it| key(&it)),
        _ => None,
    }
}
