//! What a rule emits, and how it takes it back.
//!
//! A rule never touches a tree. It appends to a flat [`Events`], and the tree
//! is built from that afterwards -- which is what makes speculative parsing a
//! [`Snapshot`] and a truncate rather than a rewrite. See the [module
//! docs](super).
//!
//! # A node is opened before its kind is known
//!
//! [`Events::start`] does not take a kind, because a rule usually cannot say
//! what it is parsing until it has parsed some of it: the same leading tokens
//! open a declaration and an expression statement. So `start` reserves a slot
//! and hands back a [`Marker`], and the kind is written into that slot later
//! by [`Marker::complete`] -- or never, by [`Marker::abandon`].
//!
//! An abandoned slot stays where it is, as a [`Event::Tombstone`], because
//! anything the rule emitted in the meantime is addressed by position and
//! removing it would move all of that. Tombstones are dropped by
//! [`Events::resolve`], not by `abandon`.
//!
//! # Preceding
//!
//! Left-associative operators need the opposite of a marker: `a + b` is parsed
//! by reading `a`, and only then discovering that it is the left operand of
//! something. [`Completed::precede`] reopens a finished node from the outside,
//! recording on it a *forward* reference to the node that will contain it.
//! [`Events::resolve`] turns those references back into ordinary nesting.

use std::mem;

use crate::SyntaxKind;

/// One step of a parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Open a node.
    ///
    /// `forward_parent` is the distance from this event to the `Start` of a
    /// node that must contain this one -- see [`Completed::precede`]. It is
    /// always `None` after [`Events::resolve`].
    Start {
        kind: SyntaxKind,
        forward_parent: Option<u32>,
    },
    /// A slot that no longer opens anything: either a marker that was
    /// abandoned, or one that [`Events::resolve`] has moved. Skipped when the
    /// tree is built, and never present after `resolve`.
    Tombstone,
    /// Take the next token of the source, as this kind.
    ///
    /// The kind is carried rather than read back from the source because a
    /// rule may reclassify what it consumes -- an `IDENT` that turns out to
    /// name a type, say.
    Token { kind: SyntaxKind },
    /// Close the innermost open node.
    Finish,
}

/// How far along a parse was, so that it can be put back.
///
/// Taken by [`Events::snapshot`] and spent by [`Events::rollback`]. It is
/// deliberately not the whole parser state: the token position belongs to the
/// token source, and whoever owns both is what pairs them up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    events: u32,
    open: u32,
    precedes: u32,
}

/// The events of one parse, in the order they were emitted.
#[derive(Debug, Default)]
pub struct Events {
    events: Vec<Event>,
    /// Markers started and not yet completed or abandoned. Only a rollback
    /// reads it, to refuse one that would cut a node in half.
    open: u32,
    /// Which events [`Completed::precede`] has written a forward parent into,
    /// in the order it wrote them.
    ///
    /// A forward parent is the one thing in the list that points *ahead* of
    /// itself, so it is the one thing a truncate can leave dangling. Keeping
    /// the indices means undoing them costs what was undone rather than a
    /// scan of everything that was not.
    precedes: Vec<u32>,
}

