//! Parser event stream and tree construction primitives.
//!
//! Rather than building syntax nodes eagerly, recursive descent rules append flat
//! [`Event`] items to an [`Events`] buffer. This decouples tree construction from parsing,
//! enabling lightweight speculative parsing and backtracking via [`Snapshot`] truncation.
//!
//! ### Positional Precedence Handling
//!
//! Left-associative binary expressions (such as `a + b + c`) require wrapping an already
//! completed child node into a new parent node. Because event positions are fixed,
//! [`Completed::precede`] records a `forward_parent` offset on the completed node's `Start`
//! event instead of moving existing events in memory:
//!
//! ```text
//!    #   event                     forward parent
//!   ─────────────────────────────────────────────
//!    0   Start  NAME_REF   (a)     ──▶ 3
//!    1   Token  IDENT "a"
//!    2   Finish
//!    3   Start  BIN_EXPR   (+)     ──▶ 9
//!    4   Token  PLUS
//!    5   Start  NAME_REF   (b)
//!    6   Token  IDENT "b"
//!    7   Finish
//!    8   Finish
//!    9   Start  BIN_EXPR   (+)
//!   10   Token  PLUS
//!   11   Start  NAME_REF   (c)
//!   12   Token  IDENT "c"
//!   13   Finish
//!   14   Finish
//! ```
//!
//! During [`Events::resolve`], forward references are resolved in a single forward pass,
//! producing properly nested start and finish events without tombstones.

use std::mem;

use svirig_syntax::SyntaxKind;
use svirig_text::Diagnostic;

/// A single event emitted during parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Opens a syntax node.
    ///
    /// `forward_parent` specifies the offset to an enclosing parent node created
    /// via [`Completed::precede`]. This is resolved to `None` after [`Events::resolve`].
    Start {
        kind: SyntaxKind,
        forward_parent: Option<u32>,
    },
    /// An abandoned or relocated event slot, ignored during tree construction.
    Tombstone,
    /// Consumes the next token from the source as `kind`.
    Token { kind: SyntaxKind },
    /// Closes the currently active syntax node.
    Finish,
}

/// A checkpoint of parser state used to roll back speculative attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    events: u32,
    open: u32,
    precedes: u32,
    diagnostics: u32,
}

/// A buffer of parser events recorded in sequence.
#[derive(Debug, Default)]
pub struct Events {
    events: Vec<Event>,
    /// Number of markers currently open.
    open: u32,
    /// Event indices where forward parent pointers were recorded via [`Completed::precede`].
    precedes: Vec<u32>,
    /// Diagnostics accumulated during parsing.
    diagnostics: Vec<Diagnostic>,
}

impl Events {
    /// Creates a new, empty event buffer.
    pub fn new() -> Events {
        Events::default()
    }

    /// Returns the number of events currently in the buffer.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Returns `true` if no events have been recorded.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Begins a new syntax node and returns a marker to complete or abandon it later.
    pub fn start(&mut self) -> Marker {
        let pos = self.events.len() as u32;
        self.events.push(Event::Tombstone);
        self.open += 1;
        Marker::new(pos)
    }

    /// Appends a token consumption event.
    pub fn token(&mut self, kind: SyntaxKind) {
        self.events.push(Event::Token { kind });
    }

    /// Decrements the open marker count when a marker is completed or abandoned.
    fn close(&mut self) {
        self.open = self
            .open
            .checked_sub(1)
            .expect("a marker was completed against a different `Events`");
    }

