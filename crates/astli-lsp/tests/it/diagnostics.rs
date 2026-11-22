//! What the server publishes of each open file: what its buffer says at
//! once, and what its design says once a snapshot is swapped in.

use serde_json::{Value, json};

use crate::support::{Client, Published, loaded, path, uri};

/// The capabilities of a client that takes related information.
fn related() -> Value {
    json!({ "textDocument": { "publishDiagnostics": { "relatedInformation": true } } })
}

/// The line `diagnostic` starts on, counted from zero.
fn line(diagnostic: &Value) -> u64 {
    diagnostic["range"]["start"]["line"]
        .as_u64()
        .expect("a line")
}

/// Whether `published` is of the buffer at `version`.
fn at(version: i64) -> impl Fn(&Published) -> bool {
    move |it| it.version == Some(version)
}

#[test]
fn a_buffer_is_published_on_open_and_on_each_change() {
    let mut client = Client::start(&["a.sv"], false);
    let uri = &uri("a.sv");
    client.open(uri, "module a;\n");
    let opened = client.published(uri, at(1));
    let [malformed] = &opened.diagnostics[..] else {
        panic!("{:#?}", opened.diagnostics);
    };
    assert_eq!(malformed["code"], "unclosed-at-end-of-file");
    assert_eq!(malformed["source"], "astli");
    assert_eq!(malformed["severity"], 1);
    assert_eq!(line(malformed), 0);

    client.replace(uri, "module a;\nendmodule\n");
    let changed = client.published(uri, at(2));
    assert!(changed.diagnostics.is_empty(), "{changed:#?}");
    client.finish();
}

#[test]
fn a_snapshot_publishes_what_the_design_says_and_a_close_clears_it() {
    let mut client = Client::designing(&["a.sv"], related());
    let uri = &uri("a.sv");
    let text = "module a;\n  reg r;\n  wire p = nowhere;\nendmodule\n";
    client.open(uri, text);
    // What the buffer says on its own comes first, and the tree rules once
    // the snapshot gives the file its levels, with the design's findings.
    let opened = client.published(uri, at(1));
    assert!(!opened.codes().contains(&"undeclared-name"), "{opened:#?}");
    let designed = client.published(uri, |it| it.codes().contains(&"undeclared-name"));
    assert_eq!(designed.version, Some(1));
    assert_eq!(line(designed.with("undeclared-name")), 2);
    assert_eq!(line(designed.with("unused-signal")), 1);
    let reg = designed.with("forbid-reg");
    let href = "https://astli.dev/reference/lint-rules/#lowrisc";
    assert_eq!(reg["codeDescription"]["href"], href);
    assert_eq!(line(reg), 1);
    client.logged(loaded);
    let checked = client.logged(|it| it.starts_with("checked "));
    assert!(checked.contains(": 1 units, in "), "{checked}");

    client.close(uri);
    let closed = client.published(uri, |it| it.version.is_none());
    assert!(closed.diagnostics.is_empty(), "{closed:#?}");
    client.finish();
}

#[test]
fn a_header_two_units_include_is_published_once() {
    let mut client = Client::designing(&["a.sv", "b.sv", "h.svh"], related());
    let header = &uri("h.svh");
    client.open(header, "wire w = nowhere;\n");
    let unit = |name: &str| format!("module {name};\n  `include \"h.svh\"\nendmodule\n");
    client.open(&uri("a.sv"), &unit("a"));
    client.open(&uri("b.sv"), &unit("b"));
    let both = |it: &Published| {
        let found = it
            .diagnostics
            .iter()
            .find(|it| it["code"] == "undeclared-name");
        let included = found.and_then(|it| it["relatedInformation"].as_array());
        included.is_some_and(|it| !it.is_empty())
    };
    let published = client.published(header, both);
    let undeclared = published
        .codes()
        .iter()
        .filter(|it| **it == "undeclared-name")
        .count();
    assert_eq!(undeclared, 1, "{published:#?}");
    let related = &published.with("undeclared-name")["relatedInformation"][0];
    assert_eq!(related["message"], "included from here");
    client.finish();
}