impl Events {
    pub fn new() -> Events {
        Events::default()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Opens a node whose kind is not decided yet.
    pub fn start(&mut self) -> Marker {
        let pos = self.events.len() as u32;
        self.events.push(Event::Tombstone);
        self.open += 1;
        Marker::new(pos)
    }

    /// Consumes the source's next token as `kind`.
    pub fn token(&mut self, kind: SyntaxKind) {
        self.events.push(Event::Token { kind });
    }

    /// Records that an open marker has been dealt with.
    fn close(&mut self) {
        // Zero here means the marker belongs to a different `Events`, which
        // otherwise surfaces as an arithmetic overflow in a release build's
        // silence.
        self.open = self
            .open
            .checked_sub(1)
            .expect("a marker was completed against a different `Events`");
    }

    /// How far along this is, for a later [`Events::rollback`].
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            events: self.events.len() as u32,
            open: self.open,
            precedes: self.precedes.len() as u32,
        }
    }

    /// Throws away everything emitted since `snapshot`.
    ///
    /// # Panics
    ///
    /// If a marker opened before the snapshot was completed after it, or one
    /// opened after it is still live. Either way the events being dropped are
    /// not a self-contained attempt, and dropping them would leave a node that
    /// is opened and never closed. A speculative rule has to finish what it
    /// starts before it can be undone.
    pub fn rollback(&mut self, snapshot: Snapshot) {
        assert_eq!(
            self.open, snapshot.open,
            "rolling back across a marker that is still open"
        );

        // A node that was reopened from the outside points forward at the
        // marker that reopened it. Truncating would leave that pointing at
        // nothing, and `resolve` would follow it into whatever landed there
        // next -- so the pointers go back before the events do.
        while self.precedes.len() > snapshot.precedes as usize {
            let at = self.precedes.pop().expect("checked by the loop");
            if let Some(Event::Start { forward_parent, .. }) = self.events.get_mut(at as usize) {
                *forward_parent = None;
            }
        }

        self.events.truncate(snapshot.events as usize);
    }

    /// The same events with every forward reference turned into ordinary
    /// nesting and every tombstone removed, which is the order a tree is built
    /// in.
    ///
    /// The result contains no [`Event::Tombstone`], and no [`Event::Start`]
    /// with a `forward_parent`.
    pub fn resolve(mut self) -> Vec<Event> {
        assert_eq!(self.open, 0, "a marker was never completed or abandoned");

        let mut resolved = Vec::with_capacity(self.events.len());
        // A node and everything preceding it, innermost last.
        let mut nesting = Vec::new();

        for at in 0..self.events.len() {
            match mem::replace(&mut self.events[at], Event::Tombstone) {
                // Either abandoned, or already emitted from a chain below.
                Event::Tombstone => {}
                Event::Token { kind } => resolved.push(Event::Token { kind }),
                Event::Finish => resolved.push(Event::Finish),
                Event::Start {
                    kind,
                    mut forward_parent,
                } => {
                    nesting.push(kind);
                    // Each link is a node that contains the one before it, so
                    // the chain is walked forwards and opened backwards.
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

/// A node that has been opened and not yet given a kind.
///
/// Must be [completed](Marker::complete) or [abandoned](Marker::abandon);
/// dropping one is a bug, and panics rather than leaving a node that is never
/// closed.
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

    /// Gives the node its kind and closes it.
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

    /// Drops the node, keeping whatever was emitted inside it.
    ///
    /// The children become children of whatever encloses this, which is what
    /// a rule wants when it turns out to have been parsing something simpler
    /// than it expected.
    pub fn abandon(mut self, events: &mut Events) {
        self.bomb.defuse();
        events.close();
        // Nothing was emitted inside it, so the slot can go rather than
        // becoming a tombstone nothing will ever look at.
        if self.pos as usize == events.events.len() - 1 {
            events.events.pop();
        }
    }
}

/// A node that has been given its kind.
#[derive(Debug, Clone, Copy)]
pub struct Completed {
    pos: u32,
}

impl Completed {
    /// Opens a new node that will contain this one.
    ///
    /// For the left-associative case: `a` is complete before the `+` that
    /// makes it an operand is seen, and the node for the whole expression has
    /// to start in front of it.
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

    /// The kind this node was completed with.
    pub fn kind(self, events: &Events) -> SyntaxKind {
        match events.events[self.pos as usize] {
            Event::Start { kind, .. } => kind,
            ref other => unreachable!("a completed node must be a start, not {other:?}"),
        }
    }
}

/// Panics if it is dropped without being defused.
///
/// A [`Marker`] carries one because losing a marker does not fail where it
/// happens: the node stays open, and what goes wrong is the *shape* of a tree
/// built much later, somewhere else.
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
        // Unwinding drops everything, marker included, and a panic in a drop
        // during a panic aborts the process -- so the second one has to stay
        // quiet and let the first be reported.
        if self.live && !std::thread::panicking() {
            panic!("a marker was dropped without being completed or abandoned");
        }
    }
}
