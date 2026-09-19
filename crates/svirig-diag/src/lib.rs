//! Saying where a diagnostic is, and showing it.
//!
//! [`svirig_text::Diagnostic`] says what is wrong and points at a token. It
//! cannot say *where* that is -- the store is what knows -- and it must not
//! know how to draw, because the lexer would then depend on a terminal in order
//! to report an unterminated comment. This crate is the other half.
//!
//! # Two halves, and only one of them is a terminal
//!
//! [`mod@resolve`] turns what a diagnostic points at into places: which of a
//! token's two locations the message belongs at, the macro calls it came
//! through, the `` `include ``s above it, and what order a run should be read
//! in. None of that is about a terminal, and an editor wants all of it.
//!
//! [`terminal`] is then the part that is: `ariadne`, colour, and a snippet.
//! A second backend -- an LSP speaks `relatedInformation` rather than drawing
//! arrows -- is another module beside it and reuses everything above.
//!
//! # Where the cap belongs
//!
//! Nowhere here. [`resolve_all`] hands back every diagnostic in the order they
//! should be read, and a caller that does not want three hundred of them takes
//! the first few and says how many it dropped. How much is worth printing is an
//! opinion about a terminal, and this crate does not hold those.
//!
//! See `docs/diagnostics.md`.

pub mod resolve;
pub mod sources;
pub mod terminal;

pub use resolve::{Resolved, Through, resolve, resolve_all};
pub use sources::Sources;
pub use terminal::{Style, write};
