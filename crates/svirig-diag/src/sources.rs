//! The store, as the renderer wants to read it.
//!
//! `ariadne` addresses source through a [`Cache`], which hands back a
//! [`Source`] -- the text plus a line table of its own -- for an id. [`Origins`]
//! is already that: a buffer per [`FileId`], with a path and a line table. So
//! this is the adapter between them and holds no text of its own.
//!
//! # Borrowed, and built late
//!
//! [`Source`] is generic over its storage and `&str` satisfies it, so the text
//! is borrowed straight out of the store and never copied.
//!
//! Its line table is another matter: `ariadne`'s is four `usize`s a line
//! against the four *bytes* a line costs in [`Origins`], because it tracks
//! character offsets alongside byte ones. On the largest file in the corpus
//! that is a megabyte nobody asked for. So a [`Source`] is built on the first
//! [`Cache::fetch`] of a file rather than up front, and a file that carries no
//! diagnostic never builds one -- which is what a `Cache` is for, and what
//! `ariadne`'s own `From<&str> for Source` warns about.

use std::convert::Infallible;
use std::fmt;

use ariadne::{Cache, Source};
use rustc_hash::FxHashMap;
use svirig_text::{FileId, Origins};

/// Reads [`Origins`] on `ariadne`'s behalf, keeping what it has built.
pub struct Sources<'a> {
    origins: &'a Origins,
    built: FxHashMap<FileId, Source<&'a str>>,
}

impl<'a> Sources<'a> {
    pub fn new(origins: &'a Origins) -> Sources<'a> {
        Sources {
            origins,
            built: FxHashMap::default(),
        }
    }

    pub fn origins(&self) -> &'a Origins {
        self.origins
    }

    /// What a buffer is called in a message.
    ///
    /// A file is its path. A buffer expansion synthesised has none, because
    /// pasting and stringification make text that is in no file; naming it for
    /// what it is beats naming it for nothing.
    pub fn name(&self, file: FileId) -> String {
        match self.origins.path(file) {
            Some(path) => path.display().to_string(),
            None => "<expansion>".to_string(),
        }
    }
}

impl<'a> Cache<FileId> for Sources<'a> {
    type Storage = &'a str;

    fn fetch(&mut self, id: &FileId) -> Result<&Source<&'a str>, impl fmt::Debug> {
        // Never absent: every id a diagnostic carries came out of this store.
        let origins = self.origins;
        Ok::<_, Infallible>(
            self.built
                .entry(*id)
                .or_insert_with(|| Source::from(origins.text(*id))),
        )
    }

    fn display<'b>(&self, id: &'b FileId) -> Option<impl fmt::Display + 'b> {
        Some(self.name(*id))
    }
}
