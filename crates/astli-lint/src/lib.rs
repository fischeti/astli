//! SystemVerilog lint rules over one file's syntax tree.
//!
//! [`lint`] runs every rule a [`Config`] leaves on over a [`SyntaxTree`],
//! skipping what the file's [`Waivers`] cover, and returns what they found
//! as diagnostics, each coded with its rule's name:
//!
//! ```
//! use astli_lint::{Config, Waivers, lint};
//! use astli_parse::SyntaxTree;
//!
//! let source = "module top;\n  always_ff @(posedge clk_i) q = d;\nendmodule\n";
//! let tree = SyntaxTree::parse("top.sv", source.to_string());
//! let (waivers, _) = Waivers::read(&tree);
//!
//! let [found] = lint(&tree, &waivers, &Config::default()).try_into().unwrap();
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
//! A rule may take options beyond its level, each a [`RuleOption`] it
//! declares with its default: a naming rule, the cases and [`Pattern`]s a
//! name may be in. [`Config::set_option`] sets one as configuration spells
//! it, so that `port-name-suffix` takes `i_*` where lowRISC writes `*_i`.
//!
//! # Waivers
//!
//! `(* astli_allow = "rule, group" *)` on a construct turns those rules off
//! inside it: on a statement, a procedural block, a declaration, an
//! instance, a module. A name that is neither a rule's nor a group's, a
//! value that is not a string, or a waiver a later one on the same construct
//! replaces, is reported as [`INVALID_WAIVER`] by [`Waivers::read`]. A file's
//! waivers are read once, and waive both its tree rules and, read off the
//! file as written, the design rules' findings in it.
//!
//! ```
//! use astli_lint::{Config, Waivers, lint};
//! use astli_parse::SyntaxTree;
//!
//! let source = r#"module top;
//!   (* astli_allow = "always-ff-non-blocking" *)
//!   always_ff @(posedge clk_i) q = d;
//! endmodule
//! "#;
//! let tree = SyntaxTree::parse("top.sv", source.to_string());
//! let (waivers, invalid) = Waivers::read(&tree);
//! assert!(invalid.is_empty());
//! assert!(lint(&tree, &waivers, &Config::default()).is_empty());
//! ```

mod config;
mod nodes;
mod options;
mod rule;
mod rules;
mod waiver;
mod widths;

pub use config::{Config, Level, UnknownRule};
pub use options::{Case, OptionError, OptionKind, Pattern, RuleOption};
pub use rule::{Group, Layer, RULES, Rule};
pub use waiver::{INVALID_WAIVER, Waivers};

use astli_parse::SyntaxTree;
use astli_sema::{Access, Built, Elaboration, FileId, Resolution, SymbolRef, accesses};
use astli_text::{Diagnostic, Origins, Severity};
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};

use nodes::Nodes;
use options::Options;
use rule::{Check, Cx, DesignCx};
use widths::{Width, widths};

/// What the rules `config` leaves on find in `tree` that its `waivers` do
/// not cover, in the order of the file.
///
/// A rule at [`Level::Warn`] reports a warning and one at [`Level::Deny`] an
/// error. What the parser found malformed is not repeated: that is
/// [`SyntaxTree::diagnostics`]; nor is a malformed waiver, which
/// [`Waivers::read`] reports.
pub fn lint(tree: &SyntaxTree, waivers: &Waivers, config: &Config) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    let nodes = Nodes::new(tree.root());
    run(
        config,
        tree.origins(),
        waivers,
        &mut found,
        |options, severity, found| {
            if let Check::Tree(check) = options.rule.check {
                check(&mut Cx {
                    tree,
                    nodes: &nodes,
                    options,
                    severity,
                    found,
                });
            }
        },
    );
    found
}

