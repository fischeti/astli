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
//! replaces, is reported as [`INVALID_WAIVER`].
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
mod options;
mod rule;
mod rules;
mod waiver;
mod widths;

pub use config::{Config, Level, UnknownRule};
pub use options::{Case, OptionError, OptionKind, Pattern, RuleOption};
pub use rule::{Group, RULES, Rule};
pub use waiver::{INVALID_WAIVER, Waivers};
pub use widths::{Width, WidthKind, widths};

use astli_parse::SyntaxTree;
use astli_sema::{
    Access, Elaboration, FileId, Member, Resolution, SymbolKind, SymbolRef, accesses,
};
use astli_text::{Diagnostic, Origins, Severity};
use rayon::prelude::*;
use rowan::TextRange;
use rustc_hash::{FxHashMap, FxHashSet};

use rule::{BuiltCx, Check, Cx, DesignCx};

/// What the rules `config` leaves on find in `tree`, in the order of the
/// file.
///
/// A rule at [`Level::Warn`] reports a warning and one at [`Level::Deny`] an
/// error. What the parser found malformed is not repeated: that is
/// [`SyntaxTree::diagnostics`].
pub fn lint(tree: &SyntaxTree, config: &Config) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    let waivers = Waivers::read(tree, &mut found);
    let rules = RULES.iter().zip(config.levels()).zip(config.options());
    for ((rule, level), options) in rules {
        let (Some(severity), Check::Tree(check)) = (severity(*level), &rule.check) else {
            continue;
        };
        let mut hits = Vec::new();
        check(&mut Cx {
            tree,
            rule,
            options,
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

fn severity(level: Level) -> Option<Severity> {
    match level {
        Level::Allow => None,
        Level::Warn => Some(Severity::Warning),
        Level::Deny => Some(Severity::Error),
    }
}

/// The rules that read a design, over each of its files.
///
/// Made once per design, over its elaboration, since every file is
/// resolved once and what each spells gathered: a name another file reaches through a hierarchical
/// path is a use of the signal it resolves to, and one it spells in a
/// region sema does not see into, or reaches by a path that does not
/// resolve, may be a use of any signal so named.
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
    /// The package each package member is declared in.
    packages: FxHashMap<SymbolRef, Box<str>>,
}

impl<'d> Linter<'d> {
    pub fn new(elaboration: &'d Elaboration) -> Linter<'d> {
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
                used.1 |= access.write;
            }
        }
        let mut spelled: FxHashMap<Box<str>, Option<FileId>> = FxHashMap::default();
        let mut members = FxHashSet::default();
        let mut packages = FxHashMap::default();
        for (file, hir) in design.files() {
            let names = design.names(file);
            for (_, symbol) in hir.symbols() {
                let SymbolKind::Definition { keyword, scope, .. } = &symbol.kind else {
                    continue;
                };
                if *keyword != astli_syntax::SyntaxKind::PACKAGE_KW {
                    continue;
                }
                for member in &hir[*scope].members {
                    if let Member::Declare(id) = member {
                        let at = SymbolRef { file, symbol: *id };
                        packages.insert(at, symbol.name.text.clone());
                    }
                }
            }
            let mut spell = |name: &astli_sema::Name| {
                let entry = spelled.entry(name.text.clone()).or_insert(Some(file));
                if *entry != Some(file) {
                    *entry = None;
                }
            };
            for (id, expr) in hir.exprs() {
                match &expr.kind {
                    astli_sema::ExprKind::Member { name, .. }
                        if names.expr(id) == Some(Resolution::Unknown) =>
                    {
                        members.insert(name.text.clone());
                    }
                    astli_sema::ExprKind::Opaque(opaque) => {
                        opaque.names.iter().for_each(&mut spell)
                    }
                    _ => {}
                }
            }
            for (_, symbol) in hir.symbols() {
                if let astli_sema::SymbolKind::Other(opaque) = &symbol.kind {
                    opaque.names.iter().for_each(&mut spell);
                }
            }
            for (_, scope) in hir.scopes() {
                for member in &scope.members {
                    if let astli_sema::Member::Opaque(opaque) = member {
                        opaque.names.iter().for_each(&mut spell);
                    }
                }
            }
            for (_, stmt) in hir.stmts() {
                if let astli_sema::StmtKind::Opaque(opaque) = &stmt.kind {
                    opaque.names.iter().for_each(&mut spell);
                }
            }
        }
        Linter {
            elaboration,
            files,
            remote,
            spelled,
            members,
            packages,
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

    /// The package `symbol` is declared in, if it is a package's member.
    pub(crate) fn package_of(&self, symbol: SymbolRef) -> Option<&str> {
        self.packages.get(&symbol).map(|name| &**name)
    }

    /// What the design rules `config` leaves on find in `file`, whose spans
    /// `origins` resolves, skipping what `waivers`, read off the file's tree
    /// as written, waive.
    pub fn lint(
        &self,
        file: FileId,
        origins: &Origins,
        waivers: &Waivers,
        config: &Config,
    ) -> Vec<Diagnostic> {
        let names = self.elaboration.design().names(file);
        let accesses = &self.files[file.index()];
        let mut found = Vec::new();
        for (rule, level) in RULES.iter().zip(config.levels()) {
            let (Some(severity), Check::Design(check)) = (severity(*level), &rule.check) else {
                continue;
            };
            let mut hits = Vec::new();
            check(&mut DesignCx {
                linter: self,
                file,
                origins,
                names,
                accesses,
                elaboration: self.elaboration,
                rule,
                severity,
                found: &mut hits,
            });
            hits.retain(|hit| !waivers.cover_span(rule, origins, hit.at));
            found.extend(hits);
        }
        found
            .sort_by_key(|diagnostic| (diagnostic.at.src_id, diagnostic.at.start, diagnostic.code));
        found
    }

    /// What the rules that read what the tops build, and `config` leaves
    /// on, find in `file`: from `widths`, the file's of
    /// [`widths`].
    pub fn lint_built(
        &self,
        origins: &Origins,
        waivers: &Waivers,
        config: &Config,
        widths: &[Width],
    ) -> Vec<Diagnostic> {
        let mut found = Vec::new();
        for (rule, level) in RULES.iter().zip(config.levels()) {
            let (Some(severity), Check::Built(check)) = (severity(*level), &rule.check) else {
                continue;
            };
            let mut hits = Vec::new();
            check(&mut BuiltCx {
                origins,
                widths,
                rule,
                severity,
                found: &mut hits,
            });
            hits.retain(|hit| !waivers.cover_span(rule, origins, hit.at));
            found.extend(hits);
        }
        found
            .sort_by_key(|diagnostic| (diagnostic.at.src_id, diagnostic.at.start, diagnostic.code));
        found
    }
}
