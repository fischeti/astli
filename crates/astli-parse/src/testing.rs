//! What the unit tests share: one rule, run once over a file of its own.

use astli_preproc::{Input, Session};
use astli_syntax::{SyntaxKind::SOURCE_FILE, SyntaxNode};

use crate::{Parser, Raw, build};

/// A session holding one file, for the tests that need its tokens.
pub struct Source {
    pub session: Session<'static>,
    pub file: astli_text::SourceId,
}

impl Source {
    pub fn new(text: &str) -> Source {
        let mut session = Session::new();
        let file = session.add("top.sv", text.to_string());
        Source { session, file }
    }

    pub fn input(&self) -> Input<'_> {
        self.session.input(self.file)
    }
}

/// Runs `rule` once at the start of `text`: the text of the node it built,
/// if it says it built one, and the text after that node.
///
/// Whatever the rule leaves is bumped in afterwards, so a rule that stops early
/// is still held to the tree being the file. A rule that builds nothing must
/// also consume nothing, or its caller could not fall back from where it began.
pub fn one(
    text: &str,
    rule: impl FnOnce(&mut Parser<Raw<'_>>) -> bool,
) -> (Option<String>, String) {
    let source = Source::new(text);
    let input = source.input();
    let mut parser = Parser::new(Raw::new(input));

    let root = parser.start();
    let before = parser.position();
    let took = rule(&mut parser);
    assert!(
        took || parser.position() == before,
        "the rule built nothing and still consumed tokens of {text:?}"
    );
    while !parser.at_end() {
        parser.bump();
    }
    parser.complete(root, SOURCE_FILE);

    let tree = SyntaxNode::new_root(build(&parser.finish().events, &input));
    assert_eq!(tree.text().to_string(), text, "the tree is not the file");

    match tree.children().next().filter(|_| took) {
        Some(node) => {
            let end = usize::from(node.text_range().end());
            (Some(node.text().to_string()), text[end..].to_string())
        }
        None => (None, text.to_string()),
    }
}
