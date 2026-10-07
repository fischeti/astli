//! A SystemVerilog formatter, in the style of lowRISC's Verilog style guide.
//!
//! [`format()`] takes a [`SyntaxTree`] from `astli-parse` and returns the file
//! laid out again:
//!
//! ```
//! use astli_fmt::{Options, format};
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
//!     format(&tree, &Options::default()).unwrap(),
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
//! [`Options`] sets the width, the indentation and how far a column may pad
//! to line up; by default, 100 columns, two spaces and 12. The rules the
//! output follows are in
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
//! use astli_fmt::{Options, format, unformatted};
//! use astli_parse::SyntaxTree;
//!
//! let source = "module top;\nnettype   real wire_t with resolver;\nendmodule\n";
//! let tree = SyntaxTree::parse("top.sv", source.to_string());
//!
//! assert_eq!(format(&tree, &Options::default()).unwrap(), "module top;\n  nettype   real wire_t with resolver;\nendmodule\n");
//! let [left] = unformatted(&tree).try_into().unwrap();
//! assert_eq!(left.text().to_string().trim(), "nettype   real wire_t with resolver;");
//! ```
//!
//! So is an item, statement or member carrying the attribute
//! `(* astli_fmt_skip *)`, such as a table aligned by hand. That is asked
//! for, so [`unformatted`] leaves it out.
//!
//! ```
//! # use astli_fmt::{Options, format, unformatted};
//! # use astli_parse::SyntaxTree;
//! let source = "module top;\n(* astli_fmt_skip *)\nlogic   [7:0]   q;\nendmodule\n";
//! let tree = SyntaxTree::parse("top.sv", source.to_string());
//!
//! assert_eq!(format(&tree, &Options::default()).unwrap(), "module top;\n  (* astli_fmt_skip *)\n  logic   [7:0]   q;\nendmodule\n");
//! assert!(unformatted(&tree).is_empty());
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
//! match astli_fmt::format(&tree, &astli_fmt::Options::default()) {
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
use astli_syntax::{
    SyntaxKind::{DIRECTIVE, WHITESPACE},
    SyntaxNode,
};

use crate::doc::{Layout, print};

/// What a project may set about the layout [`format()`] writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// The columns a line may take.
    pub width: usize,
    /// The spaces of one level of indentation.
    pub indent: usize,
    /// The most padding a cell takes to line up with its column. A trailing
    /// comment and a named connection are exempt. Zero lines up only cells
    /// that already end together, and a value past `width` never limits.
    pub max_pad: usize,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            width: 100,
            indent: 2,
            max_pad: 12,
        }
    }
}

/// `options`, with the line ending of the first line break between tokens
/// in `tree`.
///
/// Not the first line's ending: a newline inside a token is the token's own,
/// and the first line may end in one the formatter moves.
fn layout(tree: &SyntaxTree, options: &Options) -> Layout {
    let first = (tree.root().descendants_with_tokens())
        .filter_map(|it| it.into_token())
        .find(|token| token.kind() == WHITESPACE && token.text().contains('\n'));
    let crlf = first.is_some_and(|token| {
        let text = token.text();
        text[..text.find('\n').unwrap_or(0)].ends_with('\r')
    });
    Layout {
        width: options.width,
        indent: options.indent,
        max_pad: options.max_pad,
        newline: if crlf { "\r\n" } else { "\n" },
    }
}

/// Formats the file `tree` was parsed from, or refuses if the result would
/// preprocess differently.
///
/// A construct no rule lays out yet is written as it was read, its lines
/// moved together to where it now stands. The line ending is the one the
/// file's first line break between tokens has.
///
/// A file with an encrypted envelope is written as it is: ciphertext has no
/// layout to improve, and a line of it moved may no longer decrypt.
pub fn format(tree: &SyntaxTree, options: &Options) -> Result<String, Refusal> {
    if encrypted(tree.root()) {
        return Ok(tree.source().to_string());
    }
    let (doc, _) = rules::write(tree.root(), tree.source());
    let formatted = print(&doc, layout(tree, options));
    transparency::check(tree.source(), &formatted)?;
    Ok(formatted)
}