#[test]
fn a_design_read_without_its_dependencies_publishes_only_its_problems_and_the_buffer() {
    // A client that takes no related information.
    let mut client = Client::designing(&["a.sv"], json!({}));
    let uri = &uri("a.sv");
    let text = "// unfetched\nmodule a;\n  wire p = nowhere;\n";
    client.open(uri, text);
    let manifest = &crate::support::uri("astli.toml");
    let problem = client.published(manifest, |it| !it.diagnostics.is_empty());
    assert_eq!(problem.version, None);
    let unfetched = problem.with("unfetched");
    // The manifest does not exist, so the problem is at its start; its
    // label is a line of the message.
    assert_eq!(
        unfetched["range"]["end"],
        json!({ "line": 0, "character": 0 })
    );
    let at = path("astli.toml");
    let message = format!(
        "`ip` is not fetched\nnote: the package is read without its dependencies\n\
         help: `astli fetch` fetches it\n{}:1:1: required here",
        at.display()
    );
    assert_eq!(unfetched["message"], message);

    client.logged(loaded);
    let last = client.latest(uri).expect("published on open");
    assert!(
        last.codes().contains(&"unclosed-at-end-of-file"),
        "{last:#?}"
    );
    assert!(!last.codes().contains(&"undeclared-name"), "{last:#?}");

    // Once the dependencies read, the design's findings come back, and the
    // manifest's problem goes.
    client.replace(uri, "module a;\n  wire p = nowhere;\nendmodule\n");
    client.published(uri, |it| it.codes().contains(&"undeclared-name"));
    client.published(manifest, |it| it.diagnostics.is_empty());
    // A design dropped, its last file closed, takes its problems with it.
    client.change(uri, 3, json!([{ "text": text }]));
    client.published(manifest, |it| !it.diagnostics.is_empty());
    client.close(uri);
    client.published(manifest, |it| it.diagnostics.is_empty());
    client.finish();
}

#[test]
fn a_change_keeps_the_design_findings_that_end_before_it() {
    let mut client = Client::designing(&["a.sv"], json!({}));
    let uri = &uri("a.sv");
    let text = "module a;\n  wire p = nowhere;\n  wire w;\n  wire q = nowhere;\nendmodule\n";
    client.open(uri, text);
    let undeclared = |it: &Published| -> Vec<u64> {
        (it.diagnostics.iter())
            .filter(|it| it["code"] == "undeclared-name")
            .map(line)
            .collect()
    };
    client.published(uri, |it| undeclared(it) == [1, 3]);

    // A line inserted between the two: the first is where it was, the
    // second may not be, until the design is loaded again.
    client.change(uri, 3, json!([inserted(2, "  wire v;\n")]));
    let changed = client.published(uri, at(3));
    assert_eq!(undeclared(&changed), [1]);
    let again = client.published(uri, |it| undeclared(it).len() == 2);
    assert_eq!((again.version, undeclared(&again)), (Some(3), vec![1, 4]));

    // Of several changes, each read in the text the one before it left,
    // the earliest start is where the text first differs, neither the
    // first's nor the last's.
    let changes = [
        inserted(5, "  wire x;\n"),
        inserted(2, "  wire y;\n"),
        inserted(8, "  wire z;\n"),
    ];
    client.change(uri, 4, json!(changes));
    let changed = client.published(uri, at(4));
    assert_eq!(undeclared(&changed), [1]);
    let again = client.published(uri, |it| undeclared(it).len() == 2);
    assert_eq!((again.version, undeclared(&again)), (Some(4), vec![1, 5]));
    client.finish();
}

#[test]
fn a_change_drops_a_design_finding_a_related_place_of_which_it_follows() {
    let mut client = Client::designing(&["a.sv"], related());
    let uri = &uri("a.sv");
    client.open(uri, "module a;\n  initial x = 1;\n  logic x;\nendmodule\n");
    let found = client.published(uri, |it| it.codes().contains(&"used-before-declaration"));
    let related = &found.with("used-before-declaration")["relatedInformation"][0];
    assert_eq!(related["message"], "declared here");
    // The use ends before the change, but the declaration it relates does
    // not, and may no longer be where the finding says.
    client.change(uri, 2, json!([inserted(2, "  // x\n")]));
    let changed = client.published(uri, at(2));
    let codes = changed.codes();
    assert!(!codes.contains(&"used-before-declaration"), "{changed:#?}");
    client.finish();
}

