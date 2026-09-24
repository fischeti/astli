//! SystemVerilog formatter.
//!
//! [`format()`] lays out one file on its own: no include is followed and no
//! definition from outside the file is consulted, so the output depends on the
//! file's bytes alone. Before returning, it checks that the result preprocesses
//! to the same thing as the input under any set of definitions, and refuses
//! with a [`Refusal`] instead of returning text that would not.

mod align;
mod comments;
mod doc;
mod rules;
mod transparency;
mod verbatim;

pub use transparency::{Reason, Refusal};

use astli_parse::SyntaxTree;
use astli_syntax::SyntaxNode;

use crate::doc::{Layout, print};

/// The defaults D7 in `docs/plan.md` names; there are no options yet.
const LAYOUT: Layout = Layout {
    width: 100,
    indent: 2,
};

/// Formats the file `tree` was parsed from.
///
/// A construct no rule lays out yet is written as it was read, its lines
/// moved together to where it now stands.
pub fn format(tree: &SyntaxTree) -> Result<String, Refusal> {
    let (doc, _) = rules::write(tree.root(), tree.source());
    let formatted = line_endings(tree.source(), print(&doc, LAYOUT));
    transparency::check(tree.source(), &formatted)?;
    Ok(formatted)
}

/// The nodes [`format()`] writes as they were read, because no rule lays them
/// out: the outermost of them, in order.
pub fn unformatted(tree: &SyntaxTree) -> Vec<SyntaxNode> {
    rules::write(tree.root(), tree.source()).1
}

/// `formatted` with the line ending `source` has on its first line.
///
/// A line inside a token keeps its `\r`, so only the newlines without one
/// are given one.
fn line_endings(source: &str, formatted: String) -> String {
    let crlf = source
        .find('\n')
        .is_some_and(|at| source[..at].ends_with('\r'));
    if !crlf {
        return formatted;
    }
    let mut out = String::with_capacity(formatted.len() + formatted.len() / 16);
    let mut after_cr = false;
    for char in formatted.chars() {
        if char == '\n' && !after_cr {
            out.push('\r');
        }
        out.push(char);
        after_cr = char == '\r';
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format_str(source: &str) -> String {
        format(&SyntaxTree::parse("test.sv", source.to_owned())).unwrap()
    }

    /// Not a case under `tests/data`, whose snapshots are read with their line
    /// endings normalised.
    #[test]
    fn a_crlf_file_stays_crlf() {
        let source = "  module m;  \r\n  `define A \\\r\n    1\r\n  endmodule\r\n\r\n\r\n";
        let formatted = "module m;\r\n  `define A \\\r\n    1\r\nendmodule\r\n";
        assert_eq!(format_str(source), formatted);
        assert_eq!(format_str(formatted), formatted);
    }

    #[test]
    fn a_file_of_whitespace_formats_to_nothing() {
        assert_eq!(format_str(" \n\n"), "");
    }
}
