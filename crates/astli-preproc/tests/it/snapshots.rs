//! Every `tests/data/expansions/*.sv`, expanded, with what it records for a
//! reader starting from the text: each call the file writes, the
//! `` `define `` it read and what it wrote, and each branch not taken.
//! Compared with the `.expansions` beside it.
//!
//! A case may include the `.svh` of its own name, which is how a header's
//! calls and branches are reached. Adding a case means writing the `.sv` and
//! running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run -p astli-preproc snapshots
//! ```

use std::fmt::Write as _;
use std::path::Path;

use astli_preproc::{Build, Session, render};
use astli_testing::snapshots;
use astli_text::{Origins, Span};

#[test]
fn snapshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/expansions");
    let cases = snapshots::cases(&root, &["expansions", "svh"]);
    cases.run("not a case, its header or its `.expansions`", |case| {
        let actual = dump(&case.path);
        expect_test::expect_file![case.path.with_extension("expansions")].assert_eq(&actual);
    });
}

fn dump(path: &Path) -> String {
    let dir = path.parent().expect("a case is in a directory");
    let mut session = Session::new().building(Build::new().include_dir(dir));
    let file = session.open(path).expect("a case is readable");
    let expanded = session.expand(file);
    let origins = session.origins();

    let mut out = String::new();
    for &id in &expanded.calls {
        let expansion = origins.expansion(id);
        let call = expansion.call;
        // The call is what a cursor anywhere on it finds.
        for offset in [call.start, call.end - 1] {
            let found = expanded.call_at(origins, call.src_id, offset);
            assert_eq!(found, Some(id), "{} at {offset}", place(origins, call));
        }
        let wrote = collapsed(&render(origins, &expanded.written_by(origins, id)));
        let def = match expansion.def {
            Some(def) => place(origins, def),
            None => "built in".to_string(),
        };
        writeln!(
            out,
            "{} {}",
            place(origins, call),
            collapsed(origins.slice(call))
        )
        .unwrap();
        writeln!(out, "  defined {def}").unwrap();
        writeln!(out, "  wrote {wrote}").unwrap();
    }
    for &span in &expanded.skipped {
        let text = collapsed(origins.slice(span));
        writeln!(
            out,
            "skipped {} to {}: {text}",
            place(origins, span),
            end(origins, span)
        )
        .unwrap();
    }
    for diagnostic in &expanded.diagnostics {
        let at = origins.reported_at(diagnostic.at);
        writeln!(out, "{} {}", diagnostic.code.as_str(), place(origins, at)).unwrap();
    }
    out
}

/// Where `span` starts, as its file's name and a line and column, and for a
/// header the line of the `` `include `` that read it.
fn place(origins: &Origins, span: Span) -> String {
    let name = origins.path(span.src_id).and_then(Path::file_name);
    let name = name.map_or("?".into(), |name| name.to_string_lossy());
    let at = origins.line_col(span.src_id, span.start);
    let site = origins.included_from(span.src_id).map(|site| {
        let at = origins.line_col(site.src_id, site.start);
        format!(" (included at {})", at.line)
    });
    format!("{name}:{}:{}{}", at.line, at.col, site.unwrap_or_default())
}

fn end(origins: &Origins, span: Span) -> String {
    let at = origins.line_col(span.src_id, span.end);
    format!("{}:{}", at.line, at.col)
}

fn collapsed(text: &str) -> String {
    let words: Vec<_> = text.split_whitespace().collect();
    match words.is_empty() {
        true => "(nothing)".to_string(),
        false => words.join(" "),
    }
}