/// A change that inserts `text` at the start of `line`, counted from zero.
fn inserted(line: u32, text: &str) -> Value {
    let start = json!({ "line": line, "character": 0 });
    json!({ "range": { "start": start, "end": start }, "text": text })
}

#[test]
fn a_problem_in_no_file_is_shown_when_a_load_first_finds_it() {
    let mut client = Client::designing(&["a.sv"], json!({}));
    let uri = &uri("a.sv");
    client.open(uri, "// placeless\nmodule a;\nendmodule\n");
    client.logged(loaded);
    client.replace(uri, "// placeless\nmodule b;\nendmodule\n");
    client.logged(loaded);
    let _ = client.request("astli/unknown", Value::Null);
    assert_eq!(client.shown, ["a problem in no file"]);
    client.finish();
}

#[test]
fn a_design_finding_is_placed_in_the_negotiated_encoding() {
    // `𝄞` is four bytes and two UTF-16 code units, `é` two bytes and one.
    let text = "module a;\n  wire p = /* \u{1D11E}\u{e9} */ nowhere;\nendmodule\n";
    for (offered, character) in [(json!(["utf-8"]), 24), (json!([]), 21)] {
        let capabilities = json!({ "general": { "positionEncodings": offered } });
        let mut client = Client::designing(&["a.sv"], capabilities);
        let uri = &uri("a.sv");
        client.open(uri, text);
        let found = client.published(uri, |it| it.codes().contains(&"undeclared-name"));
        let start = &found.with("undeclared-name")["range"]["start"];
        assert_eq!(*start, json!({ "line": 1, "character": character }));
        client.finish();
    }
}

#[test]
fn a_file_is_published_under_the_uri_the_client_opened_it_with() {
    let mut client = Client::designing(&["a+b.sv"], related());
    // `a+b.sv`, its `+` escaped, as some clients escape every character a
    // URI reserves, though a path needs no `+` escaped.
    let plain = uri("a+b.sv");
    let spelled = &format!("{}a%2Bb.sv", &plain[..plain.len() - "a+b.sv".len()]);
    let text =
        "`define READ(x) (x + nowhere)\nmodule a;\n  wire w;\n  wire p = `READ(w);\nendmodule\n";
    client.open(spelled, text);
    let found = client.published(spelled, |it| it.codes().contains(&"undeclared-name"));
    // A related place in the file names it as the client does too.
    let related = &found.with("undeclared-name")["relatedInformation"][0];
    assert_eq!(related["message"], "this is the text it stands for");
    assert_eq!(related["location"]["uri"], spelled.as_str());
    client.close(spelled);
    let closed = client.published(spelled, |it| it.version.is_none());
    assert!(closed.diagnostics.is_empty(), "{closed:#?}");
    assert!(
        client.published.iter().all(|it| it.uri != plain),
        "{:#?}",
        client.published
    );
    client.finish();
}

#[test]
fn a_problem_placed_in_text_the_buffer_no_longer_holds_is_dropped() {
    // The design at `one/` reads `shared.sv`, which is open in a design of
    // its own, and places a problem there.
    let mut client = Client::designing(&["a.sv", "../shared.sv"], json!({}));
    let shared = &uri("shared.sv");
    client.open(shared, "module shared;\n// twice\nendmodule\n");
    client.logged(loaded);
    client.open(&uri("one/a.sv"), "// hold\nmodule a;\nendmodule\n");
    client.held();
    // An edit to `shared.sv` before the load reads it, which no load of
    // `one/` has read yet: it overtakes nothing, and the load reads the
    // text as it was when it started.
    client.change(shared, 2, json!([inserted(0, "\n")]));
    let _ = client.latest(shared);
    client.release();
    // So what it placed there is not taken, and the design is loaded
    // again, reading the text as it is.
    client.held();
    client.release();
    let found = client.published(shared, |it| it.codes().contains(&"declared-twice"));
    let twice = found.with("declared-twice");
    assert_eq!((found.version, line(twice)), (Some(2), 2));
    client.finish();
}

