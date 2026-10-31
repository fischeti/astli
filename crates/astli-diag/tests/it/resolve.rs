//! Unit tests for diagnostic location resolution.

use astli_diag::{resolve, resolve_all};
use astli_text::{Code, Diagnostic, Expansion, Origins, SourceId, Span};

const CODE: Code = Code("test");

fn find(origins: &Origins, file: SourceId, needle: &str) -> Span {
    let text = origins.text(file);
    let start = text.find(needle).unwrap_or_else(|| panic!("no {needle:?}"));
    Span::new(file, start as u32, (start + needle.len()) as u32)
}

#[test]
fn a_token_written_where_it_is_used_resolves_to_itself() {
    let mut origins = Origins::new();
    let file = origins.add_file("top.sv", "logic q;\n".to_string());
    let at = find(&origins, file, "logic");

    let diag = Diagnostic::error(CODE, at, "wrong");
    let resolved = resolve(&origins, &diag);

    assert_eq!(resolved.at, at);
    assert_eq!(resolved.spelled, None);
    assert!(resolved.through.is_empty());
}

#[test]
fn a_token_a_macro_placed_reports_at_the_call() {
    let mut origins = Origins::new();
    let header = origins.add_file("defs.svh", "`define W nonexistent_t\n".to_string());
    let top = origins.add_file("top.sv", "logic [`W-1:0] q;\n".to_string());

    let call = find(&origins, top, "`W");
    let expansion = origins.expand(Expansion {
        name: call,
        call,
        def: Some(find(&origins, header, "`define W")),
    });
    let spelled = find(&origins, header, "nonexistent_t");
    let at = origins.through(spelled, Some(expansion));
    let diag = Diagnostic::error(CODE, at, "wrong");

    let resolved = resolve(&origins, &diag);
    assert_eq!(resolved.at, call);
    assert_eq!(resolved.spelled, Some(spelled));
    assert_eq!(resolved.through.len(), 1);
    assert_eq!(resolved.through[0].name, "`W");
}

#[test]
fn a_macro_reached_through_a_macro_reads_back_as_a_chain() {
    let mut origins = Origins::new();
    let file = origins.add_file(
        "top.sv",
        "`define INNER x\n`define OUTER `INNER\n`OUTER\n".to_string(),
    );

    let outer_call = find(&origins, file, "`OUTER");
    let inner_call = find(&origins, file, "`INNER");
    let outer = origins.expand(Expansion {
        name: outer_call,
        call: outer_call,
        def: None,
    });
    // `INNER is written in OUTER's body, so OUTER's expansion placed it.
    let placed = origins.through(inner_call, Some(outer));
    let inner = origins.expand(Expansion {
        name: placed,
        call: placed,
        def: None,
    });

    let x = find(&origins, file, "x");
    let diag = Diagnostic::error(CODE, origins.through(x, Some(inner)), "wrong");
    let resolved = resolve(&origins, &diag);

    assert_eq!(resolved.through.len(), 2);
    assert_eq!(resolved.through[0].name, "`INNER");
    assert_eq!(resolved.through[1].name, "`OUTER");
    // Each call is shown where it is written, whatever placed it.
    assert_eq!(resolved.through[0].call, inner_call);
    assert_eq!(resolved.at, outer_call);
}

#[test]
fn a_run_is_ordered_by_place_and_not_by_when_it_was_found() {
    let mut origins = Origins::new();
    let a = origins.add_file("a.sv", "one\ntwo\nthree\n".to_string());
    let b = origins.add_file("b.sv", "four\n".to_string());

    let at = |file, needle| find(&origins, file, needle);
    let diagnostics = vec![
        Diagnostic::error(CODE, at(a, "three"), "third"),
        Diagnostic::error(CODE, at(b, "four"), "fourth"),
        Diagnostic::error(CODE, at(a, "one"), "first"),
    ];

    let order: Vec<_> = resolve_all(&origins, &diagnostics)
        .iter()
        .map(|resolved| resolved.diagnostic.message.as_str())
        .collect();
    assert_eq!(order, ["first", "third", "fourth"]);
}

#[test]
fn the_same_complaint_about_the_same_place_is_said_once() {
    let mut origins = Origins::new();
    let file = origins.add_file("top.sv", "a b\n".to_string());
    let at = find(&origins, file, "a");
    let elsewhere = find(&origins, file, "b");

    let diagnostics = vec![
        Diagnostic::error(CODE, at, "wrong"),
        Diagnostic::error(CODE, at, "wrong"),
        Diagnostic::error(CODE, elsewhere, "wrong"),
    ];
    assert_eq!(resolve_all(&origins, &diagnostics).len(), 2);
}

#[test]
fn an_included_file_carries_the_chain_that_reached_it() {
    let mut origins = Origins::new();
    let top = origins.add_file("top.sv", "`include \"f.svh\"\n".to_string());
    let site = find(&origins, top, "`include \"f.svh\"");
    let f = origins.add_included("f.svh", "logic q;\n".to_string(), site);

    let diag = Diagnostic::error(CODE, find(&origins, f, "logic"), "x");
    let resolved = resolve(&origins, &diag);

    assert_eq!(resolved.included_from, [site]);
}
