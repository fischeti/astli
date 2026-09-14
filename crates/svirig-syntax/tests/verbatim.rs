//! What the fallback takes, and where it stops.

use rowan::NodeOrToken;
use svirig_syntax::parser::{Context, Parser, Raw, build, parse, verbatim};
use svirig_syntax::preproc::Input;
use svirig_syntax::{SyntaxKind::*, SyntaxNode, Token, tokenize};
use svirig_text::{FileId, Origins};

mod corpus;

/// The rate the milestone is graded on: how much of a file the parser still
/// cannot make sense of.
///
/// **Lower this as rules land.** It may never rise: a rate that goes up is a
/// regression even when every other test passes, which is the whole reason it
/// is asserted rather than only reported.
const RATCHET: f64 = 100.0;

struct Source {
    origins: Origins,
    file: FileId,
    tokens: Vec<Token>,
}

impl Source {
    fn new(text: &str) -> Source {
        let mut origins = Origins::new();
        let file = origins.add_file("top.sv", text.to_string());
        let tokens = tokenize(origins.text(file));
        Source {
            origins,
            file,
            tokens,
        }
    }

    fn input(&self) -> Input<'_> {
        Input::new(self.file, self.origins.text(self.file), &self.tokens)
    }
}

/// The text of each `VERBATIM` run one file parses to.
fn runs(text: &str) -> Vec<String> {
    let source = Source::new(text);
    parse(source.input())
        .children()
        .filter(|node| node.kind() == VERBATIM)
        .map(|node| node.text().to_string())
        .collect()
}

/// One run taken in `context`, and what is left after it.
///
/// The remainder is read off the tree rather than off the cursor, so that the
/// whitespace a rule never sees is in it: where a run stops is a question
/// about bytes.
fn one(text: &str, context: Context) -> (String, String) {
    let source = Source::new(text);
    let mut parser = Parser::new(Raw::new(source.input()));
    let file = parser.start();
    verbatim(&mut parser, context);
    while !parser.at_end() {
        parser.bump();
    }
    parser.complete(file, SOURCE_FILE);

    let tree = SyntaxNode::new_root(build(&parser.finish(), source.input()));
    let taken = tree
        .children()
        .find(|node| node.kind() == VERBATIM)
        .map(|node| node.text().to_string())
        .unwrap_or_default();
    let whole = tree.text().to_string();
    let rest = whole[taken.len()..].to_string();
    (taken, rest)
}

/// Grammar tokens inside a `VERBATIM`, and grammar tokens in all.
fn rate(tree: &SyntaxNode) -> (usize, usize) {
    fn walk(node: &SyntaxNode, inside: bool, verbatim: &mut usize, total: &mut usize) {
        let inside = inside || node.kind() == VERBATIM;
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => walk(&node, inside, verbatim, total),
                NodeOrToken::Token(token) if !token.kind().is_trivia() => {
                    *total += 1;
                    *verbatim += usize::from(inside);
                }
                NodeOrToken::Token(_) => {}
            }
        }
    }

    let (mut verbatim, mut total) = (0, 0);
    walk(tree, false, &mut verbatim, &mut total);
    (verbatim, total)
}

#[test]
fn a_run_ends_at_its_own_semicolon() {
    assert_eq!(runs("logic a;\nlogic b;\n"), ["logic a;", "\nlogic b;"]);
}

#[test]
fn a_semicolon_inside_something_does_not_end_it() {
    let (taken, rest) = one(
        "for (int i = 0; i < 4; i++) x = 1; more",
        Context::Terminated,
    );
    assert_eq!(taken, "for (int i = 0; i < 4; i++) x = 1;");
    assert_eq!(rest, " more");
}

#[test]
fn a_block_is_taken_whole() {
    let (taken, rest) = one(
        "always_ff begin\n  a <= 1;\n  b <= 2;\nend\nnext",
        Context::Terminated,
    );
    assert_eq!(taken, "always_ff begin\n  a <= 1;\n  b <= 2;\nend");
    assert_eq!(rest, "\nnext");
}

#[test]
fn a_module_is_one_run() {
    assert_eq!(
        runs("module m;\n  assign x = 1;\nendmodule\n"),
        ["module m;\n  assign x = 1;\nendmodule"]
    );
}

#[test]
fn a_run_cannot_escape_past_what_encloses_it() {
    // The `end` belongs to a `begin` this run never saw, so it stops rather
    // than swallowing the rest of the file.
    let (taken, rest) = one("a = 1 end more", Context::Terminated);
    assert_eq!(taken, "a = 1");
    assert_eq!(rest, " end more");
}