    /// Captures a snapshot of the current event buffer state.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            events: self.events.len() as u32,
            open: self.open,
            precedes: self.precedes.len() as u32,
            diagnostics: self.diagnostics.len() as u32,
        }
    }

    /// Rolls back the event stream to the state recorded in `snapshot`.
    ///
    /// # Panics
    ///
    /// Panics if any marker opened since `snapshot` is still open.
    pub fn rollback(&mut self, snapshot: Snapshot) {
        assert_eq!(
            self.open, snapshot.open,
            "rolling back across a marker that is still open"
        );

        while self.precedes.len() > snapshot.precedes as usize {
            let at = self.precedes.pop().expect("checked by the loop");
            if let Some(Event::Start { forward_parent, .. }) = self.events.get_mut(at as usize) {
                *forward_parent = None;
            }
        }

        self.events.truncate(snapshot.events as usize);
        self.diagnostics.truncate(snapshot.diagnostics as usize);
    }

    /// Records a diagnostic message in the parser state.
    pub fn report(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Returns all diagnostics recorded and not rolled back.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Takes all recorded diagnostics, leaving an empty list in their place.
    pub fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    /// Resolves all forward parent links and drops tombstones, producing a sequential event list.
    ///
    /// # Panics
    ///
    /// Panics if any markers remain open.
    pub fn resolve(mut self) -> Vec<Event> {
        assert_eq!(self.open, 0, "a marker was never completed or abandoned");

        let mut resolved = Vec::with_capacity(self.events.len());
        let mut nesting = Vec::new();

        for at in 0..self.events.len() {
            match self.events[at] {
                Event::Tombstone => {}
                Event::Token { kind } => resolved.push(Event::Token { kind }),
                Event::Finish => resolved.push(Event::Finish),
                Event::Start {
                    kind,
                    mut forward_parent,
                } => {
                    nesting.push(kind);
                    let mut link = at;
                    while let Some(distance) = forward_parent {
                        link += distance as usize;
                        match mem::replace(&mut self.events[link], Event::Tombstone) {
                            Event::Start {
                                kind,
                                forward_parent: next,
                            } => {
                                nesting.push(kind);
                                forward_parent = next;
                            }
                            other => {
                                unreachable!("a forward parent must be a start, not {other:?}")
                            }
                        }
                    }
                    for kind in nesting.drain(..).rev() {
                        resolved.push(Event::Start {
                            kind,
                            forward_parent: None,
                        });
                    }
                }
            }
        }

        resolved
    }
}

/// An uncompleted marker representing an open syntax node in the event stream.
///
/// Markers must be closed via [`Marker::complete`] or discarded via [`Marker::abandon`].
/// Dropping a marker without handling it panics to prevent malformed tree structures.
#[derive(Debug)]
pub struct Marker {
    pos: u32,
    bomb: Bomb,
}

impl Marker {
    fn new(pos: u32) -> Marker {
        Marker {
            pos,
            bomb: Bomb::new(),
        }
    }

    /// Completes the open node with `kind` and emits a corresponding finish event.
    pub fn complete(mut self, events: &mut Events, kind: SyntaxKind) -> Completed {
        self.bomb.defuse();
        events.events[self.pos as usize] = Event::Start {
            kind,
            forward_parent: None,
        };
        events.events.push(Event::Finish);
        events.close();
        Completed { pos: self.pos }
    }

    /// Discards the open node, leaving its children to attach to the parent node.
    pub fn abandon(mut self, events: &mut Events) {
        self.bomb.defuse();
        events.close();
        if self.pos as usize == events.events.len() - 1 {
            events.events.pop();
        }
    }
}

/// Handle to a completed syntax node in the event stream.
#[derive(Debug, Clone, Copy)]
pub struct Completed {
    pos: u32,
}

impl Completed {
    /// Opens a new parent node starting before this completed node.
    pub fn precede(self, events: &mut Events) -> Marker {
        let marker = events.start();
        match &mut events.events[self.pos as usize] {
            Event::Start { forward_parent, .. } => {
                *forward_parent = Some(marker.pos - self.pos);
            }
            other => unreachable!("a completed node must be a start, not {other:?}"),
        }
        events.precedes.push(self.pos);
        marker
    }

    /// Returns the syntax kind this node was completed with.
    pub fn kind(self, events: &Events) -> SyntaxKind {
        match events.events[self.pos as usize] {
            Event::Start { kind, .. } => kind,
            ref other => unreachable!("a completed node must be a start, not {other:?}"),
        }
    }
}

/// Safety guard that panics if a [`Marker`] is dropped without being completed or abandoned.
#[derive(Debug)]
struct Bomb {
    live: bool,
}

impl Bomb {
    fn new() -> Bomb {
        Bomb { live: true }
    }

    fn defuse(&mut self) {
        self.live = false;
    }
}

impl Drop for Bomb {
    fn drop(&mut self) {
        if self.live && !std::thread::panicking() {
            panic!("a marker was dropped without being completed or abandoned");
        }
    }
}
