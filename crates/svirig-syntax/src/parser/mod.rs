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
//! * [`mod@build`] -- walks the events against the original tokens and puts the
//!   trivia back.
//! * the grammar itself, split by what it parses. **Not written yet**: every
//!   file currently parses to one [`VERBATIM`]
//!   node, which is what [`parse`] will keep doing for whatever the rules
//!   cannot make sense of.
//!
//! See `docs/plan.md` and `docs/next.md`.

pub mod build;
pub mod event;
pub mod source;

pub use build::build;
pub use event::{Completed, Event, Events, Marker, Snapshot};
pub use source::{Expanded, Position, Raw, Tokens};

use crate::preproc::Input;
use crate::{SyntaxKind::*, SyntaxNode};

/// Parses one file into a lossless tree.
///
/// There is no grammar yet, so the whole file comes back as one
/// [`VERBATIM`] node: correct, useless, and the
/// shape every later rung whittles down. What already holds is the property
/// the rungs must not break -- the tree's text is the file's, byte for byte.
pub fn parse(input: Input) -> SyntaxNode {
    let mut tokens = Raw::new(input);
    let mut events = Events::new();

    let file = events.start();
    let rest = events.start();
    while !tokens.at_end() {
        events.token(tokens.kind(0));
        tokens.bump();
    }
    rest.complete(&mut events, VERBATIM);
    file.complete(&mut events, SOURCE_FILE);

    SyntaxNode::new_root(build(&events.resolve(), input))
}