#[test]
fn a_stray_closer_still_makes_progress() {
    // Otherwise a caller looping until the end would never get past it.
    let (taken, _) = one("endmodule", Context::Terminated);
    assert_eq!(taken, "endmodule");
}

#[test]
fn a_prototype_has_no_body_to_look_for() {
    // Each of these would otherwise push a `function` that no `endfunction`
    // closes, and the run would eat everything after it.
    for text in [
        "extern function void f(); logic after;",
        "pure virtual function int g(); logic after;",
        "import \"DPI-C\" function void h(); logic after;",
        "extern task t(); logic after;",
    ] {
        let (taken, rest) = one(text, Context::Terminated);
        assert!(taken.ends_with(';'), "{text:?} took {taken:?}");
        assert_eq!(rest, " logic after;", "{text:?}");
    }
}

#[test]
fn a_function_with_a_body_is_taken_whole() {
    let (taken, rest) = one(
        "virtual function int g();\n  return 1;\nendfunction\nafter",
        Context::Terminated,
    );
    assert_eq!(taken, "virtual function int g();\n  return 1;\nendfunction");
    assert_eq!(rest, "\nafter");
}

#[test]
fn the_keywords_that_only_sometimes_open_something() {
    // A forward declaration, a type, and an inline assertion: none of the
    // three has an `end...` to find.
    for (text, expected) in [
        ("typedef class C; logic after;", "typedef class C;"),
        (
            "virtual interface axi_if vif; logic after;",
            "virtual interface axi_if vif;",
        ),
        (
            "assert property (@(posedge clk) a |-> b); logic after;",
            "assert property (@(posedge clk) a |-> b);",
        ),
    ] {
        let (taken, rest) = one(text, Context::Terminated);
        assert_eq!(taken, expected);
        assert_eq!(rest, " logic after;", "{text:?}");
    }
}

#[test]
fn an_interface_class_is_closed_by_endclass() {
    let (taken, rest) = one("interface class C; endclass after", Context::Terminated);
    assert_eq!(taken, "interface class C; endclass");
    assert_eq!(rest, " after");
}

#[test]
fn a_list_element_ends_at_the_comma() {
    let (taken, rest) = one("a + b, c", Context::Element);
    assert_eq!(taken, "a + b");
    assert_eq!(rest, ", c");
}

#[test]
fn a_list_element_keeps_its_own_brackets() {
    let (taken, rest) = one("f(x, y), next", Context::Element);
    assert_eq!(taken, "f(x, y)");
    assert_eq!(rest, ", next");
}

#[test]
fn a_file_of_runs_is_still_the_file() {
    for text in [
        "",
        "module m; endmodule\n",
        "class C;\n  extern function void f();\n  function int g(); return 1; endfunction\nendclass\n",
        ") ) ) ;",
        "begin begin begin",
    ] {
        let source = Source::new(text);
        assert_eq!(parse(source.input()).text().to_string(), text, "{text:?}");
    }
}

/// The number M3 is graded on, over the whole corpus.
#[test]
fn corpus_verbatim_rate_does_not_rise() {
    let Some(files) = corpus::files() else {
        return;
    };

    let mut per_repo: Vec<(String, usize, usize)> = Vec::new();
    let (mut all_verbatim, mut all_total) = (0usize, 0usize);

    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let mut origins = Origins::new();
        let file = origins.add_file(path, text);
        let text = origins.text(file);
        let tokens = tokenize(text);
        let tree = parse(Input::new(file, text, &tokens));

        let (verbatim, total) = rate(&tree);
        all_verbatim += verbatim;
        all_total += total;

        let repo = corpus::repo(path);
        match per_repo.iter_mut().find(|(name, _, _)| *name == repo) {
            Some(entry) => {
                entry.1 += verbatim;
                entry.2 += total;
            }
            None => per_repo.push((repo, verbatim, total)),
        }
    }

    per_repo.sort();
    for (repo, verbatim, total) in &per_repo {
        eprintln!(
            "{repo:16} {:5.1}%  ({verbatim}/{total})",
            100.0 * *verbatim as f64 / *total as f64
        );
    }

    let overall = 100.0 * all_verbatim as f64 / all_total as f64;
    eprintln!("{:16} {overall:5.1}%  ({all_verbatim}/{all_total})", "all");
    assert!(
        overall <= RATCHET,
        "the verbatim rate rose to {overall:.1}%, above the recorded {RATCHET:.1}%"
    );
}
