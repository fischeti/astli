//! The rule table, and what a rule reports through.

use std::fmt;
use std::ops::Range;

use astli_parse::SyntaxTree;
use astli_sema::{Access, Design, Elaboration, FileId, Hir, Names, SymbolRef};
use astli_syntax::SyntaxNode;
use astli_text::{Code, Diagnostic, Origins, Severity, Span};
use rowan::TextRange;

use crate::options::{Case, OptionKind, Pattern, RuleOption, Style, Value};
use crate::rules::{
    begin, case, declarations, drivers, files, generate, instances, items, names, preproc,
    procedural, tokens, types, unused, widths,
};
use crate::{Level, Linter, Width};

/// Every rule, in the order `--list` prints them: by group, then by name.
pub static RULES: &[Rule] = &[
    Rule {
        name: "always-comb-blocking",
        group: Group::Correctness,
        summary: "a non-blocking assignment in `always_comb`",
        check: Check::Tree(procedural::always_comb_blocking),
        options: &[],
    },
    Rule {
        name: "always-ff-non-blocking",
        group: Group::Correctness,
        summary: "a blocking assignment in `always_ff` to a variable not declared in it",
        check: Check::Tree(procedural::always_ff_non_blocking),
        options: &[],
    },
    Rule {
        name: "duplicate-case-item",
        group: Group::Correctness,
        summary: "a `case` label written twice, so its second item is never taken",
        check: Check::Tree(case::duplicate_case_item),
        options: &[],
    },
    Rule {
        name: "always-comb",
        group: Group::Suspicious,
        summary: "`always @*` where `always_comb` would say what it is",
        check: Check::Tree(procedural::always_comb),
        options: &[],
    },
    Rule {
        name: "case-missing-default",
        group: Group::Suspicious,
        summary: "a `case` with no `default` item that is not `unique` or `unique0`",
        check: Check::Tree(case::case_missing_default),
        options: &[],
    },
    Rule {
        name: "forbid-defparam",
        group: Group::Suspicious,
        summary: "`defparam`, which overrides a parameter from elsewhere in the hierarchy",
        check: Check::Tree(tokens::forbid_defparam),
        options: &[],
    },
    Rule {
        name: "undriven-signal",
        group: Group::Suspicious,
        summary: "a signal of a module, interface or program read and never written, or an output never driven",
        check: Check::Design(drivers::undriven_signal),
        options: &[],
    },
    Rule {
        name: "unused-import",
        group: Group::Suspicious,
        summary: "an import nothing in the file uses",
        check: Check::Design(unused::unused_import),
        options: &[],
    },
    Rule {
        name: "unused-parameter",
        group: Group::Suspicious,
        summary: "a parameter of a module, interface or program nothing reads",
        check: Check::Design(unused::unused_parameter),
        options: &[],
    },
    Rule {
        name: "unused-signal",
        group: Group::Suspicious,
        summary: "a net, variable or input of a module, interface or program nothing reads",
        check: Check::Design(unused::unused_signal),
        options: &[],
    },
    Rule {
        name: "variable-initializer",
        group: Group::Suspicious,
        summary: "a variable in a module or interface given a value where it is declared",
        check: Check::Tree(declarations::variable_initializer),
        options: &[],
    },
    Rule {
        name: "width-expand",
        group: Group::Suspicious,
        summary: "a value extended to a wider target, port or operand, in what `--top` builds",
        check: Check::Built(widths::width_expand),
        options: &[],
    },
    Rule {
        name: "width-trunc",
        group: Group::Suspicious,
        summary: "a value truncated to a narrower target or port, in what `--top` builds",
        check: Check::Built(widths::width_trunc),
        options: &[],
    },
    Rule {
        name: "constraint-name-style",
        group: Group::Lowrisc,
        summary: "a constraint not `lower_snake_case` ending in `_c`",
        check: Check::Tree(names::constraint_name_style),
        options: &[cases(&["lower_snake_case"]), patterns(&["*_c"])],
    },
    Rule {
        name: "disable-statement",
        group: Group::Lowrisc,
        summary: "`disable` of a label that is no `begin` block around it, or is a process's whole body",
        check: Check::Tree(procedural::disable_statement),
        options: &[],
    },
    Rule {
        name: "enum-name-style",
        group: Group::Lowrisc,
        summary: "an enum type not `lower_snake_case` ending in `_e` or `_t`",
        check: Check::Tree(names::enum_name_style),
        options: &[cases(&["lower_snake_case"]), patterns(&["*_e", "*_t"])],
    },
    Rule {
        name: "explicit-function-lifetime",
        group: Group::Lowrisc,
        summary: "a function outside a class without `automatic` or `static`",
        check: Check::Tree(files::explicit_function_lifetime),
        options: &[],
    },
    Rule {
        name: "explicit-function-task-parameter-type",
        group: Group::Lowrisc,
        summary: "a function or task argument without a type",
        check: Check::Tree(types::explicit_function_task_parameter_type),
        options: &[],
    },
    Rule {
        name: "explicit-parameter-storage-type",
        group: Group::Lowrisc,
        summary: "a parameter without a type, other than one with a string value",
        check: Check::Tree(types::explicit_parameter_storage_type),
        options: &[],
    },
    Rule {
        name: "explicit-task-lifetime",
        group: Group::Lowrisc,
        summary: "a task outside a class without `automatic` or `static`",
        check: Check::Tree(files::explicit_task_lifetime),
        options: &[],
    },
    Rule {
        name: "forbid-reg",
        group: Group::Lowrisc,
        summary: "`reg`, where `logic` says the same",
        check: Check::Tree(tokens::forbid_reg),
        options: &[],
    },
    Rule {
        name: "forbid-wildcard-connection",
        group: Group::Lowrisc,
        summary: "an instance connecting its ports with `.*`",
        check: Check::Tree(instances::forbid_wildcard_connection),
        options: &[],
    },
    Rule {
        name: "generate-label",
        group: Group::Lowrisc,
        summary: "a generate block without a label",
        check: Check::Tree(generate::generate_label),
        options: &[],
    },
    Rule {
        name: "generate-label-prefix",
        group: Group::Lowrisc,
        summary: "a generate block label not starting with `gen_` or `g_`",
        check: Check::Tree(generate::generate_label_prefix),
        options: &[patterns(&["gen_*", "g_*"])],
    },
    Rule {
        name: "instance-name-style",
        group: Group::Lowrisc,
        summary: "an instance not `lower_snake_case`",
        check: Check::Tree(names::instance_name_style),
        options: &[cases(&["lower_snake_case"]), patterns(&["*"])],
    },
    Rule {
        name: "interface-name-style",
        group: Group::Lowrisc,
        summary: "an interface not `lower_snake_case` ending in `_if`",
        check: Check::Tree(names::interface_name_style),
        options: &[cases(&["lower_snake_case"]), patterns(&["*_if"])],
    },
    Rule {
        name: "macro-name-style",
        group: Group::Lowrisc,
        summary: "a macro not `ALL_CAPS`, other than UVM's `uvm_` ones",
        check: Check::Tree(names::macro_name_style),
        options: &[cases(&["ALL_CAPS"]), patterns(&["*"])],
    },
    Rule {
        name: "module-begin-block",
        group: Group::Lowrisc,
        summary: "a `begin` block directly in a module",
        check: Check::Tree(generate::module_begin_block),
        options: &[],
    },
    Rule {
        name: "module-filename",
        group: Group::Lowrisc,
        summary: "a file declaring modules, none named as the file is",
        check: Check::Tree(files::module_filename),
        options: &[],
    },
    Rule {
        name: "module-parameter",
        group: Group::Lowrisc,
        summary: "an instance setting more than one parameter, some by position",
        check: Check::Tree(instances::module_parameter),
        options: &[],
    },
    Rule {
        name: "module-port",
        group: Group::Lowrisc,
        summary: "an instance connecting more than one port, some by position",
        check: Check::Tree(instances::module_port),
        options: &[],
    },
    Rule {
        name: "package-filename",
        group: Group::Lowrisc,
        summary: "a package not named as its file is",
        check: Check::Tree(files::package_filename),
        options: &[],
    },
    Rule {
        name: "packed-dimensions-range-ordering",
        group: Group::Lowrisc,
        summary: "a packed range in ascending order, `[0:7]`",
        check: Check::Tree(types::packed_dimensions_range_ordering),
        options: &[],
    },
    Rule {
        name: "parameter-name-style",
        group: Group::Lowrisc,
        summary: "a parameter neither `CamelCase` nor `ALL_CAPS`",
        check: Check::Tree(names::parameter_name_style),
        options: &[cases(&["CamelCase", "ALL_CAPS"]), patterns(&["*"])],
    },
    Rule {
        name: "port-name-suffix",
        group: Group::Lowrisc,
        summary: "a module's port not ending in `_i`, `_o` or `_io` as its direction says",
        check: Check::Tree(names::port_name_suffix),
        options: &[
            directed("input", &["*_i", "*_ni", "*_pi"]),
            directed("output", &["*_o", "*_no", "*_po"]),
            directed("inout", &["*_io", "*_nio", "*_pio"]),
        ],
    },
    Rule {
        name: "positive-meaning-parameter-name",
        group: Group::Lowrisc,
        summary: "a parameter named `Disable...`",
        check: Check::Tree(names::positive_meaning_parameter_name),
        options: &[],
    },
    Rule {
        name: "signal-name-style",
        group: Group::Lowrisc,
        summary: "a net, variable or port of a design element not `lower_snake_case`",
        check: Check::Tree(names::signal_name_style),
        options: &[cases(&["lower_snake_case"]), patterns(&["*"])],
    },
    Rule {
        name: "struct-union-name-style",
        group: Group::Lowrisc,
        summary: "a struct or union type not `lower_snake_case` ending in `_t`",
        check: Check::Tree(names::struct_union_name_style),
        options: &[cases(&["lower_snake_case"]), patterns(&["*_t"])],
    },
    Rule {
        name: "typedef-enums",
        group: Group::Lowrisc,
        summary: "an `enum` without a `typedef`",
        check: Check::Tree(types::typedef_enums),
        options: &[],
    },
    Rule {
        name: "unpacked-dimensions-range-ordering",
        group: Group::Lowrisc,
        summary: "an unpacked range in descending order, `[7:0]`",
        check: Check::Tree(types::unpacked_dimensions_range_ordering),
        options: &[],
    },
    Rule {
        name: "v2001-generate-begin",
        group: Group::Lowrisc,
        summary: "a `begin` block directly inside `generate`",
        check: Check::Tree(generate::v2001_generate_begin),
        options: &[],
    },
    Rule {
        name: "endif-comment",
        group: Group::Restriction,
        summary: "an `` `endif `` without a comment naming the `` `ifdef ``'s macro",
        check: Check::Tree(preproc::endif_comment),
        options: &[],
    },
    Rule {
        name: "explicit-begin",
        group: Group::Restriction,
        summary: "an `if`, `else`, loop or procedural block whose body has no `begin`",
        check: Check::Tree(begin::explicit_begin),
        options: &[],
    },
    Rule {
        name: "forbid-negative-array-dim",
        group: Group::Restriction,
        summary: "a negative literal bound in a dimension",
        check: Check::Tree(items::forbid_negative_array_dim),
        options: &[],
    },
    Rule {
        name: "invalid-system-task-function",
        group: Group::Restriction,
        summary: "`$random`, `$dist_*`, `$psprintf` or `$srandom`",
        check: Check::Tree(tokens::invalid_system_task_function),
        options: &[],
    },
    Rule {
        name: "legacy-generate-region",
        group: Group::Restriction,
        summary: "a `generate` ... `endgenerate` region",
        check: Check::Tree(items::legacy_generate_region),
        options: &[],
    },
    Rule {
        name: "legacy-genvar-declaration",
        group: Group::Restriction,
        summary: "a `genvar` declared apart from its loop",
        check: Check::Tree(items::legacy_genvar_declaration),
        options: &[],
    },
    Rule {
        name: "one-module-per-file",
        group: Group::Restriction,
        summary: "a second module in one file",
        check: Check::Tree(items::one_module_per_file),
        options: &[],
    },
    Rule {
        name: "proper-parameter-declaration",
        group: Group::Restriction,
        summary: "a `parameter` outside a parameter list, or a `localparam` outside a design element, class or package",
        check: Check::Tree(items::proper_parameter_declaration),
        options: &[],
    },
    Rule {
        name: "uvm-macro-semicolon",
        group: Group::Restriction,
        summary: "a `;` after a `` `uvm_ `` macro call",
        check: Check::Tree(preproc::uvm_macro_semicolon),
        options: &[],
    },
];

