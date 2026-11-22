//! `textDocument/formatting`: `astli fmt`'s output for an open buffer, as
//! an edit of the lines that differ, so that the client's cursor and folds
//! keep their place.
//!
//! The layout is the `[fmt]` of the file's package, which the host reads.
//! The client's formatting options are ignored: `astli.toml` is the
//! configuration, and every run in a project lays a file out the same.

use astli_parse::SyntaxTree;
use astli_text::Lines;
use lsp_server::{ErrorCode, RequestId};
use lsp_types::{DocumentFormattingParams, MessageType, Range, TextEdit};
use serde_json::Value;
use similar::{DiffTag, TextDiff};

use crate::buffers;
use crate::position::Encoding;
use crate::server::{Server, error};

impl Server {
    /// Answers with the edits that format the buffer, or none if the file
    /// is not open, is excluded, or is already formatted. A configuration
    /// that cannot be read and a refusal are told to the client, since a
    /// format on save that does nothing would otherwise say nothing.
    pub fn formatting(&self, id: RequestId, params: Value) {
        let params: DocumentFormattingParams = match serde_json::from_value(params) {
            Ok(params) => params,
            Err(err) => {
                let said = format!("`textDocument/formatting`: {err}");
                return self.answer::<()>(id, Err(error(ErrorCode::InvalidParams, said)));
            }
        };
        let open = buffers::path(&params.text_document.uri)
            .and_then(|path| Some((self.buffers.get(&path)?, path)));
        let Some((buffer, path)) = open else {
            return self.answer(id, Ok(Vec::<TextEdit>::new()));
        };
        let options = match self.host.fmt_options(&path) {
            Ok(Some(options)) => options,
            Ok(None) => return self.answer(id, Ok(Vec::<TextEdit>::new())),
            Err(why) => {
                self.show(MessageType::Error, why);
                return self.answer(id, Ok(Vec::<TextEdit>::new()));
            }
        };
        let tree = SyntaxTree::parse(&path, buffer.text.to_string());
        match astli_fmt::format(&tree, &options) {
            Ok(formatted) => {
                let edits = edits(&buffer.text, &buffer.lines, &formatted, self.encoding);
                self.answer(id, Ok(edits));
            }
            Err(refusal) => {
                let at = tree.line_col(refusal.offset);
                let said = format!(
                    "{}:{at}: {refusal}: the file is left as it is, and this is a formatter bug",
                    path.display()
                );
                self.show(MessageType::Error, said);
                self.answer(id, Ok(Vec::<TextEdit>::new()));
            }
        }
    }
}

/// The edits that turn `text` into `formatted`: one for each run of lines
/// that differ, each replacing whole lines, so that a line the formatter
/// left alone is not touched. Fixing a whole file is answered the same way.
pub fn edits(text: &str, lines: &Lines, formatted: &str, encoding: Encoding) -> Vec<TextEdit> {
    let diff = TextDiff::from_lines(text, formatted);
    let new: Vec<&str> = diff.iter_new_slices().collect();
    // The offset of the start of line `line`, or the end of the text for
    // the line past the last.
    let start = |line: usize| lines.start(line).unwrap_or(text.len() as u32);
    let place = |line: usize| encoding.position(text, lines, start(line));
    (diff.ops().iter())
        .map(|op| op.as_tag_tuple())
        .filter(|(tag, ..)| *tag != DiffTag::Equal)
        .map(|(_, old, written)| TextEdit {
            range: Range {
                start: place(old.start),
                end: place(old.end),
            },
            new_text: new[written].concat(),
        })
        .collect()
}
