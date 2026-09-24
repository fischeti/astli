//! Search path configuration and resolution for `` `include `` directives.

use std::path::{Path, PathBuf};

use astli_text::clean;

/// Maximum nesting depth for `` `include `` directives before triggering an error.
///
/// IEEE 1800-2023 §22.4 requires support for at least 15 levels of nesting.
pub const MAX_DEPTH: usize = 200;

/// Search directories for resolving `` `include `` directive targets.
#[derive(Debug, Clone, Default)]
pub(crate) struct Includes {
    /// Search paths for quoted includes (`` `include "filename" ``), checked
    /// after the directory containing the current file.
    pub quoted: Vec<PathBuf>,
    /// Search paths for angle-bracket includes (`` `include <filename> ``).
    pub angle: Vec<PathBuf>,
}

impl Includes {
    /// Resolves candidate file paths for `name` in priority order.
    ///
    /// If `name` is an absolute path, only the normalized path is returned.
    /// For relative quoted includes (`angle = false`), the parent directory of
    /// `used_in` is checked first, followed by the configured search paths.
    pub fn search(&self, name: &str, used_in: Option<&Path>, angle: bool) -> Vec<PathBuf> {
        let name = Path::new(name);
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
