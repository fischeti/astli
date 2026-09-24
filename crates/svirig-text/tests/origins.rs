//! The origin map, exercised against the shapes expansion hands it.
//!
//! The records here are built by hand rather than by a preprocessor, and stay
//! that way: this crate is meant to stand alone, so what it can express has to
//! be answerable without one. `svirig-syntax`'s own tests cover the same
//! ground with real expansions behind it.

use svirig_text::{Expansion, Origins, Span};

/// A span covering the first occurrence of `needle`, so that the tests read as
/// source rather than as byte arithmetic.
fn find(origins: &Origins, file: svirig_text::SourceId, needle: &str) -> Span {
    let text = origins.text(file);
    let start = text.find(needle).unwrap_or_else(|| panic!("no {needle:?}"));
    Span::new(file, start as u32, (start + needle.len()) as u32)
}

#[test]
fn a_span_carries_its_file() {
    let mut origins = Origins::new();
    let a = origins.add_file("a.sv", "module a; endmodule\n".to_string());
    let b = origins.add_file("b.sv", "module b; endmodule\n".to_string());

    assert_eq!(origins.slice(find(&origins, a, "module a")), "module a");
    assert_eq!(origins.path(a).unwrap().to_str(), Some("a.sv"));

    // Two spans in different files never contain one another and have no
    // common cover -- which is a question with no answer, not an error.
    let (a, b) = (find(&origins, a, "module"), find(&origins, b, "module"));
    assert!(!a.contains(b));
    assert_eq!(a.cover(b), None);
    assert_eq!(a.cover(a), Some(a));
}

#[test]
fn line_and_column_count_from_one() {
    let mut origins = Origins::new();
    let file = origins.add_file("f.sv", "module m;\r\n  logic x;\nendmodule\n".to_string());

    let at = |needle| origins.line_col(file, find(&origins, file, needle).start);
    assert_eq!(at("module m").to_string(), "1:1");
    assert_eq!(at("logic").to_string(), "2:3");
    assert_eq!(at("endmodule").to_string(), "3:1");

    // The column counts characters, so a comment in another script does not
    // push the caret sideways.
    let file = origins.add_file("u.sv", "// ✓ ok\nlogic x;\n".to_string());
    assert_eq!(origins.line_col(file, 0).to_string(), "1:1");
    assert_eq!(
        origins.line_col(file, find(&origins, file, "ok").start),
        svirig_text::LineCol { line: 1, col: 6 }
    );
}

#[test]
fn an_included_file_remembers_what_pulled_it_in() {
    let mut origins = Origins::new();
    let top = origins.add_file("top.sv", "`include \"a.svh\"\n".to_string());
    let a = origins.add_included(
        "a.svh",
        "`include \"b.svh\"\n".to_string(),
        find(&origins, top, "`include \"a.svh\""),
    );
    let b = origins.add_included(
        "b.svh",
        "`define W 8\n".to_string(),
        find(&origins, a, "`include \"b.svh\""),
    );

    assert_eq!(origins.included_from(top), None);
    let chain: Vec<_> = origins
        .include_trace(b)
        .map(|span| {
            format!(
                "{}:{}",
                origins.path(span.file).unwrap().display(),
                origins.line_col(span.file, span.start)
            )
        })
        .collect();
    assert_eq!(chain, ["a.svh:1:1", "top.sv:1:1"]);
}

#[test]
fn an_argument_and_a_body_token_share_one_expansion() {
    // The case a byte-oriented map has to swap roles for. `f` is written in the
    // body, in one file; `p` is written in the argument, in another. Both are
    // placed by the same call, and here that needs no special mechanism -- the
    // two tokens just have different spellings seen through one expansion.
    let mut origins = Origins::new();
    let head = origins.add_file("m.svh", "`define M(x) f(x)\n".to_string());
    let top = origins.add_file("top.sv", "assign y = `M(p + q);\n".to_string());

    let call = find(&origins, top, "`M(p + q)");
    let expansion = origins.expand(Expansion {
        name: find(&origins, top, "`M"),
        call,
        def: Some(find(&origins, head, "`define M(x) f(x)")),
    });

    let body_token = origins.through(find(&origins, head, "f"), Some(expansion));
    let arg_token = origins.through(find(&origins, top, "p"), Some(expansion));

    assert_eq!(origins.slice(body_token), "f");
    assert_eq!(origins.slice(arg_token), "p");
    assert_eq!(origins.spelled(body_token), find(&origins, head, "f"));
    assert_eq!(origins.spelled(arg_token), find(&origins, top, "p"));
    assert_eq!(origins.placed_by(body_token.file), Some(expansion));
    assert_eq!(origins.placed_by(arg_token.file), Some(expansion));
    // And a message about either points at the call the author wrote.
    assert_eq!(origins.reported_at(body_token), call);
    assert_eq!(origins.reported_at(arg_token), call);
}

