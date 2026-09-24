//! What a diagnostic can express, built by hand.
//!
//! No crate here produces one yet, so these are the shapes the producers are
//! being written against: a complaint about a token that was written where it
//! is used, and one about a token a macro placed, which is the case the type
//! exists for.

use svirig_text::{Code, Diagnostic, Expansion, Origins, Severity, Span};

const UNDEFINED: Code = Code("undefined-macro");

/// A span covering the first occurrence of `needle`, so that the tests read as
/// source rather than as byte arithmetic.
fn find(origins: &Origins, file: svirig_text::SourceId, needle: &str) -> Span {
    let text = origins.text(file);
    let start = text.find(needle).unwrap_or_else(|| panic!("no {needle:?}"));
    Span::new(file, start as u32, (start + needle.len()) as u32)
}

#[test]
fn severity_orders_by_how_much_it_matters() {
    assert!(Severity::Error > Severity::Warning);
    assert!(Severity::Warning > Severity::Note);
    assert!(Severity::Note > Severity::Help);

    // Which is the point: the worst of a run is a `max`.
    let run = [Severity::Note, Severity::Error, Severity::Warning];
    assert_eq!(run.iter().max(), Some(&Severity::Error));
    assert!(run.iter().copied().max().is_some_and(Severity::is_error));
}

#[test]
fn a_code_survives_as_text() {
    assert_eq!(UNDEFINED.as_str(), "undefined-macro");
    assert_eq!(UNDEFINED.to_string(), "undefined-macro");
    assert_eq!(Severity::Warning.to_string(), "warning");
}

#[test]
fn a_diagnostic_carries_what_a_renderer_needs() {
    let mut origins = Origins::new();
    let file = origins.add_file("top.sv", "`FOO\n".to_string());
    let at = find(&origins, file, "`FOO");

    let diag = Diagnostic::error(UNDEFINED, at, "`FOO` is not defined")
        .note("the reference stands as written");

    assert!(diag.is_error());
    assert_eq!(diag.code, UNDEFINED);
    assert_eq!(diag.at, at);
    assert_eq!(diag.notes, ["the reference stands as written"]);
    assert!(diag.labels.is_empty());

    // Nothing here resolves a location: turning `at` into a line is the
    // renderer's, and the store is what answers it.
    assert_eq!(origins.line_col(file, diag.at.start).to_string(), "1:1");
}

#[test]
fn a_label_points_somewhere_the_chain_does_not() {
    let mut origins = Origins::new();
    let header = origins.add_file("defs.svh", "`define FOO 1\n".to_string());
    let top = origins.add_file("top.sv", "assign x = `FOO;\n".to_string());

    let call = find(&origins, top, "`FOO");
    let def = find(&origins, header, "`define FOO 1");

    let diag = Diagnostic::warning(Code("redefined"), call, "`FOO` is redefined")
        .label(def, "the previous definition");

    // A label is always secondary: `at` is what the diagnostic is about.
    assert_eq!(diag.labels.len(), 1);
    assert_eq!(diag.labels[0].at.src_id, header);
    assert_eq!(diag.at.src_id, top);
    assert_eq!(origins.slice(diag.labels[0].at), "`define FOO 1");
}

#[test]
fn a_diagnostic_about_an_expanded_token_keeps_both_ends() {
    let mut origins = Origins::new();
    let header = origins.add_file("defs.svh", "`define WIDTH nonexistent_t\n".to_string());
    let top = origins.add_file("top.sv", "logic [`WIDTH-1:0] q;\n".to_string());

    let expansion = origins.expand(Expansion {
        name: find(&origins, top, "`WIDTH"),
        call: find(&origins, top, "`WIDTH"),
        def: Some(find(&origins, header, "`define WIDTH")),
    });
    // The token is spelled in the macro body and used in `top.sv`.
    let at = origins.through(find(&origins, header, "nonexistent_t"), Some(expansion));

    let diag = Diagnostic::error(Code("unknown-type"), at, "`nonexistent_t` names no type");

    // The bytes are in the header; the message belongs at the call, which is
    // the only one of the two the author of `top.sv` can see. Neither is
    // stored twice -- the store derives the second from the first.
    assert_eq!(origins.spelled(diag.at).src_id, header);
    assert_eq!(origins.reported_at(diag.at).src_id, top);
    assert_eq!(origins.slice(origins.reported_at(diag.at)), "`WIDTH");

    // And the chain is the store's to walk, not the diagnostic's to carry.
    let chain: Vec<_> = origins.trace(diag.at.src_id).collect();
    assert_eq!(chain.len(), 1);
    assert_eq!(origins.slice(chain[0].call), "`WIDTH");
}
