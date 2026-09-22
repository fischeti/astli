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

#[cfg(test)]
mod tests {
    //! What a rule emits, and what survives a rollback. Only `SOURCE_FILE` and
    //! `VERBATIM` are used as node kinds, since the kind is not under test.

    use super::*;
    use svirig_syntax::SyntaxKind::*;

    /// The tree the events describe, as `(NODE child child)`.
    fn shape(events: Events) -> String {
        let mut out = String::new();
        let mut depth = 0usize;
        for event in events.resolve() {
            match event {
                Event::Start {
                    kind,
                    forward_parent,
                } => {
                    assert_eq!(forward_parent, None, "resolve left a forward parent");
                    if depth > 0 {
                        out.push(' ');
                    }
                    out.push_str(&format!("({kind:?}"));
                    depth += 1;
                }
                Event::Token { kind } => out.push_str(&format!(" {kind:?}")),
                Event::Finish => {
                    out.push(')');
                    depth -= 1;
                }
                Event::Tombstone => panic!("resolve left a tombstone"),
            }
        }
        out
    }

    #[test]
    fn a_node_contains_what_was_emitted_inside_it() {
        let mut events = Events::new();
        let file = events.start();
        events.token(MODULE_KW);
        events.token(IDENT);
        events.token(SEMICOLON);
        file.complete(&mut events, SOURCE_FILE);

        assert_eq!(shape(events), "(SOURCE_FILE MODULE_KW IDENT SEMICOLON)");
    }

    #[test]
    fn nodes_nest() {
        let mut events = Events::new();
        let file = events.start();
        let inner = events.start();
        events.token(IDENT);
        inner.complete(&mut events, VERBATIM);
        events.token(SEMICOLON);
        file.complete(&mut events, SOURCE_FILE);

        assert_eq!(shape(events), "(SOURCE_FILE (VERBATIM IDENT) SEMICOLON)");
    }

    #[test]
    fn abandoning_the_last_marker_leaves_nothing_behind() {
        let mut events = Events::new();
        let file = events.start();
        events.token(IDENT);
        let speculative = events.start();
        speculative.abandon(&mut events);
        events.token(SEMICOLON);
        file.complete(&mut events, SOURCE_FILE);

        assert_eq!(shape(events), "(SOURCE_FILE IDENT SEMICOLON)");
    }

    #[test]
    fn abandoning_a_marker_keeps_its_children() {
        // A rule that opened a node, parsed into it, and then found out it was
        // parsing something simpler: the tokens belong to whatever encloses it.
        let mut events = Events::new();
        let file = events.start();
        let speculative = events.start();
        events.token(IDENT);
        events.token(SEMICOLON);
        speculative.abandon(&mut events);
        file.complete(&mut events, SOURCE_FILE);

        assert_eq!(shape(events), "(SOURCE_FILE IDENT SEMICOLON)");
    }

    #[test]
    fn preceding_wraps_a_finished_node() {
        // What `a + b` needs: `a` is a complete node before the operator that
        // makes it an operand has been seen.
        let mut events = Events::new();
        let file = events.start();

        let left = events.start();
        events.token(IDENT);
        let left = left.complete(&mut events, VERBATIM);

        let binary = left.precede(&mut events);
        events.token(PLUS);
        events.token(IDENT);
        binary.complete(&mut events, VERBATIM);

        file.complete(&mut events, SOURCE_FILE);

        assert_eq!(
            shape(events),
            "(SOURCE_FILE (VERBATIM (VERBATIM IDENT) PLUS IDENT))"
        );
    }

    #[test]
    fn preceding_twice_nests_outward() {
        // `a + b + c`, left-associative: each operator reopens the whole of what
        // came before it.
        let mut events = Events::new();
        let first = events.start();
        events.token(IDENT);
        let mut done = first.complete(&mut events, VERBATIM);

        for _ in 0..2 {
            let outer = done.precede(&mut events);
            events.token(PLUS);
            events.token(IDENT);
            done = outer.complete(&mut events, VERBATIM);
        }

        assert_eq!(
            shape(events),
            "(VERBATIM (VERBATIM (VERBATIM IDENT) PLUS IDENT) PLUS IDENT)"
        );
    }

    #[test]
    fn a_rollback_puts_the_events_back() {
        let mut events = Events::new();
        let file = events.start();
        events.token(MODULE_KW);

        let snapshot = events.snapshot();
        let attempt = events.start();
        events.token(IDENT);
        events.token(SEMICOLON);
        attempt.complete(&mut events, VERBATIM);
        assert_eq!(events.events.len(), 6);

        events.rollback(snapshot);
        assert_eq!(events.events.len(), 2);

        events.token(IDENT);
        file.complete(&mut events, SOURCE_FILE);
        assert_eq!(shape(events), "(SOURCE_FILE MODULE_KW IDENT)");
    }

