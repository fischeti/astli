//! Workspace symbols, asked of designs the support host analyses from small
//! sources once each has loaded.
//!
//! A symbol is written as its name, its kind, the file's name and its
//! range, each `line:character` from 1.

use std::path::Path;

use astli_testing::Fixture;
use lsp_types::{Location, SymbolKind, WorkspaceSymbol, WorkspaceSymbolLocation};
use serde_json::json;

use crate::support::{Client, code, loaded, uri};

/// Asks for the symbols `query` matches, which the server answers.
fn ask(client: &mut Client, query: &str) -> Vec<WorkspaceSymbol> {
    let answer = client.request("workspace/symbol", json!({ "query": query }));
    serde_json::from_value(answer.expect("answered")).expect("workspace symbols")
}

fn written(symbols: &[WorkspaceSymbol]) -> Vec<String> {
    (symbols.iter())
        .map(|symbol| {
            let WorkspaceSymbolLocation::Location(Location { uri, range }) = &symbol.location
            else {
                panic!("a location without a range");
            };
            let path = uri.to_file_path().expect("a file");
            let name = path.file_name().unwrap().to_string_lossy();
            let info = &symbol.base_symbol_information;
            let kind = match info.kind {
                SymbolKind::Module => "module",
                SymbolKind::Interface => "interface",
                SymbolKind::Package => "package",
                SymbolKind::Class => "class",
                kind => panic!("{kind:?}"),
            };
            let (start, end) = (range.start, range.end);
            format!(
                "{} {kind} {name} {}:{}-{}:{}",
                info.name,
                start.line + 1,
                start.character + 1,
                end.line + 1,
                end.character + 1
            )
        })
        .collect()
}

/// A server whose designs read `a.sv`, with `uri` opened holding `text`
/// and loaded.
fn opened(uri: &str, text: &str) -> Client {
    let mut client = Client::analysing(&["a.sv"], false);
    client.open(uri, text);
    client.logged(loaded);
    client
}

#[test]
fn better_matches_come_first() {
    let text = "\
module u_a_r_t; endmodule
module my_uart; endmodule
package uart_pkg; endpackage
interface uart_tx; endinterface
class Uart; endclass
module spi; endmodule
program uart_prog; endprogram
";
    let mut client = opened(&uri("a.sv"), text);
    // Exact, ignoring case, then prefixes, shorter first, then the query
    // inside a name, then its characters apart; `spi` does not match.
    assert_eq!(
        written(&ask(&mut client, "UART")),
        [
            "Uart class a.sv 5:7-5:11",
            "uart_tx interface a.sv 4:11-4:18",
            "uart_pkg package a.sv 3:9-3:17",
            "uart_prog module a.sv 7:9-7:18",
            "my_uart module a.sv 2:8-2:15",
            "u_a_r_t module a.sv 1:8-1:15",
        ]
    );
    client.finish();
}

#[test]
fn an_empty_query_matches_everything() {
    let text = "module b; endmodule\nmodule a; endmodule\npackage ab_pkg; endpackage\n";
    let mut client = opened(&uri("a.sv"), text);
    assert_eq!(
        written(&ask(&mut client, "")),
        [
            "a module a.sv 2:8-2:9",
            "b module a.sv 1:8-1:9",
            "ab_pkg package a.sv 3:9-3:15",
        ]
    );
    client.finish();
}

#[test]
fn an_answer_holds_at_most_so_many() {
    let text: String = (0..200)
        .map(|it| format!("module m{it:03}; endmodule\n"))
        .collect();
    let mut client = opened(&uri("a.sv"), &text);
    let symbols = written(&ask(&mut client, "m"));
    assert_eq!(symbols.len(), 128);
    assert_eq!(symbols[0], "m000 module a.sv 1:8-1:12");
    assert_eq!(symbols[127], "m127 module a.sv 128:8-128:12");
    client.finish();
}

#[test]
fn a_dependency_s_declaration_is_listed() {
    // The dependency is read from the disk, as a file not open is.
    let fixture = Fixture::new("lsp-workspace-symbols-dependency");
    fixture.file("dep/b.sv", "module dep_b; endmodule\n");
    let mut client = Client::analysing(&["a.sv", "../dep/b.sv"], false);
    let opened = fixture.path().join("top/a.sv");
    let uri = lsp_types::Uri::from_file_path(&opened).unwrap().to_string();
    client.open(&uri, "module top_a; dep_b u_b(); endmodule\n");
    client.logged(loaded);
    let symbols = ask(&mut client, "dep");
    assert_eq!(written(&symbols), ["dep_b module b.sv 1:8-1:13"]);
    let WorkspaceSymbolLocation::Location(location) = &symbols[0].location else {
        panic!("a location without a range");
    };
    let path = location.uri.to_file_path().unwrap();
    assert_eq!(path, Path::new(fixture.path()).join("dep/b.sv"));
    client.finish();
}

#[test]
fn a_declaration_two_designs_hold_is_answered_once() {
    // Each design reads `../shared.sv`, as two packages that depend on one
    // do; opened, it is a design of its own too, which reads neither.
    let mut client = Client::analysing(&["a.sv", "../shared.sv"], false);
    client.open(&uri("shared.sv"), "module shared; endmodule\n");
    client.open(&uri("one/a.sv"), "module one; endmodule\n");
    client.open(&uri("two/a.sv"), "module two; endmodule\n");
    // Opening each overtakes the loads of the designs that read it.
    let ready = |client: &mut Client| written(&ask(client, "")).len() == 3;
    while !ready(&mut client) {
        client.logged(loaded);
    }
    assert_eq!(
        written(&ask(&mut client, "")),
        [
            "one module a.sv 1:8-1:11",
            "two module a.sv 1:8-1:11",
            "shared module shared.sv 1:8-1:14",
        ]
    );
    client.finish();
}

#[test]
fn a_range_counts_utf16_code_units() {
    let text = "/* 𝄞é */ module \\café ; endmodule\n";
    let mut client = opened(&uri("a.sv"), text);
    // `𝄞` is two code units and one character, so the name, its backslash
    // with it, starts a code unit after the character it is.
    assert_eq!(
        written(&ask(&mut client, "CAFÉ")),
        ["café module a.sv 1:18-1:23"]
    );
    client.finish();
}

#[test]
fn with_no_design_loaded_the_answer_is_empty() {
    let mut client = Client::analysing(&["a.sv"], false);
    assert!(ask(&mut client, "").is_empty());
    client.finish();
}

#[test]
fn a_cancelled_request_is_answered_as_cancelled() {
    let mut client = opened(&uri("a.sv"), "module a; endmodule\n");
    // Every thread of the pool waits, so that the request is still on it
    // when its cancellation is read.
    let (started, waiting) = crossbeam_channel::unbounded();
    let (release, released) = crossbeam_channel::unbounded::<()>();
    rayon::spawn_broadcast(move |_| {
        started.send(()).unwrap();
        let _ = released.recv();
    });
    for _ in 0..rayon::current_num_threads() {
        waiting.recv().unwrap();
    }
    let id = client.send("workspace/symbol", json!({ "query": "" }));
    client.notify("$/cancelRequest", json!({ "id": id }));
    // Answered on the loop, after the cancellation was read.
    let unknown = client.request("astli/unknown", json!(null));
    assert_eq!(code(unknown), lsp_server::ErrorCode::MethodNotFound as i32);
    drop(release);
    let answer = client.answer(&id);
    assert_eq!(code(answer), lsp_server::ErrorCode::RequestCanceled as i32);
    client.finish();
}
