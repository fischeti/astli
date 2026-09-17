//! The tree the events describe, and the trivia the events never saw.

use svirig_syntax::parser::{Events, Tokens, build, parse};
use svirig_syntax::preproc::{Input, Preprocessor};
use svirig_syntax::{SyntaxKind::*, SyntaxNode};
use svirig_text::FileId;

mod corpus;

struct Source {
    pp: Preprocessor<'static>,
    file: FileId,
}

impl Source {
    fn new(text: &str) -> Source {
        let mut pp = Preprocessor::new();
        let file = pp.add("top.sv", text.to_string());
        Source { pp, file }
    }

    fn input(&self) -> Input<'_> {
        self.pp.input(self.file)
    }
}

/// The tree, with each node's text in brackets, so that a test can say where a
/// comment landed.
fn shape(node: &SyntaxNode) -> String {
    let mut out = format!("({:?}", node.kind());
    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Node(node) => {
                out.push(' ');
                out.push_str(&shape(&node));
            }
            rowan::NodeOrToken::Token(token) => {
                out.push_str(&format!(" {:?}", token.text()));
            }
        }
    }
    out.push(')');
    out
}

/// `SOURCE_FILE` over one `VERBATIM` per token run, split where `at` says.
///
/// Two nodes is the smallest arrangement that has a boundary for trivia to
/// fall on one side or the other of.
fn split_at(source: &Source, at: usize) -> SyntaxNode {
    // `Input` is a view and copies, so one borrow of the session serves both
    // the reading and the build.
    let input = source.input();
    let mut tokens = svirig_syntax::parser::Raw::new(input);
    let mut events = Events::new();
    let file = events.start();

    for half in [at, usize::MAX] {
        let node = events.start();
        let mut taken = 0;
        while !tokens.at_end() && taken < half {
            events.token(tokens.kind(0));
            tokens.bump();
            taken += 1;
        }
        node.complete(&mut events, VERBATIM);
    }

    file.complete(&mut events, SOURCE_FILE);
    SyntaxNode::new_root(build(&events.resolve(), input))
}

#[test]
fn a_file_comes_back_byte_for_byte() {
    for text in [
        "",
        "\n",
        "module m; endmodule\n",
        "  // only a comment\n",
        "module m;\r\n  assign x = 1'b0;\r\nendmodule\r\n",
        "`define W(x) x + \\\n  1\nlogic [`W(2):0] y;\n",
        "/* unterminated\n",
        "\\odd.name ",
    ] {
        let source = Source::new(text);
        let tree = parse(source.input());
        assert_eq!(tree.text().to_string(), text, "{text:?}");
    }
}

#[test]
fn a_comment_on_the_same_line_stays_with_what_it_annotates() {
    // The boundary falls after `;`, and the comment must not cross it.
    let source = Source::new("logic x; // why\nlogic y;\n");
    assert_eq!(
        shape(&split_at(&source, 3)),
        r#"(SOURCE_FILE (VERBATIM "logic" " " "x" ";" " " "// why") (VERBATIM "\n" "logic" " " "y" ";") "\n")"#
    );
}

#[test]
fn a_comment_on_its_own_line_belongs_to_what_follows() {
    let source = Source::new("logic x;\n// about y\nlogic y;\n");
    assert_eq!(
        shape(&split_at(&source, 3)),
        r#"(SOURCE_FILE (VERBATIM "logic" " " "x" ";") (VERBATIM "\n" "// about y" "\n" "logic" " " "y" ";") "\n")"#
    );
}

#[test]
fn whitespace_alone_never_attaches_backwards() {
    // No comment in the run, so the boundary takes none of it: the space
    // carries no signal, and the formatter asks for what it wants instead.
    let source = Source::new("logic x ;");
    assert_eq!(
        shape(&split_at(&source, 2)),
        r#"(SOURCE_FILE (VERBATIM "logic" " " "x") (VERBATIM " " ";"))"#
    );
}

#[test]
fn a_block_comment_that_opens_on_the_line_is_kept_there() {
    let source = Source::new("logic x; /* why\n   and more */ logic y;");
    assert_eq!(
        shape(&split_at(&source, 3)),
        r#"(SOURCE_FILE (VERBATIM "logic" " " "x" ";" " " "/* why\n   and more */") (VERBATIM " " "logic" " " "y" ";"))"#
    );
}

#[test]
fn a_trailing_comment_at_the_end_of_a_file_is_inside_the_tree() {
    let source = Source::new("endmodule // top\n");
    let tree = parse(source.input());
    assert_eq!(tree.text().to_string(), "endmodule // top\n");
    assert_eq!(
        shape(&tree),
        r#"(SOURCE_FILE (VERBATIM "endmodule" " " "// top") "\n")"#
    );
}

#[test]
#[should_panic(expected = "tokens were never put in the tree")]
fn a_rule_that_stops_early_is_a_bug() {
    let source = Source::new("module m; endmodule");
    let mut events = Events::new();
    let file = events.start();
    events.token(MODULE_KW);
    file.complete(&mut events, SOURCE_FILE);
    build(&events.resolve(), source.input());
}

#[test]
#[should_panic(expected = "more tokens than the file has")]
fn a_rule_that_runs_past_the_end_is_a_bug() {
    let source = Source::new("module");
    let mut events = Events::new();
    let file = events.start();
    for _ in 0..3 {
        events.token(MODULE_KW);
    }
    file.complete(&mut events, SOURCE_FILE);
    build(&events.resolve(), source.input());
}

/// Every corpus file, parsed and compared with itself.
///
/// There is no grammar yet, so what this proves is the trip through events and
/// back: the tree is the file. It is the invariant every later rung is not
/// allowed to break.
#[test]
fn corpus_round_trips_through_the_tree() {
    let Some(files) = corpus::files() else {
        return;
    };

    let mut parsed = 0usize;
    let mut bytes = 0usize;
    let mut differ = Vec::new();

    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue; // not UTF-8; not ours to parse
        };
        let mut pp = Preprocessor::new();
        let file = pp.add(path, text);
        let tree = parse(pp.input(file));
        let text = pp.origins().text(file);

        parsed += 1;
        bytes += text.len();
        if tree.text() != text {
            differ.push(path.display().to_string());
        }
    }

    eprintln!("{parsed} files, {bytes} bytes");
    assert!(
        differ.is_empty(),
        "{} files did not come back byte for byte, first few:\n{}",
        differ.len(),
        differ
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
