//! What a rule emits, and what survives a rollback.
//!
//! There is no tree builder yet, so the shape an event list stands for is
//! read back here as an s-expression. `SOURCE_FILE` and `VERBATIM` are the
//! only node kinds that exist; where a test needs two levels of nesting it
//! uses `VERBATIM` twice, and says what the real grammar would have put
//! there.

use svirig_parse::{Event, Events};
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
    assert_eq!(events.len(), 6);

    events.rollback(snapshot);
    assert_eq!(events.len(), 2);

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
    assert_eq!(events.diagnostics().len(), 3);

    // The attempt did not happen, so neither did what it complained about.
    events.rollback(snapshot);
    assert_eq!(events.diagnostics().len(), 1);
    assert_eq!(events.diagnostics()[0].message, "wrong at 0");
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
    assert_eq!(events.diagnostics().len(), 1);
    assert_eq!(events.take_diagnostics().len(), 1);
    assert!(events.diagnostics().is_empty());
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
    assert_eq!(events.diagnostics().len(), 1);
    assert_eq!(events.diagnostics()[0].message, "wrong at 0");
}
