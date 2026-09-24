//! Source text provider adapting [`Origins`] for `ariadne` diagnostic rendering.
//!
//! [`Sources`] implements `ariadne`'s [`Cache`] trait by looking up file buffers
//! stored in an [`Origins`] instance. Each requested file buffer is converted on
//! demand into an `ariadne::Source` tracking line and column boundaries.

use std::convert::Infallible;
use std::fmt;

use ariadne::{Cache, Source};
use astli_text::{Origins, SourceId};
use rustc_hash::FxHashMap;

/// An `ariadne` source cache backed by a reference to an [`Origins`] database.
pub struct Sources<'a> {
    origins: &'a Origins,
    built: FxHashMap<SourceId, Source<&'a str>>,
}

impl<'a> Sources<'a> {
    /// Creates a new source cache backed by `origins`.
    pub fn new(origins: &'a Origins) -> Sources<'a> {
        Sources {
            origins,
            built: FxHashMap::default(),
        }
    }

    /// Returns the underlying [`Origins`] reference.
    pub fn origins(&self) -> &'a Origins {
        self.origins
    }

    /// Returns the display name for a given file buffer.
    pub fn name(&self, file: SourceId) -> String {
        match self.origins.path(file) {
            Some(path) => path.display().to_string(),
            None => "<expansion>".to_string(),
        }
    }
}

impl<'a> Cache<SourceId> for Sources<'a> {
    type Storage = &'a str;

    fn fetch(&mut self, id: &SourceId) -> Result<&Source<&'a str>, impl fmt::Debug> {
        let origins = self.origins;
        Ok::<_, Infallible>(
            self.built
                .entry(*id)
                .or_insert_with(|| Source::from(origins.text(*id))),
        )
    }

    fn display<'b>(&self, id: &'b SourceId) -> Option<impl fmt::Display + 'b> {
        Some(self.name(*id))
    }
}