/// Whether a directive in the tree at `root` is
/// `` `pragma protect begin_protected ``.
fn encrypted(root: &SyntaxNode) -> bool {
    let mut directives = root.descendants().filter(|node| node.kind() == DIRECTIVE);
    directives.any(|directive| {
        let mut words = (directive.children_with_tokens())
            .filter_map(|element| element.into_token())
            .filter(|token| !token.kind().is_trivia());
        let mut next = || words.next().map(|token| token.text().to_string());
        next().as_deref() == Some("`pragma")
            && next().as_deref() == Some("protect")
            && next().as_deref() == Some("begin_protected")
    })
}

/// The nodes [`format()`] writes as they were read, because no rule lays them
/// out: the outermost of them, in order.
pub fn unformatted(tree: &SyntaxTree) -> Vec<SyntaxNode> {
    rules::write(tree.root(), tree.source()).1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format_str(source: &str) -> String {
        format(
            &SyntaxTree::parse("test.sv", source.to_owned()),
            &Options::default(),
        )
        .unwrap()
    }

    /// Not a case under `tests/data`, whose snapshots are read with their line
    /// endings normalised.
    #[test]
    fn an_encrypted_file_is_written_as_it_is() {
        let source = "module ip (\n    input logic a\n);\n\
                      `pragma protect begin_protected\n\
                      `pragma protect data_block\n\
                      Qm9ndXMgY2lwaGVydGV4dA+//aB8Zm1n2Qk=\n\
                      `pragma protect end_protected\n\
                      endmodule\n";
        assert_eq!(format_str(source), source);
    }

    #[test]
    fn a_crlf_file_stays_crlf() {
        let source = "  module m;  \r\n  `define A \\\r\n    1\r\n  endmodule\r\n\r\n\r\n";
        let formatted = "module m;\r\n  `define A \\\r\n    1\r\nendmodule\r\n";
        assert_eq!(format_str(source), formatted);
        assert_eq!(format_str(formatted), formatted);
    }

    /// A newline inside a comment is the comment's, whichever line ending
    /// the file has.
    #[test]
    fn a_crlf_file_keeps_a_comment_with_bare_newlines() {
        let source = "module m;\r\n  /* a\n     b */\r\n  logic x;\r\nendmodule\r\n";
        assert_eq!(format_str(source), source);
    }

    #[test]
    fn a_file_of_whitespace_formats_to_nothing() {
        assert_eq!(format_str(" \n\n"), "");
    }

    fn format_with(source: &str, options: Options) -> String {
        let formatted = format(&SyntaxTree::parse("test.sv", source.to_owned()), &options);
        let again = format(
            &SyntaxTree::parse("test.sv", formatted.clone().unwrap()),
            &options,
        );
        assert_eq!(again, formatted, "not idempotent");
        formatted.unwrap()
    }

    /// A continuation is indented by two levels, whatever a level is.
    #[test]
    fn indent_sets_a_level_and_a_continuation_takes_two() {
        let source = "module m;\nalways_comb begin\nx = 1;\nend\n\
                      assign y = fffffffffffffffffffffffff(aaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbbbbbbb);\n\
                      endmodule\n";
        let options = Options {
            width: 60,
            indent: 4,
            ..Options::default()
        };
        let formatted = "module m;\n    always_comb begin\n        x = 1;\n    end\n    \
                         assign y = fffffffffffffffffffffffff(\n            \
                         aaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbbbbbbb\n    );\nendmodule\n";
        assert_eq!(format_with(source, options), formatted);
    }

    #[test]
    fn max_pad_limits_how_far_a_column_lines_up() {
        let source = "module m;\nlogic a;\nlogic [31:0] b;\nlogic [3:0] c;\nendmodule\n";
        let pad = |max_pad| {
            let options = Options {
                max_pad,
                ..Options::default()
            };
            format_with(source, options)
        };
        assert_eq!(
            pad(0),
            "module m;\n  logic a;\n  logic [31:0] b;\n  logic [3:0] c;\nendmodule\n"
        );
        assert_eq!(
            pad(5),
            "module m;\n  logic a;\n  logic [31:0] b;\n  logic [3:0]  c;\nendmodule\n"
        );
        assert_eq!(
            pad(usize::MAX),
            "module m;\n  logic        a;\n  logic [31:0] b;\n  logic [3:0]  c;\nendmodule\n"
        );
    }
}
