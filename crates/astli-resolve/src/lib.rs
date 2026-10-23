//! Where a design's dependencies come from: git repositories mirrored into a
//! cache shared by every design on the machine, and checked out from there.
//!
//! [`Store`] is the cache. It holds one bare [`Mirror`] per repository,
//! keyed by its URL, so that a fork and the repository it forks are two
//! mirrors. A mirror answers what resolution asks of a package without a
//! checkout: its versions, the commit a `rev` names, and its manifest at a
//! commit. [`Mirror::checkout`] then writes one commit out as plain files,
//! once, read by every design that needs it.
//!
//! The crate serves the `astli` driver; its API follows what the driver
//! needs rather than promising anything to others.

mod error;
mod mirror;
mod store;

pub use error::Error;
pub use mirror::Mirror;
pub use store::{Store, canonical};
