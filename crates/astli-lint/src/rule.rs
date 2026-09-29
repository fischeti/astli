//! The rule table, and what a rule reports through.

use std::fmt;

use astli_parse::SyntaxTree;
use astli_syntax::SyntaxNode;
use astli_text::{Code, Diagnostic, Severity};
use rowan::TextRange;

use crate::Level;
use crate::rules::{begin, case, generate, instances, items, names, preproc, procedural, tokens};

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
    Rule {
        name: "duplicate-case-item",
        group: Group::Correctness,
        summary: "a `case` label written twice, so its second item is never taken",
        check: case::duplicate_case_item,
    },
    Rule {
        name: "always-comb",
        group: Group::Suspicious,
        summary: "`always @*` where `always_comb` would say what it is",
        check: procedural::always_comb,
    },
    Rule {
        name: "case-missing-default",
        group: Group::Suspicious,
        summary: "a `case` with no `default` item that is not `unique` or `unique0`",
        check: case::case_missing_default,
    },
    Rule {
        name: "forbid-defparam",
        group: Group::Suspicious,
        summary: "`defparam`, which overrides a parameter from elsewhere in the hierarchy",
        check: tokens::forbid_defparam,
    },
    Rule {
        name: "constraint-name-style",
        group: Group::Lowrisc,
        summary: "a constraint not `lower_snake_case` ending in `_c`",
        check: names::constraint_name_style,
    },
    Rule {
        name: "enum-name-style",
        group: Group::Lowrisc,
        summary: "an enum type not `lower_snake_case` ending in `_e` or `_t`",
        check: names::enum_name_style,
    },
    Rule {
        name: "generate-label",
        group: Group::Lowrisc,
        summary: "a generate block without a label",
        check: generate::generate_label,
    },
    Rule {
        name: "generate-label-prefix",
        group: Group::Lowrisc,
        summary: "a generate block label not starting with `gen_` or `g_`",
        check: generate::generate_label_prefix,
    },
    Rule {
        name: "interface-name-style",
        group: Group::Lowrisc,
        summary: "an interface not `lower_snake_case` ending in `_if`",
        check: names::interface_name_style,
    },
    Rule {
        name: "macro-name-style",
        group: Group::Lowrisc,
        summary: "a macro not `ALL_CAPS`, other than UVM's `uvm_` ones",
        check: names::macro_name_style,
    },
    Rule {
        name: "module-begin-block",
        group: Group::Lowrisc,
        summary: "a `begin` block directly in a module",
        check: generate::module_begin_block,
    },
    Rule {
        name: "module-parameter",
        group: Group::Lowrisc,
        summary: "an instance setting more than one parameter, some by position",
        check: instances::module_parameter,
    },
    Rule {
        name: "module-port",
        group: Group::Lowrisc,
        summary: "an instance connecting more than one port, some by position",
        check: instances::module_port,
    },
    Rule {
        name: "parameter-name-style",
        group: Group::Lowrisc,
        summary: "a parameter neither `CamelCase` nor `ALL_CAPS`",
        check: names::parameter_name_style,
    },
    Rule {
        name: "struct-union-name-style",
        group: Group::Lowrisc,
        summary: "a struct or union type not `lower_snake_case` ending in `_t`",
        check: names::struct_union_name_style,
    },
    Rule {
        name: "v2001-generate-begin",
        group: Group::Lowrisc,
        summary: "a `begin` block directly inside `generate`",
        check: generate::v2001_generate_begin,
    },
    Rule {
        name: "endif-comment",
        group: Group::Restriction,
        summary: "an `` `endif `` without a comment naming the `` `ifdef ``'s macro",
        check: preproc::endif_comment,
    },
    Rule {
        name: "explicit-begin",
        group: Group::Restriction,
        summary: "an `if`, `else`, loop or procedural block whose body has no `begin`",
        check: begin::explicit_begin,
    },
    Rule {
        name: "forbid-negative-array-dim",
        group: Group::Restriction,
        summary: "a negative literal bound in a dimension",
        check: items::forbid_negative_array_dim,
    },
    Rule {
        name: "invalid-system-task-function",
        group: Group::Restriction,
        summary: "`$random`, `$dist_*`, `$psprintf` or `$srandom`",
        check: tokens::invalid_system_task_function,
    },
    Rule {
        name: "legacy-generate-region",
        group: Group::Restriction,
        summary: "a `generate` ... `endgenerate` region",
        check: items::legacy_generate_region,
    },
    Rule {
        name: "legacy-genvar-declaration",
        group: Group::Restriction,
        summary: "a `genvar` declared apart from its loop",
        check: items::legacy_genvar_declaration,
    },
    Rule {
        name: "one-module-per-file",
        group: Group::Restriction,
        summary: "a second module in one file",
        check: items::one_module_per_file,
    },
    Rule {
        name: "proper-parameter-declaration",
        group: Group::Restriction,
        summary: "a `parameter` outside a parameter list, or a `localparam` outside a design element, class or package",
        check: items::proper_parameter_declaration,
    },
    Rule {
        name: "uvm-macro-semicolon",
        group: Group::Restriction,
        summary: "a `;` after a `` `uvm_ `` macro call",
        check: preproc::uvm_macro_semicolon,
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

    /// `node`'s range from its first token to its last, without the
    /// whitespace and comments the tree puts ahead of it.
    pub fn range(node: &SyntaxNode) -> TextRange {
        let mut tokens = (node.descendants_with_tokens())
            .filter_map(|element| element.into_token())
            .filter(|token| !token.kind().is_trivia())
            .map(|token| token.text_range());
        let first = tokens.next().unwrap_or(node.text_range());
        let last = tokens.last().unwrap_or(first);
        first.cover(last)
    }

    pub fn report(&mut self, diagnostic: Diagnostic) {
        self.found.push(diagnostic);
    }
}
