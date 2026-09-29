//! The rule table, and what a rule reports through.

use std::fmt;

use astli_parse::SyntaxTree;
use astli_syntax::SyntaxNode;
use astli_text::{Code, Diagnostic, Severity};
use rowan::TextRange;

use crate::Level;
use crate::rules::procedural;

/// Every rule, in the order `--list` prints them: by group, then by name.
pub static RULES: &[Rule] = &[
    Rule {
        name: "always-comb-blocking",
        group: Group::Correctness,
        summary: "a non-blocking assignment in `always_comb`",
        check: procedural::always_comb_blocking,
    },
    Rule {
        name: "always-ff-non-blocking",
        group: Group::Correctness,
        summary: "a blocking assignment in `always_ff` to a variable not declared in it",
        check: procedural::always_ff_non_blocking,
    },
];

/// One lint rule.
pub struct Rule {
    /// The name diagnostics carry as their code, and configuration uses.
    pub name: &'static str,
    pub group: Group,
    /// What the rule finds, in a phrase.
    pub summary: &'static str,
    pub(crate) check: fn(&mut Cx),
}

impl fmt::Debug for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

/// What kind of problem a rule finds, which decides its default level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    /// Almost certainly a bug. Denied by default.
    Correctness,
    /// Legal, and more often wrong than meant. Warned by default.
    Suspicious,
    /// lowRISC's Verilog style guide, as OpenTitan configures it. Warned by
    /// default.
    Lowrisc,
    /// A construct a project may choose to forbid. Allowed by default.
    Restriction,
}

impl Group {
    pub const ALL: [Group; 4] = [
        Group::Correctness,
        Group::Suspicious,
        Group::Lowrisc,
        Group::Restriction,
    ];

    /// The group called `name`, as configuration spells it.
    pub fn named(name: &str) -> Option<Group> {
        Group::ALL.into_iter().find(|group| group.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Group::Correctness => "correctness",
            Group::Suspicious => "suspicious",
            Group::Lowrisc => "lowrisc",
            Group::Restriction => "restriction",
        }
    }

    /// The level of each of its rules when nothing is configured.
    pub fn level(self) -> Level {
        match self {
            Group::Correctness => Level::Deny,
            Group::Suspicious | Group::Lowrisc => Level::Warn,
            Group::Restriction => Level::Allow,
        }
    }
}

/// What one rule reads and reports into, over one run of it.
pub(crate) struct Cx<'a> {
    pub tree: &'a SyntaxTree,
    pub rule: &'static Rule,
    pub severity: Severity,
    pub found: &'a mut Vec<Diagnostic>,
}

impl<'a> Cx<'a> {
    pub fn root(&self) -> &'a SyntaxNode {
        self.tree.root()
    }

    /// A diagnostic from this rule at `range`, at the level it runs at, for
    /// the rule to add labels and notes to before it reports it.
    pub fn diagnostic(&self, range: TextRange, message: impl Into<String>) -> Diagnostic {
        let at = self.tree.span(range);
        Diagnostic::new(self.severity, Code(self.rule.name), at, message)
    }

    pub fn report(&mut self, diagnostic: Diagnostic) {
        self.found.push(diagnostic);
    }
}
