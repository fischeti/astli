//! Every `tests/data/<rule>/*.sv`, linted with that rule alone, and what it
//! found compared with the `.lint` beside it: a line per diagnostic, empty
//! when the case is one the rule must accept.
//!
//! A case must parse whole, so that a rule is tested on the tree it reads,
//! except one named `unparsed*.sv`, for a rule that finds what the parser
//! keeps as written.
//!
//! A rule that reads what is built sees the case built from each module
//! nothing in it instantiates.
//!
//! A line `// option: <key> <value>...` in a case sets the rule's option
//! `key` to the values after it, none for an empty list.
//!
//! Where a rule's findings carry a fix, the case with every fix applied is
//! compared with the `.fixed` beside it, and must parse whole and leave
//! nothing more to fix: a fix that does not do away with its finding, or
//! breaks the file, fails here.
//!
//! A case is a file, and the reason it exists is a comment inside it. Adding
//! one means writing the `.sv` and running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run -p astli-lint snapshots
//! ```
//!
//! which writes every `.lint` and `.fixed` that is missing or differs. The
//! diff is the review.

use std::fmt::Write;
use std::path::Path;
use std::sync::Arc;

use astli_lint::{Config, Group, Layer, Level, Linter, RULES, Waivers, lint};
use astli_parse::SyntaxTree;
use astli_parse::parse_expanded;
use astli_preproc::Session;
use astli_sema::{Built, Design, Elaboration, SymbolKind, SymbolRef, lower};
use astli_syntax::SyntaxKind;
use astli_testing::snapshots::{self, Case};
use astli_text::{Diagnostic, Edit, apply};
use expect_test::expect_file;

#[test]
fn snapshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    // A case's rule is the directory it is in, which must name one.
    let mut scan = snapshots::cases(&root, &["lint", "fixed"]);
    let rule_of = |case: &Case| {
        let dir = case.path.parent()?;
        let name = dir.file_name()?.to_string_lossy().into_owned();
        (dir.parent()? == root && RULES.iter().any(|it| it.name == name)).then_some(name)
    };
    let (cases, others): (Vec<_>, Vec<_>) =
        (scan.cases.drain(..)).partition(|it| rule_of(it).is_some());
    scan.strays
        .extend(others.iter().map(|it| it.path.display().to_string()));
    scan.cases = cases;
    scan.run(
        "not a rule's directory, a case, or a case's `.lint`",
        |case| {
            let rule = rule_of(case).expect("a case in a rule's directory");
            let (name, path) = (&case.name, &case.path);
            let unparsed = path
                .file_name()
                .is_some_and(|it| it.to_string_lossy().starts_with("unparsed"));
            let (said, fixed) = found(&rule, name, case.text(), unparsed);
            expect_file![path.with_extension("lint")].assert_eq(&said);
            let fixed_path = path.with_extension("fixed");
            match fixed {
                Some(fixed) => {
                    expect_file![fixed_path].assert_eq(&fixed);
                    let (_, again) = found(&rule, name, fixed, unparsed);
                    assert!(again.is_none(), "{name}: fixed, it still has fixes");
                }
                None => assert!(!fixed_path.exists(), "{name}: no fix, but a `.fixed`"),
            }
        },
    );
}

/// What `rule`, run alone, finds in `text`, which may keep constructs as
/// written when `unparsed` allows it, and the text with every fix it found
/// applied, if it found one.
fn found(rule: &str, name: &str, text: String, unparsed: bool) -> (String, Option<String>) {
    let tree = SyntaxTree::parse(name, text);
    let unparsed = match unparsed {
        true => Vec::new(),
        false => tree.unparsed(),
    };
    assert!(
        tree.diagnostics().is_empty() && unparsed.is_empty(),
        "{name}: not parsed from line {}",
        (tree.diagnostics().iter().chain(&unparsed))
            .map(|it| tree.line_col(it.at.start).line)
            .min()
            .unwrap_or(0)
    );

    let mut config = Config::default();
    for group in Group::ALL {
        config.set(group.name(), Level::Allow).unwrap();
    }
    config.set(rule, Level::Warn).unwrap();
    for line in tree.source().lines() {
        if let Some(option) = line.trim().strip_prefix("// option:") {
            let mut words = option.split_whitespace();
            let key = words.next().expect("an option's key");
            let values: Vec<&str> = words.collect();
            config.set_option(rule, key, &values).unwrap();
        }
    }

    let mut out = String::new();
    let found = match RULES.iter().find(|it| it.name == rule) {
        Some(it) if it.layer() >= Layer::Design => in_design(name, &tree, &config),
        _ => {
            let (waivers, mut found) = Waivers::read(&tree);
            found.extend(lint(&tree, &waivers, &config));
            found.sort_by_key(|it| it.at.start);
            found
        }
    };
    for diagnostic in &found {
        let at = tree.line_col(diagnostic.at.start);
        write!(out, "{at}: [{}] {}", diagnostic.code, diagnostic.message).unwrap();
        match &diagnostic.label {
            Some(caret) => writeln!(out, " ({caret})").unwrap(),
            None => writeln!(out).unwrap(),
        }
    }
    let fixes: Vec<&[Edit]> = (found.iter())
        .map(|it| it.fix.as_slice())
        .filter(|it| !it.is_empty())
        .collect();
    let fixed = (!fixes.is_empty()).then(|| apply(tree.source(), &fixes).0);
    (out, fixed)
}

/// What the design and built rules `config` leaves on find in the case,
/// expanded and lowered as the one file of a design, and waived by its tree
/// as written.
fn in_design(name: &str, tree: &SyntaxTree, config: &Config) -> Vec<Diagnostic> {
    let mut session = Session::new();
    let file = session.add(name, tree.source().to_string());
    let expanded = session.expand(file);
    let parsed = parse_expanded(&session, &expanded.tokens);
    let elaboration = Elaboration::new(Arc::new(Design::new(vec![Arc::new(lower(&parsed))])));
    let design = elaboration.design();
    let (file, hir) = design.files().next().unwrap();
    let instantiated: Vec<&str> = (hir.symbols())
        .filter_map(|(_, symbol)| match &symbol.kind {
            SymbolKind::Instance(instance) => Some(&*instance.definition.text),
            _ => None,
        })
        .collect();
    let tops: Vec<_> = (hir.symbols())
        .filter(|(_, symbol)| {
            matches!(
                symbol.kind,
                SymbolKind::Definition {
                    keyword: SyntaxKind::MODULE_KW,
                    ..
                }
            ) && !instantiated.contains(&&*symbol.name.text)
        })
        .filter_map(|(symbol, _)| elaboration.body(SymbolRef { file, symbol }))
        .collect();
    let linter = Linter::new(&elaboration, Some(&Built::new(&elaboration, &tops)));
    linter.lint(file, session.origins(), &Waivers::read(tree).0, config)
}
