//! The design rules over a design of several files, where what one file
//! holds decides what is reported in another, which a case of the snapshots
//! cannot show, being a design of one file.

use std::sync::Arc;

use astli_lint::{Config, Group, Level, Linter, Waivers};
use astli_parse::{SyntaxTree, parse_expanded};
use astli_preproc::Session;
use astli_sema::{Design, Elaboration, lower};

/// What `rule`, run alone, finds in the first of `files`, each a name and
/// its text, linted as a design of them all.
fn found(rule: &str, files: &[(&str, &str)]) -> Vec<String> {
    let mut session = Session::new();
    let ids: Vec<_> = (files.iter())
        .map(|(name, text)| session.add(name, text.to_string()))
        .collect();
    let hirs = (ids.iter())
        .map(|&file| {
            let expanded = session.expand(file);
            Arc::new(lower(&parse_expanded(&session, &expanded.tokens)))
        })
        .collect();
    let elaboration = Elaboration::new(Arc::new(Design::new(hirs)));
    let mut config = Config::default();
    for group in Group::ALL {
        config.set(group.name(), Level::Allow).unwrap();
    }
    config.set(rule, Level::Warn).unwrap();
    let (name, text) = files[0];
    let tree = SyntaxTree::parse(name, text.to_string());
    let (file, _) = elaboration.design().files().next().unwrap();
    let linter = Linter::new(&elaboration, None);
    let found = linter.lint(file, session.origins(), &Waivers::read(&tree).0, &config);
    found.into_iter().map(|it| it.message).collect()
}

/// A signal another file spells in a region sema does not see into is not
/// reported unused, since that region may read it through a path that
/// names it: an opaque member, statement, expression or type.
#[test]
fn signal_another_file_spells_opaquely_is_not_unused() {
    let signal = ("m.sv", "module m;\n  logic sig;\nendmodule\n");
    let found = |item: &str| {
        let other = format!("module top;\n  m u ();\n  {item}\nendmodule\n");
        found("unused-signal", &[signal, ("top.sv", &other)])
    };
    assert_eq!(found(""), ["signal `sig` is never read"]);
    for item in [
        "`T(u.sig)",
        "initial `T(u.sig);",
        "initial $display(`T(u.sig));",
        "typedef `T(u.sig) t;",
    ] {
        assert_eq!(found(item), Vec::<String>::new(), "{item}");
    }
}
