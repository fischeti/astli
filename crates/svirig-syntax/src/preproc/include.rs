//! Where an `` `include `` looks for the file it names.
//!
//! Expanded mode only. The formatter never follows an include
//! ([D6](../../../../docs/plan.md)): each file is formatted alone, and a
//! header's text is not part of the file being formatted.
//!
//! Only the search is here. Reading a candidate and adding it to the store
//! is [`Origins::load_included`](svirig_text::Origins::load_included), since
//! what a path holds is not a question about SystemVerilog.
//!
//! # Where it looks
//!
//! 22.4 gives the quoted form the including file's own directory and then an
//! implementation-defined search, and reserves the angle form for the files
//! the implementation supplies. This one supplies none, so the angle list is
//! empty until a driver fills it.

use std::path::{Path, PathBuf};

use svirig_text::clean;

/// How deep `` `include `` may nest before it is treated as runaway.
///
/// 22.4 requires at least 15 levels, and real code stays far below that. The
/// limit is set high because it is a backstop rather than a rule: a cycle is
/// normally caught by the store's check for a file already open, and this only
/// has to catch the ones that check cannot see.
pub const MAX_DEPTH: usize = 200;

/// Where `` `include `` looks.
#[derive(Debug, Clone, Default)]
pub struct Includes {
    /// Searched for `` `include "f.svh" ``, after the directory of the file the
    /// include is used in.
    pub quoted: Vec<PathBuf>,
    /// Searched for `` `include <f.svh> ``. Empty unless a driver fills it:
    /// 22.4 reserves this form for files the implementation supplies, and this
    /// one supplies none.
    pub angle: Vec<PathBuf>,
}

impl Includes {
    /// Nothing on the search path.
    pub fn new() -> Includes {
        Includes::default()
    }

    /// The candidates for `name`, in the order they are to be tried.
    ///
    /// `used_in` is the file the `` `include `` is used in, which is the file
    /// it is written in unless a macro carried it somewhere else.
    pub fn search(&self, name: &str, used_in: Option<&Path>, angle: bool) -> Vec<PathBuf> {
        let name = Path::new(name);
        // An absolute name says where it is; there is nothing to search.
        if name.is_absolute() {
            return vec![clean(name)];
        }

        let own = (!angle).then(|| used_in.and_then(Path::parent)).flatten();
        let list = if angle { &self.angle } else { &self.quoted };

        own.into_iter()
            .chain(list.iter().map(PathBuf::as_path))
            .map(|dir| clean(&dir.join(name)))
            .collect()
    }
}