/// Runs `check` for each rule `config` leaves on, given the rule with its
/// options and the severity its level gives, and adds to `found` what it
/// reports that `waivers` do not cover where `origins` places it; then sorts
/// `found` by place. `check` runs the rule only if it reads what the caller
/// has.
fn run(
    config: &Config,
    origins: &Origins,
    waivers: &Waivers,
    found: &mut Vec<Diagnostic>,
    mut check: impl FnMut(Options<'_>, Severity, &mut Vec<Diagnostic>),
) {
    let rules = RULES.iter().zip(config.levels()).zip(config.options());
    for ((rule, level), values) in rules {
        let severity = match level {
            Level::Allow => continue,
            Level::Warn => Severity::Warning,
            Level::Deny => Severity::Error,
        };
        let mut hits = Vec::new();
        check(Options { rule, values }, severity, &mut hits);
        hits.retain(|hit| !waivers.cover(rule, origins, hit.at));
        found.extend(hits);
    }
    found.sort_by_key(|diagnostic| (diagnostic.at.src_id, diagnostic.at.start, diagnostic.code));
}

/// The rules that read a design, and what its tops build, over each of its
/// files.
///
/// Made once per design, over its elaboration, since every file is
/// resolved once and what each spells gathered, and what is built walked
/// once: a name another file reaches through a hierarchical path is a use
/// of the signal it resolves to, and one it spells in a region sema does
/// not see into, or reaches by a path that does not resolve, may be a use
/// of any signal so named.
pub struct Linter<'d> {
    /// The design, each definition at its parameters' defaults.
    elaboration: &'d Elaboration,
    /// What each file accesses.
    files: Vec<Vec<Access>>,
    /// Each symbol a file other than its own reads or writes, through a
    /// hierarchical name: whether one reads it, and whether one writes it.
    remote: FxHashMap<SymbolRef, (bool, bool)>,
    /// Each name an opaque region spells, and the file that first does, or
    /// `None` once a second one has.
    spelled: FxHashMap<Box<str>, Option<FileId>>,
    /// Each name any file accesses as a member of a path that does not
    /// resolve, `up.name`, which may be one through the instance tree.
    members: FxHashSet<Box<str>>,
    /// The widths that differ in each file, in what the tops build; `None`
    /// without them.
    widths: Option<Vec<Vec<Width>>>,
}

impl<'d> Linter<'d> {
    /// The linter of `elaboration`, whose built rules read what `built`
    /// builds of it, and without it run not at all.
    pub fn new(elaboration: &'d Elaboration, built: Option<&Built>) -> Linter<'d> {
        let design = elaboration.design();
        let ids: Vec<FileId> = design.files().map(|(file, _)| file).collect();
        let files: Vec<Vec<Access>> = (ids.par_iter())
            .map(|&file| accesses(design, file, design.names(file)))
            .collect();
        let mut remote: FxHashMap<SymbolRef, (bool, bool)> = FxHashMap::default();
        for (&file, accesses) in ids.iter().zip(&files) {
            for access in accesses.iter().filter(|it| it.symbol.file != file) {
                let used = remote.entry(access.symbol).or_default();
                used.0 |= access.read;
                used.1 |= access.write.is_some();
            }
        }
        let mut spelled: FxHashMap<Box<str>, Option<FileId>> = FxHashMap::default();
        let mut members = FxHashSet::default();
        for (file, hir) in design.files() {
            let names = design.names(file);
            for (id, expr) in hir.exprs() {
                if let astli_sema::ExprKind::Member { name, .. } = &expr.kind
                    && names.expr(id) == Some(Resolution::Unknown)
                {
                    members.insert(name.text.clone());
                }
            }
            for &id in names.loose() {
                if let astli_sema::ExprKind::Loose(name) = &hir[id].kind {
                    let first = spelled.entry(name.clone()).or_insert(Some(file));
                    if *first != Some(file) {
                        *first = None;
                    }
                }
            }
        }
        let widths = built.map(|built| {
            let mut by_file = vec![Vec::new(); ids.len()];
            for width in widths(elaboration, built) {
                by_file[width.file.index()].push(width);
            }
            by_file
        });
        Linter {
            elaboration,
            files,
            remote,
            spelled,
            members,
            widths,
        }
    }

    /// Whether a file other than `file` spells `name` where sema cannot
    /// tell what it names, or any file names it as a member of a path that
    /// does not resolve: either may be a use of a symbol of `file` that
    /// resolution does not find.
    pub(crate) fn maybe_used(&self, file: FileId, name: &str) -> bool {
        self.members.contains(name)
            || self
                .spelled
                .get(name)
                .is_some_and(|first| *first != Some(file))
    }

    /// Whether a file other than `symbol`'s reads it, and whether one
    /// writes it, through a hierarchical name.
    pub(crate) fn remote(&self, symbol: SymbolRef) -> (bool, bool) {
        self.remote.get(&symbol).copied().unwrap_or_default()
    }

    /// What the design and built rules `config` leaves on find in `file`,
    /// whose spans `origins` resolves, skipping what `waivers`, read off the
    /// file's tree as written, waive.
    pub fn lint(
        &self,
        file: FileId,
        origins: &Origins,
        waivers: &Waivers,
        config: &Config,
    ) -> Vec<Diagnostic> {
        let names = self.elaboration.design().names(file);
        let accesses = &self.files[file.index()];
        let widths = (self.widths.as_ref()).map(|widths| &*widths[file.index()]);
        let mut found = Vec::new();
        run(
            config,
            origins,
            waivers,
            &mut found,
            |options, severity, found| {
                let check = match options.rule.check {
                    Check::Design(check) => check,
                    Check::Built(check) if widths.is_some() => check,
                    _ => return,
                };
                check(&mut DesignCx {
                    linter: self,
                    file,
                    origins,
                    names,
                    accesses,
                    elaboration: self.elaboration,
                    widths: widths.unwrap_or_default(),
                    options,
                    severity,
                    found,
                });
            },
        );
        found
    }
}
