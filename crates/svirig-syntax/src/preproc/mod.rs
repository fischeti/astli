//! Preprocessing: directives, macros, includes, conditionals.
//!
//! Laid out as the crate it will become. `svirig-preproc` is meant to be
//! publishable on its own -- the Rust ecosystem has no good SystemVerilog
//! preprocessor -- so nothing in here may reach back into the parser.
//!
//! Only directive recognition exists so far. See `docs/preprocessor.md`.

pub mod directive;

pub use directive::{Directive, DirectiveName, Formal, Include, MacroDef, Operands, scan};
