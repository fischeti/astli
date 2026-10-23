//! A package: the directory a design lives in, and what its `astli.toml`
//! says about it.
//!
//! A run that names no files finds them from the package it is in.
//! [`Package::find`] walks up from a directory to the package around it, and
//! [`Package::read`] reads a manifest named outright. Every path in a
//! manifest is relative to the directory holding it, the package's root, and
//! none may leave it, since the root is what a package publishes.
//!
//! The manifest also configures the tools: its `[fmt]` and `[lint]` tables
//! are handed over as they are, for the driver to read, so that the
//! formatter and the linter need no `serde`.
//!
//! ```
//! use astli_package::Package;
//!
//! let dir = std::env::temp_dir().join(format!("astli-package-doc-{}", std::process::id()));
//! std::fs::create_dir_all(dir.join("hw/include")).unwrap();
//! std::fs::write(
//!     dir.join("astli.toml"),
//!     "[package]\nname = \"soc\"\nsources = [\"hw\"]\ninclude-dirs = [\"hw/include\"]\n",
//! )
//! .unwrap();
//!
//! let package = Package::find(&dir.join("hw")).unwrap().unwrap();
//! assert_eq!(package.name.as_deref(), Some("soc"));
//! assert_eq!(package.sources, [package.root.join("hw")]);
//! # std::fs::remove_dir_all(&dir).unwrap();
//! ```

mod dependencies;
mod error;
mod features;
mod files;
mod overrides;
mod package;
mod requirement;
mod walk;

pub use dependencies::{Dependencies, Dependency};
pub use error::Error;
pub use features::{Feature, Selection};
pub use files::Files;
pub use overrides::Declared;
pub use package::{BENDER_MANIFEST, MANIFEST, Package, manifest_dependencies};
pub use requirement::{At, Requirement, Source, bender_dependencies};
