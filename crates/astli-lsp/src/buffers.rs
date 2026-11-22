//! The text of each file the client has open, as it last sent it, which
//! every read of a source file prefers to the disk.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use astli_text::{Disk, Lines, Reader, clean};
use lsp_types::{TextDocumentContentChangeEvent, Uri};

use crate::position::Encoding;

/// An open file.
#[derive(Debug)]
pub struct Buffer {
    pub text: String,
    /// The client's version, which grows with each change it sends.
    pub version: i32,
    pub lines: Lines,
}

/// The open files, by path.
#[derive(Default)]
pub struct Buffers(HashMap<PathBuf, Buffer>);

impl Buffers {
    /// Holds `text` as the file at `path`, replacing any buffer it had.
    pub fn open(&mut self, path: &Path, text: String, version: i32) {
        let lines = Lines::new(&text);
        let buffer = Buffer {
            text,
            version,
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
        version: i32,
        changes: Vec<TextDocumentContentChangeEvent>,
        encoding: Encoding,
    ) -> bool {
        let Some(buffer) = self.0.get_mut(&key(path)) else {
            return false;
        };
        for change in changes {
            match change {
                TextDocumentContentChangeEvent::TextDocumentContentChangePartial(change) => {
                    let bytes = encoding.bytes(&buffer.text, &buffer.lines, change.range);
                    let bytes = bytes.start as usize..bytes.end as usize;
                    buffer.text.replace_range(bytes, &change.text);
                }
                TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(change) => {
                    buffer.text = change.text;
                }
            }
            buffer.lines = Lines::new(&buffer.text);
        }
        buffer.version = version;
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
}

impl Reader for Buffers {
    fn exists(&self, path: &Path) -> bool {
        self.get(path).is_some() || Disk.exists(path)
    }

    fn read(&self, path: &Path) -> io::Result<String> {
        match self.get(path) {
            Some(buffer) => Ok(buffer.text.clone()),
            None => Disk.read(path),
        }
    }
}

/// A path as a buffer is held under: absolute and without `.` or `..`, so
/// that a file the loader names from an include directory finds the buffer
/// the client opened. A symbolic link is not followed, so a buffer opened
/// through one is not found through the other.
fn key(path: &Path) -> PathBuf {
    match std::path::absolute(path) {
        Ok(path) => clean(&path),
        Err(_) => clean(path),
    }
}

/// The path a `file:` URI names, or `None` for another scheme, such as a
/// client's unsaved `untitled:` buffer, which no read of the design asks for.
pub fn path(uri: &Uri) -> Option<PathBuf> {
    match uri.scheme() {
        "file" => uri.to_file_path().ok(),
        _ => None,
    }
}
