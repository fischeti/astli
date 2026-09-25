//! A SystemVerilog formatter, in the style of lowRISC's Verilog style guide.
//!
//! [`format()`] takes a [`SyntaxTree`] from `astli-parse` and returns the file
//! laid out again:
//!
//! ```
//! use astli_fmt::format;
//! use astli_parse::SyntaxTree;
//!
//! let source = r"module top(input logic clk,output logic [7:0] q);
//! logic [7:0] count;  // counter
//! wire   enable;
//! assign q=count;
//! endmodule
//! ";
//! let tree = SyntaxTree::parse("top.sv", source.to_string());
//!
//! assert_eq!(
//!     format(&tree).unwrap(),
//!     r"module top (
//!   input  logic       clk,
//!   output logic [7:0] q
//! );
//!   logic [7:0] count; // counter
//!   wire        enable;
//!   assign q = count;
//! endmodule
//! ",
//! );
//! ```
//!
//! The lines are 100 columns wide and indented by two. There are no options
//! yet. The rules the output follows are in
//! [`docs/formatter.md`](https://github.com/fischeti/astli/blob/main/docs/formatter.md).
//!
//! # One file, as written
//!
//! The formatter reads one file on its own. It follows no `` `include `` and
//! looks up no definition from outside the file, so the output depends on
//! the file's bytes alone. Directives and macro calls are laid out where they
//! stand, and every branch of an `` `ifdef `` is formatted.
//!
//! # What is left alone
//!
//! A construct the grammar or the rules do not cover yet is written as it was
//! read, its lines moved together to where it now stands. [`unformatted`]
//! lists those nodes, the outermost of each, in order:
//!
//! ```
//! use astli_fmt::{format, unformatted};
//! use astli_parse::SyntaxTree;
//!
//! let source = "module top;\nnettype   real wire_t with resolver;\nendmodule\n";
//! let tree = SyntaxTree::parse("top.sv", source.to_string());
//!
//! assert_eq!(format(&tree).unwrap(), "module top;\n  nettype   real wire_t with resolver;\nendmodule\n");
//! let [left] = unformatted(&tree).try_into().unwrap();
//! assert_eq!(left.text().to_string().trim(), "nettype   real wire_t with resolver;");
//! ```
//!
//! # Refusals
//!
//! Before returning, [`format()`] checks that the output preprocesses to the
//! same tokens as the input, under every set of definitions at once. If not,
//! it returns a [`Refusal`] instead of the text: the tokens other than
//! whitespace changed, a directive's line took in or lost a token, or a
//! `` `define `` was rewritten. Each is a bug in the formatter, caught before
//! it reaches a file. [`Refusal::offset`] is where in the input the output
//! first departs, which is what a report of it needs.
//!
//! ```
//! # use astli_parse::SyntaxTree;
//! # let tree = SyntaxTree::parse("top.sv", String::new());
//! match astli_fmt::format(&tree) {
//!     Ok(text) => { /* write it back */ }
//!     Err(refusal) => eprintln!("top.sv: {refusal} at byte {}", refusal.offset),
//! }
//! ```
//!
//! A file with a syntax error still formats: what the parser could not take
//! apart is left as it was. Check the tree's `diagnostics()` first if a tool
//! should not touch such a file.

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

/// Formats the file `tree` was parsed from, or refuses if the result would
/// preprocess differently.
///
/// A construct no rule lays out yet is written as it was read, its lines
/// moved together to where it now stands. The line ending is the one the
/// file's first line has.
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
