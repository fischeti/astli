//! The parser: a hand-written recursive descent over a token source, emitting
//! a flat list of [events](event) that a tree is built from afterwards.
//!
//! # Why events, and not a builder
//!
//! The obvious shape is to drive `rowan`'s `GreenNodeBuilder` inline, opening
//! and closing nodes as rules recurse. It works for a language that is nearly
//! LL(1), and SystemVerilog is not: `foo bar;` is a declaration only if `foo`
//! names a type, and `(a)(b)` is a cast or a call. Both are settled by parsing
//! one way, finding out, and **undoing it** -- and a builder's checkpoint can
//! wrap a node retroactively but cannot take one back.
//!
//! So a rule appends to a `Vec<Event>` instead. A snapshot is how long that
//! vector is; undoing is a truncate. The tree is built once, at the end, from
//! events that are known to be final.
//!
//! # The layout this grows into
//!
//! * [`event`] -- the event list, markers over it, and rollback.
//! * [`source`] -- the tokens, parameterised so that one grammar serves both
//!   the raw stream the formatter reads and the expanded one a compiler would.
//! * the builder, which walks events against the original tokens and puts the
//!   trivia back.
//! * the grammar itself, split by what it parses.
//!
//! See `docs/plan.md` and `docs/next.md`.

pub mod event;
pub mod source;

pub use event::{Completed, Event, Events, Marker, Snapshot};
pub use source::{Expanded, Position, Raw, Tokens};
