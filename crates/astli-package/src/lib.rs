//! A package: the directory a design lives in, and what its `astli.toml`
//! says about it.
//!
//! A run that names no files finds them from the package it is in.
//! [`Root::find`] walks up from a directory to the package around it, the
//! root of the run, and [`Root::read`] reads a manifest named outright.
//! Every path in a manifest is relative to the directory holding it, the
//! package's root, and none may leave it, since the root is what a package
//! publishes. [`Workspace::find`] goes on from a root to the workspace it is
//! in, if any: the nearest manifest in its repository holding `[workspace]`,
//! and every package below that one.
//!
//! A manifest says two things. What a dependent reads of it, a [`Package`],
//! is where its files are, what it requires, and how its tools are
//! configured: its `[fmt]` and `[lint]` tables are handed over as they are,
//! for the driver to read, so that the formatter and the linter need no
//! `serde`. What only a run at the package itself reads, its
//! [`RootTables`], is its tops, tests and builds, its pins and the
//! dependencies it describes. [`Package::at`] reads a dependency, which is
//! the package alone.
//!
//! ```
//! use astli_package::Root;
//!
//! let dir = std::env::temp_dir().join(format!("astli-package-doc-{}", std::process::id()));
//! std::fs::create_dir_all(dir.join("hw/include")).unwrap();
//! std::fs::write(
//!     dir.join("astli.toml"),
//!     "[package]\nname = \"soc\"\nsources = [\"hw\"]\ninclude-dirs = [\"hw/include\"]\n",
//! )
//! .unwrap();
//!
//! let package = Root::find(&dir.join("hw")).unwrap().unwrap().package;
//! assert_eq!(package.manifest.name.as_deref(), Some("soc"));
//! assert_eq!(package.manifest.sources, ["hw"]);
//! # std::fs::remove_dir_all(&dir).unwrap();
//! ```

mod build;
mod dependencies;
mod described;
mod error;
mod features;
mod files;
mod lfs;
pub mod mark;
mod overrides;
mod package;
mod patches;
mod requirement;
mod tops;
mod walk;
mod workspace;

pub use build::{Profile, Profiles, Simulator, Tests};
pub use dependencies::{Dependencies, Dependency};
pub use described::Description;
pub use error::Error;
pub use features::{Base, Feature, Selection};
pub use files::Files;
pub use lfs::lfs_object;
pub use overrides::Declared;
pub use package::{
    BENDER_MANIFEST, ListedBy, MANIFEST, Malformed, Manifest, Package, Root, RootTables,
    WorkspaceTable,
};
pub use patches::{LOCAL, Patches, local};
pub use requirement::{
    At, Requirement, Source, Submodules, Takes, bender_dependencies, bender_name, key,
};
pub use tops::Top;
pub use walk::{EXTENSIONS, Naming, Walk};
pub use workspace::Workspace;
