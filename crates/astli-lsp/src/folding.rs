//! `textDocument/foldingRange`: the bodies of one file, from its raw tree.
//!
//! A body folds from the line it opens on to the one its closer is on,
//! `endmodule`, `end`, `)`, which stays in view. Each branch of a
//! conditional folds on its own, since the tree keeps every one. A run of
//! comment lines and a `` `define `` written over several lines fold too.
//! Nothing inside a `VERBATIM` region or a macro call's arguments does.

use astli_syntax::{SyntaxKind::*, SyntaxNode, SyntaxToken, first_significant, last_significant};
use lsp_server::ResponseError;
use lsp_types::{FoldingRange, FoldingRangeKind, FoldingRangeParams};
use rowan::{NodeOrToken, TextSize};

use crate::buffers::Buffer;
use crate::position::Encoding;
use crate::server::Server;

impl Server {
    pub fn folding_ranges(
        &mut self,
        params: FoldingRangeParams,
    ) -> Result<Option<Vec<FoldingRange>>, ResponseError> {
        let encoding = self.encoding;
        let lines_only = (self.client.text_document.as_ref())
            .and_then(|it| it.folding_range.as_ref())
            .and_then(|it| it.line_folding_only)
            .unwrap_or(false);
        let Some((buffer, tree)) = self.raw(&params.text_document.uri) else {
            return Ok(None);
        };
        let folds = Folds {
            buffer,
            encoding,
            lines_only,
            found: Vec::new(),
        };
        Ok(Some(folds.of(tree.root())))
    }
}

struct Folds<'a> {
    buffer: &'a Buffer,
    encoding: Encoding,
    /// Whether the client folds whole lines only, and so a closer has to be
    /// left out of the fold to stay in view.
    lines_only: bool,
    found: Vec<FoldingRange>,
}

impl Folds<'_> {
    fn of(mut self, root: &SyntaxNode) -> Vec<FoldingRange> {
        self.nodes(root);
        self.comments(root);
        self.found
            .sort_by_key(|it| (it.start_line, std::cmp::Reverse(it.end_line)));
        self.found
    }

    fn line(&self, offset: TextSize) -> u32 {
        self.buffer.lines.line(offset.into()) as u32
    }

    /// The folds of `node` and of the nodes inside it.
    fn nodes(&mut self, node: &SyntaxNode) {
        for child in node.children() {
            match child.kind() {
                VERBATIM | MACRO_CALL => continue,
                MODULE_DECL | INTERFACE_DECL | PROGRAM_DECL | PACKAGE_DECL | CLASS_DECL
                | CHECKER_DECL | UDP_DECL | CONFIG_DECL | FUNCTION_DECL | TASK_DECL
                | PROPERTY_DECL | SEQUENCE_DECL | COVERGROUP_DECL | CLOCKING_DECL => {
                    // A body starts after its header, which its parameters
                    // and ports fold apart from.
                    let opens = (child.children_with_tokens())
                        .filter_map(NodeOrToken::into_token)
                        .find(|it| it.kind() == SEMICOLON);
                    self.body(&child, opens);
                }
                BLOCK | CASE_STMT | GENERATE_REGION | SPECIFY_BLOCK | UDP_TABLE
                | CONSTRAINT_BLOCK | PARAM_PORT_LIST | PORT_LIST | ENUM_TYPE | STRUCT_TYPE
                | UNION_TYPE | CONDITIONAL_BRANCH => self.body(&child, None),
                ARG_LIST if node.kind() == INSTANCE => self.body(&child, None),
                DIRECTIVE if first_significant(&child).is_some_and(|it| it.text() == "`define") => {
                    self.body(&child, None);
                }
                _ => {}
            }
            self.nodes(&child);
        }
    }

    /// A fold of `node` from the line of `opens`, or of its first token,
    /// to the line its closer is on, if it spans more than one.
    ///
    /// The closer is the node's own token that starts its last line:
    /// `endmodule`, `end`, `)`. A last line that starts with a token of a
    /// node inside it, as a conditional branch's does, has no closer.
    fn body(&mut self, node: &SyntaxNode, opens: Option<SyntaxToken>) {
        let (Some(first), Some(last)) = (
            opens.or_else(|| first_significant(node)),
            last_significant(node),
        ) else {
            return;
        };
        let start_line = self.line(first.text_range().start());
        let end_line = self.line(last.text_range().end());
        let closer = self
            .first_on_line(node, end_line)
            .filter(|it| it.parent().as_ref() == Some(node));
        let (end_line, end_character) = match closer {
            Some(_) if self.lines_only => (end_line.saturating_sub(1), None),
            Some(closer) => {
                let at = closer.text_range().start().into();
                let position = (self.encoding).position(&self.buffer.text, &self.buffer.lines, at);
                (position.line, Some(position.character))
            }
            None => (end_line, None),
        };
        self.push(start_line, end_line, end_character, None);
    }

    /// The first token on `line` that is not trivia, within `node`.
    fn first_on_line(&self, node: &SyntaxNode, line: u32) -> Option<SyntaxToken> {
        let start = self.buffer.lines.start(line as usize)?;
        let start = TextSize::from(start).max(node.text_range().start());
        let token = node.token_at_offset(start).right_biased()?;
        std::iter::successors(Some(token), SyntaxToken::next_token)
            .find(|it| !it.kind().is_trivia())
    }

    /// The comments that fold: a run of `//` lines, each with nothing but
    /// space before it, and a `/* */` over more than one line.
    fn comments(&mut self, root: &SyntaxNode) {
        // The lines of the run so far, and the last line code is on, which
        // a comment after code on its line does not start a run on.
        let mut run: Option<(u32, u32)> = None;
        let mut code = None;
        for token in (root.descendants_with_tokens()).filter_map(NodeOrToken::into_token) {
            let range = token.text_range();
            match token.kind() {
                WHITESPACE => {}
                LINE_COMMENT => {
                    let line = self.line(range.start());
                    match run {
                        Some((start, end)) if line == end + 1 => run = Some((start, line)),
                        _ => {
                            self.comment_run(run.take());
                            run = (code != Some(line)).then_some((line, line));
                        }
                    }
                }
                kind => {
                    self.comment_run(run.take());
                    code = Some(self.line(range.end()));
                    if kind == BLOCK_COMMENT {
                        let start = self.line(range.start());
                        let end = self.line(range.end());
                        self.push(start, end, None, Some(FoldingRangeKind::Comment));
                    }
                }
            }
        }
        self.comment_run(run);
    }

    fn comment_run(&mut self, run: Option<(u32, u32)>) {
        if let Some((start, end)) = run {
            self.push(start, end, None, Some(FoldingRangeKind::Comment));
        }
    }

    /// A fold, if it spans more than one line.
    fn push(
        &mut self,
        start_line: u32,
        end_line: u32,
        end_character: Option<u32>,
        kind: Option<FoldingRangeKind>,
    ) {
        if end_line > start_line {
            self.found.push(FoldingRange {
                start_line,
                start_character: None,
                end_line,
                end_character,
                kind,
                collapsed_text: None,
            });
        }
    }
}
