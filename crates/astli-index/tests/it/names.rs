//! The names a tree declares, closes and uses, and its text with them
//! replaced.

use astli_index::{Declares, Role, Uses, names, renamed};
use astli_parse::{parse, parse_expanded};
use astli_preproc::{MacroTable, Session};
use astli_syntax::SyntaxNode;

fn raw(text: &str) -> SyntaxNode {
    let mut session = Session::new();
    let file = session.add("top.sv", text.to_string());
    parse(&session, file, MacroTable::new()).root
}

fn expanded(text: &str) -> SyntaxNode {
    let mut session = Session::new();
    let file = session.add("top.sv", text.to_string());
    let expanded = session.expand(file);
    parse_expanded(&session, &expanded.tokens).root
}

fn roles(root: &SyntaxNode) -> Vec<(String, Role)> {
    names(root)
        .iter()
        .map(|name| (name.text().to_string(), name.role))
        .collect()
}

/// Each name in `names` given the prefix `p_`.
fn prefixed<'a>(names: &'a [&'a str]) -> impl Fn(&str) -> Option<String> + 'a {
    move |name| names.contains(&name).then(|| format!("p_{name}"))
}

#[test]
fn an_end_label_closes_its_declaration() {
    let root = expanded("package cfg;\nendpackage : cfg\nmodule top;\nendmodule : top\n");
    assert_eq!(
        roles(&root),
        [
            ("cfg".into(), Role::Declares(Declares::Package)),
            ("cfg".into(), Role::Closes(Declares::Package)),
            ("top".into(), Role::Declares(Declares::Module)),
            ("top".into(), Role::Closes(Declares::Module)),
        ]
    );
}

#[test]
fn an_end_label_in_unparsed_text_closes_its_declaration() {
    // Two attribute instances in a row leave the module to the fallback.
    let root = expanded("(* a *)\n(* b *)\nmodule m;\nendmodule : m\n");
    assert_eq!(
        roles(&root),
        [
            ("m".into(), Role::Declares(Declares::Module)),
            ("m".into(), Role::Closes(Declares::Module)),
        ]
    );
}

#[test]
fn every_site_of_a_declared_name_is_renamed_and_nothing_else() {
    let root = raw("package cfg;\nendpackage : cfg\n\
         module core (bus_if.slave s);\n\
         \x20 import cfg::*;\n\
         \x20 logic core;\n\
         \x20 assign core = cfg::W;\n\
         endmodule : core\n\
         module top;\n  core u_core ();\n  sub u_sub ();\nendmodule\n\
         bind core checker_m u_chk ();\n");
    assert_eq!(
        renamed(&root, prefixed(&["cfg", "core", "top", "bus_if"])),
        "package p_cfg;\nendpackage : p_cfg\n\
         module p_core (p_bus_if.slave s);\n\
         \x20 import p_cfg::*;\n\
         \x20 logic core;\n\
         \x20 assign core = p_cfg::W;\n\
         endmodule : p_core\n\
         module p_top;\n  p_core u_core ();\n  sub u_sub ();\nendmodule\n\
         bind p_core checker_m u_chk ();\n",
    );
}

#[test]
fn an_escaped_name_stays_escaped() {
    let root = raw("module \\top ;\n  \\core u ();\nendmodule\n");
    assert_eq!(
        renamed(&root, prefixed(&["top", "core"])),
        "module \\p_top ;\n  \\p_core u ();\nendmodule\n"
    );
}

#[test]
fn an_expanded_tree_renames_what_a_macro_wrote() {
    let root = expanded("`define INST(t) t u_``t ();\nmodule top;\n  `INST(core)\nendmodule\n");
    assert_eq!(
        renamed(&root, prefixed(&["core"])),
        "\nmodule top;\n  p_core u_core ();\nendmodule\n"
    );
    assert!(
        names(&root)
            .iter()
            .any(|name| name.text() == "core" && name.role == Role::Uses(Uses::Instance))
    );
}
