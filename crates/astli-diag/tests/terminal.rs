//! Integration tests for terminal diagnostic formatting.

use astli_diag::{Sources, Style, resolve, write};
use astli_preproc::Session;

/// Expands `source` and renders every diagnostic it produced.
fn render(source: &str) -> String {
    let mut session = Session::new();
    let file = session.add("top.sv", source.to_string());
    let expanded = session.expand(file);
    assert!(
        !expanded.diagnostics.is_empty(),
        "nothing to render; the test means to produce a diagnostic"
    );

    let mut sources = Sources::new(session.origins());
    let mut out = Vec::new();
    for diagnostic in &expanded.diagnostics {
        let resolved = resolve(session.origins(), diagnostic);
        write(&mut out, &mut sources, &resolved, Style::plain()).unwrap();
    }
    String::from_utf8(out).expect("the renderer writes utf-8")
}

#[test]
fn a_report_carries_the_code_the_place_and_a_caret() {
    let out = render("module top;\n  logic [`WIDTH-1:0] q;\nendmodule\n");

    assert!(out.contains("[undefined-macro]"), "{out}");
    assert!(out.contains("`WIDTH is not defined"), "{out}");
    // Column 10 is where the backtick is, counting from one.
    assert!(out.contains("top.sv:2:10"), "{out}");
    // The caret is drawn, which it is not when a label has nothing to say.
    assert!(out.contains('^'), "{out}");
    assert!(out.contains("not defined here"), "{out}");
    assert!(out.contains("the reference stands as written"), "{out}");
}

#[test]
fn a_name_is_quoted_once_however_it_was_written() {
    let out = render("logic [`WIDTH-1:0] q;\n");
    assert!(out.contains("`WIDTH is not defined"), "{out}");
    assert!(!out.contains("``WIDTH"), "{out}");
}

#[test]
fn a_multi_byte_character_above_does_not_move_the_caret() {
    let out = render("// Copyright © 2026 — a header with non-ASCII in it\nlogic [`W-1:0] q;\n");

    assert!(out.contains("top.sv:2:8"), "{out}");
    assert!(out.contains("logic [`W-1:0] q;"), "{out}");
    assert!(
        !out.contains("Copyright"),
        "the caret is on the wrong line: {out}"
    );
}

#[test]
fn a_multi_byte_character_on_the_line_itself_is_counted_as_one_column() {
    let out = render("logic q; // ← twelve characters before this\nlogic [`W-1:0] r;\n");
    assert!(out.contains("top.sv:2:8"), "{out}");
}

#[test]
fn a_complaint_from_inside_a_macro_shows_the_call_and_the_body() {
    let out = render("`define INNER `MISSING\n`define OUTER `INNER\nassign x = `OUTER;\n");

    assert!(out.contains("top.sv:3:12"), "{out}");
    assert!(out.contains("this is the text it stands for"), "{out}");
    assert!(out.contains("in this expansion of `INNER"), "{out}");
    assert!(!out.contains("``INNER"), "{out}");
}

#[test]
fn plain_style_writes_no_escapes() {
    let out = render("logic [`W-1:0] q;\n");
    assert!(!out.contains('\u{1b}'), "an escape survived --color=false");
    assert!(
        out.is_ascii(),
        "ascii char set should not draw box characters"
    );
}