/// Each branch body `published` says the build left out, as its range,
/// each position `line:character` from 1, and its message.
fn faded(published: &Published) -> Vec<String> {
    let at = |it: &Value| {
        format!(
            "{}:{}",
            it["line"].as_u64().unwrap() + 1,
            it["character"].as_u64().unwrap() + 1
        )
    };
    (published.diagnostics.iter())
        .filter(|it| it["code"] == "inactive-code")
        .map(|it| {
            // A hint the client fades, which no problems list shows.
            assert_eq!((&it["severity"], &it["tags"]), (&json!(4), &json!([1])));
            let range = &it["range"];
            let message = it["message"].as_str().unwrap();
            format!("{}-{} {message}", at(&range["start"]), at(&range["end"]))
        })
        .collect()
}

#[test]
fn a_branch_the_build_did_not_take_is_faded() {
    let mut client = Client::designing(&["a.sv"], json!({}));
    let uri = &uri("a.sv");
    let text = "\
module a;
`ifdef MISSING
  wire w1;
`else
  wire w2;
`endif
`ifndef MISSING
  wire w3;
`elsif OTHER
  wire w4;
`else
  wire w5;
  wire w6;
`endif
`define OUTER
`ifdef OUTER
  `ifdef INNER
    wire w7;
  `endif
`else
  `ifdef INNER
    wire w8;
  `endif
`endif
`ifdef MISSING `endif
endmodule
";
    client.open(uri, text);
    let found = client.published(uri, |it| !faded(it).is_empty());
    assert_eq!(
        faded(&found),
        [
            "3:1-3:11 not built: `MISSING` is not defined",
            "10:1-10:11 not built under this build's defines",
            "12:1-13:11 not built: a branch before it is taken",
            "18:1-18:13 not built: `INNER` is not defined",
            "21:1-23:9 not built: a branch before it is taken",
        ]
    );
    // It is no finding with a fix.
    let hint = found.with("inactive-code").clone();
    let params = json!({
        "textDocument": { "uri": uri },
        "range": hint["range"],
        "context": { "diagnostics": [hint] },
    });
    let actions = client.request("textDocument/codeAction", params);
    assert_eq!(actions.expect("answered"), json!([]));
    client.finish();
}

#[test]
fn a_branch_the_package_build_defines_is_taken() {
    let mut client = Client::designing(&["a.sv"], json!({}));
    let uri = &uri("a.sv");
    let text =
        "// BUILT\nmodule a;\n`ifdef BUILT\n  wire on;\n`else\n  wire off;\n`endif\nendmodule\n";
    client.open(uri, text);
    let found = client.published(uri, |it| !faded(it).is_empty());
    assert_eq!(
        faded(&found),
        ["6:1-6:12 not built: a branch before it is taken"]
    );
    client.finish();
}

#[test]
fn a_header_branch_is_faded_only_where_every_unit_left_it_out() {
    let mut client = Client::designing(&["a.sv", "b.sv", "h.svh"], json!({}));
    let header = &uri("h.svh");
    let text = "\
`ifdef WIDE
wire [63:0] bus;
`else
wire [31:0] bus;
`endif
`ifdef NEVER
wire never;
`endif
";
    client.open(header, text);
    let unit = |name: &str| format!("module {name};\n  `include \"h.svh\"\nendmodule\n");
    client.open(&uri("a.sv"), &format!("`define WIDE\n{}", unit("a")));
    client.open(&uri("b.sv"), &unit("b"));
    client.logged(loaded);
    let last = client.latest(header).expect("published");
    // `a` takes the first branch and `b` the second, so neither is faded.
    assert_eq!(faded(&last), ["7:1-7:12 not built: `NEVER` is not defined"]);
    client.finish();
}

#[test]
fn a_panic_finding_what_the_design_says_keeps_the_snapshot() {
    let mut client = Client::designing(&["a.sv"], json!({}));
    let uri = &uri("a.sv");
    client.open(
        uri,
        "// unsound twice\nmodule a;\n  wire p = nowhere;\nendmodule\n",
    );
    client.logged(loaded);
    let said = client.logged(|it| it.contains("panicked"));
    assert!(said.starts_with("finding what the design at "), "{said}");
    // Its problems are still placed, and the design says nothing.
    let found = client.published(uri, |it| it.codes().contains(&"declared-twice"));
    assert!(!found.codes().contains(&"undeclared-name"), "{found:#?}");
    // And the next load is swapped in too, rather than lost the same way.
    client.replace(uri, "// unsound twice\nmodule b;\nendmodule\n");
    client.logged(loaded);
    client.finish();
}