/// One lint rule.
pub struct Rule {
    /// The name diagnostics carry as their code, and configuration uses.
    pub name: &'static str,
    pub group: Group,
    /// What the rule finds, in a phrase.
    pub summary: &'static str,
    pub(crate) check: Check,
    /// What a project may set of it beyond its level.
    pub options: &'static [RuleOption],
}

/// The cases a naming rule's `*` may be in.
const fn cases(default: &'static [&'static str]) -> RuleOption {
    RuleOption {
        key: "case",
        kind: OptionKind::Cases,
        default,
        summary: "the cases a name may be in",
    }
}

/// The patterns a naming rule holds a name to.
const fn patterns(default: &'static [&'static str]) -> RuleOption {
    RuleOption {
        key: "patterns",
        kind: OptionKind::Patterns,
        default,
        summary: "the patterns a name may match, `*` for the rest of it",
    }
}

/// The patterns a port of one direction is named by.
const fn directed(key: &'static str, default: &'static [&'static str]) -> RuleOption {
    RuleOption {
        key,
        kind: OptionKind::Patterns,
        default,
        summary: "the patterns a port of that direction may match",
    }
}

impl Rule {
    /// Whether the rule reads a design, names resolved across its files,
    /// rather than one file's tree.
    pub fn needs_design(&self) -> bool {
        matches!(self.check, Check::Design(_) | Check::Built(_))
    }

    /// Whether the rule reads what tops build, and so runs only given them.
    pub fn needs_tops(&self) -> bool {
        matches!(self.check, Check::Built(_))
    }
}

/// What a rule reads: one file's tree as written, or a file of a design,
/// expanded and lowered with names resolved.
pub(crate) enum Check {
    Tree(fn(&mut Cx)),
    Design(fn(&mut DesignCx)),
    /// What the tops build, elaborated.
    Built(fn(&mut BuiltCx)),
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
    /// The rule's options, in the order it declares them.
    pub options: &'a [Value],
    pub severity: Severity,
    pub found: &'a mut Vec<Diagnostic>,
}

/// What one rule reads and reports into, over one file of a design.
pub(crate) struct DesignCx<'a, 'd> {
    pub linter: &'a Linter<'d>,
    pub file: FileId,
    pub origins: &'a Origins,
    pub names: &'a Names,
    pub accesses: &'a [Access],
    /// The design, each definition at its parameters' defaults.
    pub elaboration: &'d Elaboration,
    pub rule: &'static Rule,
    pub severity: Severity,
    pub found: &'a mut Vec<Diagnostic>,
}

/// What a rule reading what the tops build sees of one file.
pub(crate) struct BuiltCx<'a> {
    pub origins: &'a Origins,
    /// The widths that differ in the file, in what is built.
    pub widths: &'a [Width],
    pub rule: &'static Rule,
    pub severity: Severity,
    pub found: &'a mut Vec<Diagnostic>,
}

