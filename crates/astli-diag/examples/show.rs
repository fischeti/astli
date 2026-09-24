//! Example demonstrating diagnostic rendering across various preprocessor scenarios.

use astli_diag::{Sources, Style, resolve_all, write};
use astli_preproc::Session;

fn show(title: &str, source: &str) {
    println!("\n======== {title} ========");
    let mut session = Session::new();
    let file = session.add("top.sv", source.to_string());
    let expanded = session.expand(file);

    let resolved = resolve_all(session.origins(), &expanded.diagnostics);
    let mut sources = Sources::new(session.origins());
    let mut out = Vec::new();
    for diagnostic in &resolved {
        write(&mut out, &mut sources, diagnostic, Style::plain()).unwrap();
    }
    print!("{}", String::from_utf8_lossy(&out));
}

fn main() {
    show(
        "undefined",
        "module top;\n  logic [`WIDTH-1:0] q;\nendmodule\n",
    );
    // Nested macro expansion where the error occurs within an inner macro definition.
    show(
        "through two macros",
        "`define INNER `MISSING\n`define OUTER `INNER\nassign x = `OUTER;\n",
    );
    show(
        "a call with no argument list",
        "`define M(x) f(x)\nassign y = `M;\n",
    );
    show("a stray closer", "module top;\n`endif\nendmodule\n");
}
