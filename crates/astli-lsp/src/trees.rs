//! Each open buffer's raw tree: parsed from the buffer alone, with every
//! conditional branch and every macro call as written, so that it exists
//! before any design is loaded.
//!
//! A tree is not `Send`, so the trees stay on the main loop, apart from the
//! buffers, which the pool reads. One is parsed when a request first asks
//! for it, and kept until the buffer's text changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use astli_parse::SyntaxTree;
use lsp_types::Uri;

use crate::buffers::{self, Buffer};
use crate::server::Server;

/// The raw trees parsed so far, by the path of their buffer.
#[derive(Default)]
pub struct Trees(HashMap<PathBuf, SyntaxTree>);

impl Server {
    /// The buffer `uri` names and its raw tree, or `None` if it is not
    /// open.
    pub fn raw(&mut self, uri: &Uri) -> Option<(&Buffer, &SyntaxTree)> {
        let path = buffers::path(uri)?;
        let buffer = self.buffers.get(&path)?;
        let trees = &mut self.trees.0;
        // A tree is checked against its buffer's text rather than told of
        // each edit, so that it cannot outlive a change it missed; one whose
        // buffer has closed goes the next time any tree is asked for.
        trees.retain(|path, _| self.buffers.get(path).is_some());
        let stale = trees
            .get(&path)
            .is_none_or(|tree| tree.source() != &*buffer.text);
        if stale {
            let tree = SyntaxTree::parse(Path::new(&path), buffer.text.to_string());
            trees.insert(path.clone(), tree);
        }
        Some((buffer, &trees[&path]))
    }
}