#[test]
fn a_macro_that_expands_a_macro_reads_back_as_a_chain() {
    let mut origins = Origins::new();
    let head = origins.add_file(
        "m.svh",
        "`define ASSERT(c, m) if (!(c)) $error(m)\n`define CHECK(c) `ASSERT(c, \"failed\")\n"
            .to_string(),
    );
    let top = origins.add_file("top.sv", "initial `CHECK(x > 0);\n".to_string());

    let outer = origins.expand(Expansion {
        name: find(&origins, top, "`CHECK"),
        call: find(&origins, top, "`CHECK(x > 0)"),
        def: Some(find(&origins, head, "`define CHECK")),
    });
    // The inner call is written inside the outer macro's body, so it is
    // placed by the outer expansion like any token of that body.
    // The first `` `ASSERT `` in the file is the one in CHECK's body; the
    // definition above spells it without a backtick.
    let name = origins.through(find(&origins, head, "`ASSERT"), Some(outer));
    let call = find(&origins, head, "`ASSERT(c, \"failed\")");
    let call = origins.through(call, Some(outer));
    let inner = origins.expand(Expansion {
        name,
        call,
        def: Some(find(&origins, head, "`define ASSERT")),
    });

    let token = origins.through(find(&origins, head, "$error"), Some(inner));

    // Innermost first, and it is exactly the sentence the design asked for.
    let notes: Vec<_> = origins
        .trace(token.file)
        .map(|expansion| {
            let def = expansion.def.expect("both of these have a `define");
            format!(
                "{} expanded at {}, defined at {}",
                origins.slice(expansion.name),
                origins.line_col(expansion.call.file, expansion.call.start),
                origins.line_col(def.file, def.start),
            )
        })
        .collect();
    assert_eq!(
        notes,
        [
            "`ASSERT expanded at 2:18, defined at 1:1",
            "`CHECK expanded at 1:9, defined at 2:1",
        ]
    );
    // The reader wrote `CHECK, so that is where the caret goes.
    assert_eq!(
        origins.reported_at(token),
        find(&origins, top, "`CHECK(x > 0)")
    );
}

#[test]
fn pasted_text_is_a_buffer_with_no_path() {
    // ``` `` ``` makes a name that is in no file, so it needs somewhere to live
    // that a span can still address.
    let mut origins = Origins::new();
    let head = origins.add_file("m.svh", "`define PORT(p) p``_valid\n".to_string());
    let top = origins.add_file("top.sv", "wire `PORT(rx);\n".to_string());

    let expansion = origins.expand(Expansion {
        name: find(&origins, top, "`PORT"),
        call: find(&origins, top, "`PORT(rx)"),
        def: Some(find(&origins, head, "`define PORT")),
    });
    let pasted = origins.add_synthesised("rx_valid".to_string());
    let token = origins.through(Span::new(pasted, 0, 8), Some(expansion));

    assert_eq!(origins.slice(token), "rx_valid");
    assert_eq!(origins.path(token.file), None);
    // It still reports where the author can see it.
    assert_eq!(origins.reported_at(token), find(&origins, top, "`PORT(rx)"));
}

#[test]
fn a_token_written_where_it_is_used_traces_to_nothing() {
    let mut origins = Origins::new();
    let file = origins.add_file("f.sv", "logic x;\n".to_string());
    let token = find(&origins, file, "logic");

    assert_eq!(origins.trace(token.file).count(), 0);
    assert_eq!(origins.reported_at(token), token);
    assert_eq!(origins.spelled(token), token);
}

#[test]
fn one_expansion_sees_a_buffer_through_one_view() {
    let mut origins = Origins::new();
    let head = origins.add_file("m.svh", "`define M a b\n".to_string());
    let top = origins.add_file("top.sv", "`M `M\n".to_string());

    let expand = |origins: &mut Origins, at| {
        origins.expand(Expansion {
            name: Span::new(top, at, at + 2),
            call: Span::new(top, at, at + 2),
            def: Some(find(origins, head, "`define M a b")),
        })
    };
    let first = expand(&mut origins, 0);
    let second = expand(&mut origins, 3);

    let a = origins.through(find(&origins, head, "a"), Some(first));
    let b = origins.through(find(&origins, head, "b"), Some(first));
    let again = origins.through(find(&origins, head, "a"), Some(second));

    // Tokens of one body share a view, so spans within it still cover.
    assert_eq!(a.file, b.file);
    assert_eq!(a.cover(b).map(|span| origins.slice(span)), Some("a b"));
    // The same bytes placed by another call are another use of them.
    assert_ne!(a, again);
    assert_eq!(origins.spelled(a), origins.spelled(again));
    assert_eq!(origins.slice(origins.reported_at(again)), "`M");
    assert_eq!(origins.reported_at(again).start, 3);
}
