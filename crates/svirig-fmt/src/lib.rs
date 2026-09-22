//! SystemVerilog formatter.
//!
//! [`format()`] lays out one file on its own: no include is followed and no
//! definition from outside the file is consulted, so the output depends on the
//! file's bytes alone. Before returning, it checks that the result preprocesses
//! to the same thing as the input under any set of definitions, and refuses
//! with a [`Refusal`] instead of returning text that would not.

mod transparency;

pub use transparency::{Reason, Refusal};

use svirig_parse::SyntaxTree;

/// Formats the file `tree` was parsed from.
///
/// No construct has a rule yet, so every node is written as it was read.
pub fn format(tree: &SyntaxTree) -> Result<String, Refusal> {
    let formatted = tree.root().to_string();
    transparency::check(tree.source(), &formatted)?;
    Ok(formatted)
}
