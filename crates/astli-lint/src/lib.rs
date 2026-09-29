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
//!
//! # Waivers
//!
//! `(* astli_allow = "rule, group" *)` on a construct turns those rules off
//! inside it: on a statement, a procedural block, a declaration, an
//! instance, a module. A name that is neither a rule's nor a group's, or a
//! value that is not a string, is reported as [`INVALID_WAIVER`].
//!
//! ```
//! use astli_lint::{Config, lint};
//! use astli_parse::SyntaxTree;
//!
//! let source = r#"module top;
//!   (* astli_allow = "always-ff-non-blocking" *)
//!   always_ff @(posedge clk_i) q = d;
//! endmodule
//! "#;
//! let tree = SyntaxTree::parse("top.sv", source.to_string());
//! assert!(lint(&tree, &Config::default()).is_empty());
//! ```

mod config;
mod rule;
mod rules;
mod waiver;

pub use config::{Config, Level, UnknownRule};
pub use rule::{Group, RULES, Rule};
pub use waiver::INVALID_WAIVER;

use astli_parse::SyntaxTree;
use astli_text::{Diagnostic, Severity};
use rowan::TextRange;

use rule::Cx;
use waiver::Waivers;

/// What the rules `config` leaves on find in `tree`, in the order of the
/// file.
///
/// A rule at [`Level::Warn`] reports a warning and one at [`Level::Deny`] an
/// error. What the parser found malformed is not repeated: that is
/// [`SyntaxTree::diagnostics`].
pub fn lint(tree: &SyntaxTree, config: &Config) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    let waivers = Waivers::read(tree, &mut found);
    for (rule, level) in RULES.iter().zip(config.levels()) {
        let severity = match level {
            Level::Allow => continue,
            Level::Warn => Severity::Warning,
            Level::Deny => Severity::Error,
        };
        let mut hits = Vec::new();
        (rule.check)(&mut Cx {
            tree,
            rule,
            severity,
            found: &mut hits,
        });
        hits.retain(|hit| {
            let range = TextRange::new(hit.at.start.into(), hit.at.end.into());
            !waivers.cover(rule, range)
        });
        found.extend(hits);
    }
    found.sort_by_key(|diagnostic| (diagnostic.at.start, diagnostic.code));
    found
}
