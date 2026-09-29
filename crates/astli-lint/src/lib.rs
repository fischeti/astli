//! SystemVerilog lint rules over one file's syntax tree.
//!
//! [`lint`] runs every rule a [`Config`] leaves on over a [`SyntaxTree`], and
//! returns what they found as diagnostics, each coded with its rule's name:
//!
//! ```
//! use astli_lint::{Config, lint};
//! use astli_parse::SyntaxTree;
//!
//! let source = "module top;\n  always_ff @(posedge clk_i) q = d;\nendmodule\n";
//! let tree = SyntaxTree::parse("top.sv", source.to_string());
//!
//! let [found] = lint(&tree, &Config::default()).try_into().unwrap();
//! assert_eq!(found.code.as_str(), "always-ff-non-blocking");
//! ```
//!
//! # One file, as written
//!
//! A rule reads the file as written, as the formatter does: macro calls
//! unexpanded, every branch of an `` `ifdef `` present, no `` `include ``
//! followed. What a macro call might write is not looked into, so a rule
//! says nothing about it.
//!
//! # Rules and groups
//!
//! Every rule belongs to one [`Group`], which sets whether it is on by
//! default. [`Config::set`] takes a rule's name or a group's, so a project can
//! turn a group on and one of its rules off again. [`RULES`] lists them.
//! Where verible has the same rule, it has the same name.

mod config;
mod rule;
mod rules;

pub use config::{Config, Level, UnknownRule};
pub use rule::{Group, RULES, Rule};

use astli_parse::SyntaxTree;
use astli_text::{Diagnostic, Severity};

use rule::Cx;

/// What the rules `config` leaves on find in `tree`, in the order of the
/// file.
///
/// A rule at [`Level::Warn`] reports a warning and one at [`Level::Deny`] an
/// error. What the parser found malformed is not repeated: that is
/// [`SyntaxTree::diagnostics`].
pub fn lint(tree: &SyntaxTree, config: &Config) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    for (rule, level) in RULES.iter().zip(config.levels()) {
        let severity = match level {
            Level::Allow => continue,
            Level::Warn => Severity::Warning,
            Level::Deny => Severity::Error,
        };
        (rule.check)(&mut Cx {
            tree,
            rule,
            severity,
            found: &mut found,
        });
    }
    found.sort_by_key(|diagnostic| (diagnostic.at.start, diagnostic.code));
    found
}