    #[test]
    fn a_rollback_may_be_taken_twice_from_the_same_point() {
        let mut events = Events::new();
        let file = events.start();
        let snapshot = events.snapshot();

        for kind in [MODULE_KW, PACKAGE_KW] {
            let attempt = events.start();
            events.token(kind);
            attempt.complete(&mut events, VERBATIM);
            events.rollback(snapshot);
        }

        events.token(INTERFACE_KW);
        file.complete(&mut events, SOURCE_FILE);
        assert_eq!(shape(events), "(SOURCE_FILE INTERFACE_KW)");
    }

    #[test]
    fn a_rollback_undoes_a_node_that_was_reopened_from_the_outside() {
        // A forward parent is the one thing in the list that points *ahead* of
        // itself, so a truncate is the one thing that can leave it dangling --
        // and it would then be followed into whatever landed at that index next.
        // This is the shape a speculative parse takes: try the left-associative
        // reading, find it wrong, put it back.
        let mut events = Events::new();
        let file = events.start();

        let first = events.start();
        events.token(IDENT);
        let operand = first.complete(&mut events, VERBATIM);

        let snapshot = events.snapshot();
        let outer = operand.precede(&mut events);
        events.token(PLUS);
        events.token(IDENT);
        outer.complete(&mut events, VERBATIM);

        events.rollback(snapshot);
        events.token(SEMICOLON);
        file.complete(&mut events, SOURCE_FILE);

        // The operand survives, unwrapped, and nothing follows a pointer into
        // where the semicolon now sits.
        assert_eq!(shape(events), "(SOURCE_FILE (VERBATIM IDENT) SEMICOLON)");
    }

    #[test]
    #[should_panic(expected = "rolling back across a marker that is still open")]
    fn a_rollback_that_would_cut_a_node_in_half_is_refused() {
        let mut events = Events::new();
        let snapshot = events.snapshot();
        let open = events.start();
        events.rollback(snapshot);
        open.abandon(&mut events);
    }

    #[test]
    #[should_panic(expected = "dropped without being completed or abandoned")]
    fn losing_a_marker_is_a_bug() {
        let mut events = Events::new();
        let _ = events.start();
    }

    #[test]
    #[should_panic(expected = "never completed or abandoned")]
    fn resolving_with_a_node_still_open_is_a_bug() {
        let mut events = Events::new();
        // Leaked rather than dropped, so that the marker's own bomb stays quiet
        // and `resolve` is what reports the open node. Both catch the same
        // mistake; this is the one that catches it when the marker is still alive
        // somewhere.
        std::mem::forget(events.start());
        events.resolve();
    }

    /// A diagnostic to roll back, pointing anywhere: what is under test is the
    /// side list's length, not where it says to look.
    fn complaint(at: u32) -> svirig_text::Diagnostic {
        use svirig_text::{Code, Diagnostic, FileId, Span, TokenOrigin};
        let mut origins = svirig_text::Origins::new();
        let file: FileId = origins.add_file("f.sv", "x".repeat(at as usize + 1));
        Diagnostic::error(
            Code("test"),
            TokenOrigin::written(Span::point(file, at)),
            format!("wrong at {at}"),
        )
    }

    #[test]
    fn a_rolled_back_attempt_takes_its_complaint_with_it() {
        let mut events = Events::new();
        events.report(complaint(0));

        let snapshot = events.snapshot();
        let marker = events.start();
        events.report(complaint(1));
        events.report(complaint(2));
        marker.complete(&mut events, VERBATIM);
        assert_eq!(events.diagnostics.len(), 3);

        // The attempt did not happen, so neither did what it complained about.
        events.rollback(snapshot);
        assert_eq!(events.diagnostics.len(), 1);
        assert_eq!(events.diagnostics[0].message, "wrong at 0");
    }

    #[test]
    fn an_attempt_that_is_kept_keeps_its_complaint() {
        let mut events = Events::new();
        let snapshot = events.snapshot();
        let marker = events.start();
        events.report(complaint(1));
        marker.complete(&mut events, VERBATIM);

        // Snapshot taken and never spent: nothing is undone, so nothing is
        // withdrawn either.
        let _ = snapshot;
        assert_eq!(events.diagnostics.len(), 1);
        assert_eq!(events.take_diagnostics().len(), 1);
        assert!(events.diagnostics.is_empty());
    }

    #[test]
    fn a_snapshot_taken_after_a_complaint_does_not_withdraw_it() {
        let mut events = Events::new();
        events.report(complaint(0));
        let snapshot = events.snapshot();
        events.report(complaint(1));

        events.rollback(snapshot);
        // Only what came after the snapshot goes; the length is the whole of the
        // mechanism, exactly as it is for `precedes`.
        assert_eq!(events.diagnostics.len(), 1);
        assert_eq!(events.diagnostics[0].message, "wrong at 0");
    }
}