impl BuiltCx<'_> {
    pub fn report(&mut self, at: Span, message: impl Into<String>) -> &mut Diagnostic {
        let diagnostic = Diagnostic::new(self.severity, Code(self.rule.name), at, message);
        self.found.push(diagnostic);
        self.found.last_mut().expect("just pushed")
    }
}

impl<'a, 'd> DesignCx<'a, 'd> {
    pub fn design(&self) -> &'d Design {
        self.elaboration.design()
    }

    pub fn hir(&self) -> &'d Hir {
        &self.elaboration.design()[self.file]
    }

    /// Whether a macro wrote what is at `at`, which its user can change only
    /// in the macro.
    pub fn written_by_macro(&self, at: Span) -> bool {
        self.origins.placed_by(at.src_id).is_some()
    }

    pub fn report(&mut self, at: Span, message: impl Into<String>) -> &mut Diagnostic {
        let diagnostic = Diagnostic::new(self.severity, Code(self.rule.name), at, message);
        self.found.push(diagnostic);
        self.found.last_mut().expect("just pushed")
    }

    /// The bits of its symbol a partial access reaches, at the defaults of
    /// the definition it stands in; `None` for one to all of it, or where
    /// sema cannot evaluate its select.
    pub fn reach(&self, access: &Access) -> Option<Range<u64>> {
        let (place, within) = (access.place?, access.within?);
        let body = self.elaboration.body(SymbolRef {
            file: self.file,
            symbol: within,
        })?;
        self.elaboration.reach(body, place)
    }
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

    /// The patterns the option `key` holds.
    pub fn patterns(&self, key: &str) -> &'a [Pattern] {
        match self.option(key) {
            Value::Patterns(patterns) => patterns,
            Value::Cases(_) => panic!("`{key}` of `{}` holds cases", self.rule.name),
        }
    }

    /// The style a naming rule's `case` and `patterns` say.
    pub fn style(&self) -> Style<'a> {
        let cases: &'a [Case] = match self.option("case") {
            Value::Cases(cases) => cases,
            Value::Patterns(_) => panic!("`case` of `{}` holds patterns", self.rule.name),
        };
        Style {
            cases,
            patterns: self.patterns("patterns"),
        }
    }

    fn option(&self, key: &str) -> &'a Value {
        let at = (self.rule.options.iter())
            .position(|it| it.key == key)
            .unwrap_or_else(|| panic!("`{}` declares no option `{key}`", self.rule.name));
        &self.options[at]
    }
}
