//! The text of each file the client has open, as it last sent it, which
//! every read of a source file prefers to the disk.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use astli_text::{Disk, Lines, Reader, clean};
use lsp_types::{TextDocumentContentChangeEvent, Uri};

use crate::position::Encoding;

/// An open file.
#[derive(Debug)]
pub struct Buffer {
    /// Shared, so that a load takes the buffers as they are without
    /// copying any text.
    pub text: Arc<str>,
    pub lines: Lines,
}

/// The open files, by path, as [`key`] makes it.
#[derive(Default)]
pub struct Buffers(HashMap<PathBuf, Buffer>);

impl Buffers {
    /// Holds `text` as the file at `path`, replacing any buffer it had.
    pub fn open(&mut self, path: &Path, text: String) {
        let lines = Lines::new(&text);
        let buffer = Buffer {
            text: text.into(),
            lines,
        };
        self.0.insert(key(path), buffer);
    }

    /// Applies `changes` to the buffer at `path` in order, each range read
    /// in the text the one before it left. Returns `false` if the file is
    /// not open, and changes nothing.
    pub fn change(
        &mut self,
        path: &Path,
        changes: Vec<TextDocumentContentChangeEvent>,
        encoding: Encoding,
    ) -> bool {
        let Some(buffer) = self.0.get_mut(&key(path)) else {
            return false;
        };
        let mut text = buffer.text.to_string();
        for change in changes {
            match change {
                TextDocumentContentChangeEvent::TextDocumentContentChangePartial(change) => {
                    let bytes = encoding.bytes(&text, &buffer.lines, change.range);
                    let bytes = bytes.start as usize..bytes.end as usize;
                    text.replace_range(bytes, &change.text);
                }
                TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(change) => {
                    text = change.text;
                }
            }
            buffer.lines = Lines::new(&text);
        }
        buffer.text = text.into();
        true
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
