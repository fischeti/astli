//! Classifies every conditional region in the corpus as self-delimiting or
//! ragged, to test the assumption `docs/preprocessor.md` rests on.
//!
//! A region is **self-delimiting** when each of its branches balances on its
//! own -- every `(`, `begin`, `case`, `fork` it opens, it also closes. Such a
//! region can be parsed branch by branch in the enclosing context, and the
//! formatter can lay all branches out normally. A **ragged** region hands a
//! delimiter across a branch boundary, and has to be frozen verbatim:
//!
//!     `ifdef SYNTHESIS
//!       always_comb begin
//!     `else
//!       always_ff @(posedge clk) begin
//!     `endif
//!
//!     cargo run --release --example conditionals
//!     cargo run --release --example conditionals -- --list
//!
//! # Twice over
//!
//! Every region is measured two ways, because the cheap way cannot see
//! everything.
//!
//! **By token.** A macro that expands to a delimiter -- `` `MY_BEGIN `` -- is
//! one opaque token, so a region raggedly split by one counts as
//! self-delimiting. This was the only measurement available before the
//! preprocessor existed, and it is a lower bound on raggedness.
//!
//! **By expansion.** Each branch is expanded before it is counted, so a
//! delimiter a macro supplies is a delimiter. Two things change with it: a
//! macro defined in a header this file includes is still opaque unless the
//! branch pulls the header in itself, and a *nested* region resolves against
//! the file's own definitions rather than being read as its first branch. The
//! second is the more honest reading -- a nested region is one branch in any
//! given build -- but it is a different question, so both columns are printed
//! rather than one being called the answer.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use svirig_syntax::preproc::{
    Branch, Includes, Input, Region, Taken, TokenSpan, expand_span, regions, scan,
};
use svirig_syntax::{SyntaxKind as K, tokenize};
use svirig_text::Origins;

/// A pair of tokens that must nest, and how sure we are that the opener really
/// opens something. The `Structural` families are unambiguous; the others have
/// prototype forms with no closer (`extern function foo();`) and are reported
/// separately rather than counted against a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Family {
    Paren,
    Brack,
    Brace,
    Begin,
    Case,
    Fork,
    Module,
    Generate,
    // Below here: reported, not counted.
    Function,
    Task,
    Class,
    Package,
    Interface,
    Property,
    Sequence,
    Covergroup,
    Clocking,
    Checker,
    Program,
    Primitive,
    Specify,
    Config,
    Table,
}

const STRUCTURAL: usize = 8;

impl Family {
    fn is_structural(self) -> bool {
        (self as usize) < STRUCTURAL
    }

    fn name(self) -> &'static str {
        match self {
            Family::Paren => "()",
            Family::Brack => "[]",
            Family::Brace => "{}",
            Family::Begin => "begin/end",
            Family::Case => "case/endcase",
            Family::Fork => "fork/join",
            Family::Module => "module/endmodule",
            Family::Generate => "generate/endgenerate",
            Family::Function => "function/endfunction",
            Family::Task => "task/endtask",
            Family::Class => "class/endclass",
            Family::Package => "package/endpackage",
            Family::Interface => "interface/endinterface",
            Family::Property => "property/endproperty",
            Family::Sequence => "sequence/endsequence",
            Family::Covergroup => "covergroup/endgroup",
            Family::Clocking => "clocking/endclocking",
            Family::Checker => "checker/endchecker",
            Family::Program => "program/endprogram",
            Family::Primitive => "primitive/endprimitive",
            Family::Specify => "specify/endspecify",
            Family::Config => "config/endconfig",
            Family::Table => "table/endtable",
        }
    }
}

const FAMILIES: usize = 23;

/// Net opener-minus-closer count per family over a stretch of tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Delta([i32; FAMILIES]);

impl Delta {
    fn bump(&mut self, family: Family, by: i32) {
        self.0[family as usize] += by;
    }

    fn is_flat(&self, structural_only: bool) -> bool {
        self.0
            .iter()
            .enumerate()
            .filter(|(i, _)| !structural_only || *i < STRUCTURAL)
            .all(|(_, &value)| value == 0)
    }

    /// Which families are out of balance, and by how much.
    fn offenders(&self, structural_only: bool) -> Vec<(Family, i32)> {
        ALL_FAMILIES
            .iter()
            .copied()
            .filter(|f| !structural_only || f.is_structural())
            .filter_map(|f| {
                let value = self.0[f as usize];
                (value != 0).then_some((f, value))
            })
            .collect()
    }
}

const ALL_FAMILIES: [Family; FAMILIES] = [
    Family::Paren,
    Family::Brack,
    Family::Brace,
    Family::Begin,
    Family::Case,
    Family::Fork,
    Family::Module,
    Family::Generate,
    Family::Function,
    Family::Task,
    Family::Class,
    Family::Package,
    Family::Interface,
    Family::Property,
    Family::Sequence,
    Family::Covergroup,
    Family::Clocking,
    Family::Checker,
    Family::Program,
    Family::Primitive,
    Family::Specify,
    Family::Config,
    Family::Table,
];

/// The last significant kinds seen, most recent first. Enough context to tell
/// a declaration from a prototype or a use.
type Lookback = [K; 3];

fn opener(kind: K, back: &Lookback, next: K) -> Option<(Family, i32)> {
    let prototype = back
        .iter()
        .any(|k| matches!(k, K::EXTERN_KW | K::PURE_KW | K::IMPORT_KW | K::EXPORT_KW));

    Some(match kind {
        K::L_PAREN => (Family::Paren, 1),
        K::R_PAREN => (Family::Paren, -1),
        K::L_BRACK => (Family::Brack, 1),
        K::R_BRACK => (Family::Brack, -1),
        // `'{` opens an assignment pattern and is closed by a plain `}`.
        K::L_BRACE | K::APOSTROPHE_L_BRACE => (Family::Brace, 1),
        K::R_BRACE => (Family::Brace, -1),
        K::BEGIN_KW => (Family::Begin, 1),
        K::END_KW => (Family::Begin, -1),
        K::CASE_KW | K::CASEX_KW | K::CASEZ_KW | K::RANDCASE_KW => (Family::Case, 1),
        K::ENDCASE_KW => (Family::Case, -1),
        // `disable fork` and `wait fork` are uses, not blocks.
        K::FORK_KW if !matches!(back[0], K::DISABLE_KW | K::WAIT_KW) => (Family::Fork, 1),
        K::JOIN_KW | K::JOIN_ANY_KW | K::JOIN_NONE_KW => (Family::Fork, -1),
        K::MODULE_KW | K::MACROMODULE_KW => (Family::Module, 1),
        K::ENDMODULE_KW => (Family::Module, -1),
        K::GENERATE_KW => (Family::Generate, 1),
        K::ENDGENERATE_KW => (Family::Generate, -1),

        // `covergroup ... with function sample(...)` names a method, and
        // closes with nothing.
        K::FUNCTION_KW if !prototype && back[0] != K::WITH_KW => (Family::Function, 1),
        K::ENDFUNCTION_KW => (Family::Function, -1),
        K::TASK_KW if !prototype => (Family::Task, 1),
        K::ENDTASK_KW => (Family::Task, -1),
        // `typedef class foo;` is a forward declaration.
        K::CLASS_KW if back[0] != K::TYPEDEF_KW => (Family::Class, 1),
        K::ENDCLASS_KW => (Family::Class, -1),
        K::PACKAGE_KW => (Family::Package, 1),
        K::ENDPACKAGE_KW => (Family::Package, -1),
        // An interface *port* (`interface.modport p`) or a virtual interface
        // handle declares nothing.
        K::INTERFACE_KW if next != K::DOT && back[0] != K::VIRTUAL_KW => (Family::Interface, 1),
        K::ENDINTERFACE_KW => (Family::Interface, -1),
        // `assert property (...)` uses the keyword without declaring one.
        K::PROPERTY_KW if next != K::L_PAREN => (Family::Property, 1),
        K::ENDPROPERTY_KW => (Family::Property, -1),
        K::SEQUENCE_KW if next != K::L_PAREN => (Family::Sequence, 1),
        K::ENDSEQUENCE_KW => (Family::Sequence, -1),
        K::COVERGROUP_KW => (Family::Covergroup, 1),
        K::ENDGROUP_KW => (Family::Covergroup, -1),
        K::CLOCKING_KW => (Family::Clocking, 1),
        K::ENDCLOCKING_KW => (Family::Clocking, -1),
        K::CHECKER_KW => (Family::Checker, 1),
        K::ENDCHECKER_KW => (Family::Checker, -1),
        K::PROGRAM_KW => (Family::Program, 1),
        K::ENDPROGRAM_KW => (Family::Program, -1),
        K::PRIMITIVE_KW => (Family::Primitive, 1),
        K::ENDPRIMITIVE_KW => (Family::Primitive, -1),
        K::SPECIFY_KW => (Family::Specify, 1),
        K::ENDSPECIFY_KW => (Family::Specify, -1),
        K::CONFIG_KW => (Family::Config, 1),
        K::ENDCONFIG_KW => (Family::Config, -1),
        K::TABLE_KW => (Family::Table, 1),
        K::ENDTABLE_KW => (Family::Table, -1),
        _ => return None,
    })
}

/// One region, measured both ways.
#[derive(Debug, Clone)]
struct Measured {
    file: PathBuf,
    line: u32,
    /// One delta per branch, counting tokens.
    by_token: Vec<Delta>,
    /// The same, counting what the branch expands to.
    by_expansion: Vec<Delta>,
    has_else: bool,
    nested: usize,
}

impl Measured {
    /// Self-delimiting: every branch balances on its own.
    fn is_self_delimiting(&self, branches: &[Delta], structural_only: bool) -> bool {
        branches.iter().all(|delta| delta.is_flat(structural_only))
    }

    /// Ragged regions come in two shapes, and they are worth telling apart:
    /// branches that disagree with each other cannot share a parse at all,
    /// while branches that agree on the same non-zero delta are handing one
    /// delimiter to the code after `` `endif ``.
    fn branches_disagree(&self) -> bool {
        self.by_token.windows(2).any(|pair| pair[0] != pair[1])
    }
}

/// Measures every conditional region in one file.
fn measure(path: &Path, source: String, out: &mut Vec<Measured>) {
    let mut origins = Origins::new();
    let file = origins.add_file(path, source);
    // Held apart from the store, so that expanding a branch can write to it.
    let source = origins.text(file).to_string();
    let tokens = tokenize(&source);
    let input = Input::new(file, &source, &tokens);

    let found = scan(&input);
    // Every directive's extent, so that a `` `define `` body -- which is
    // substitution text, not code -- contributes no delimiters here.
    let directives: Vec<TokenSpan> = found.directives().map(|d| d.tokens).collect();

    let mut flat = Vec::new();
    collect(&input, input.span(0..input.len()), &mut flat);

    for region in flat {
        let by_token: Vec<Delta> = branches(&region)
            .map(|branch| {
                let mut kinds = Vec::new();
                unexpanded(&input, &directives, branch.body, &mut kinds);
                delta(&kinds)
            })
            .collect();
        let nested = branches(&region)
            .map(|branch| regions(&input, branch.body).len())
            .sum();
        let line = origins
            .line_col(file, region.tokens.bytes(&tokens).start)
            .line;

        let by_expansion: Vec<Delta> = branches(&region)
            .map(|branch| {
                let expanded = expand_span(
                    &mut origins,
                    branch.body,
                    found.macros.clone(),
                    &Includes::new(),
                );
                let kinds: Vec<K> = expanded.iter().map(|token| token.kind).collect();
                delta(&kinds)
            })
            .collect();

        out.push(Measured {
            file: path.to_path_buf(),
            line,
            by_token,
            by_expansion,
            has_else: region.has_else(),
            nested,
        });
    }
}

/// Each branch of a region, plus the empty one an absent `` `else `` leaves.
///
/// That branch is a real branch: a region that opens a `begin` in every branch
/// it *writes* still disagrees with taking neither.
fn branches(region: &Region) -> impl Iterator<Item = Branch> + '_ {
    let implied = (!region.has_else()).then(|| Branch {
        taken: Taken::Otherwise,
        directive: region.tokens,
        body: TokenSpan::empty(region.tokens.file, region.tokens.end),
    });
    region.branches.iter().copied().chain(implied)
}

/// Every region under `span`, nested ones included, in source order.
fn collect(input: &Input, span: TokenSpan, out: &mut Vec<Region>) {
    for region in regions(input, span) {
        for branch in &region.branches {
            collect(input, branch.body, out);
        }
        out.push(region);
    }
}

/// The kinds a branch contributes when nothing is expanded.
///
/// A nested region is read as its first branch: counting every branch of one
/// would count code that never coexists. Directives contribute nothing --
/// their operands are theirs, and a `` `define `` body is text.
fn unexpanded(input: &Input, directives: &[TokenSpan], span: TokenSpan, out: &mut Vec<K>) {
    let mut cursor = span.start;
    while cursor < span.end {
        if let Some(region) = regions(input, input.span(cursor..span.end))
            .first()
            .filter(|region| region.tokens.start == cursor)
        {
            unexpanded(input, directives, region.branches[0].body, out);
            cursor = region.tokens.end.max(cursor + 1);
            continue;
        }
        match directives
            .iter()
            .find(|directive| directive.start == cursor)
        {
            Some(directive) => cursor = directive.end.max(cursor + 1),
            None => {
                out.push(input.kind(cursor));
                cursor += 1;
            }
        }
    }
}

/// The net delimiter balance of a run of tokens.
fn delta(kinds: &[K]) -> Delta {
    let mut out = Delta::default();
    let mut back: Lookback = [K::EOF; 3];

    for (at, &kind) in kinds.iter().enumerate() {
        if kind.is_trivia() || kind == K::LINE_CONTINUATION || kind == K::EOF {
            continue;
        }
        let next = kinds[at + 1..]
            .iter()
            .copied()
            .find(|kind| !kind.is_trivia() && *kind != K::LINE_CONTINUATION)
            .unwrap_or(K::EOF);
        if let Some((family, by)) = opener(kind, &back, next) {
            out.bump(family, by);
        }
        back = [kind, back[0], back[1]];
    }
    out
}

fn main() {
    let list_all = std::env::args().any(|arg| arg == "--list");
    let corpus = Path::new("corpus");
    if !corpus.is_dir() {
        eprintln!("no corpus/ -- run scripts/fetch-corpus.sh");
        std::process::exit(1);
    }

    let mut repos: Vec<PathBuf> = std::fs::read_dir(corpus)
        .expect("corpus/")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    repos.sort();

    // cva6 vendors common_cells, so the same file appears more than once.
    // Pooled numbers would count it twice; per-repo numbers should not lose it.
    let mut seen: HashSet<u64> = HashSet::new();
    let mut all: Vec<Measured> = Vec::new();
    let mut duplicates = 0usize;

    println!(
        "{:<16} {:>7} {:>9} {:>11} {:>13}",
        "repo", "files", "regions", "by token", "by expansion"
    );
    println!("{}", "-".repeat(60));

    for repo in &repos {
        let name = repo.file_name().unwrap().to_string_lossy().to_string();
        let mut here: Vec<Measured> = Vec::new();
        let mut files = 0usize;

        for path in walk(repo) {
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            files += 1;
            let mut hasher = DefaultHasher::new();
            source.hash(&mut hasher);
            let digest = hasher.finish();

            let mut regions = Vec::new();
            measure(&path, source, &mut regions);

            if seen.insert(digest) {
                all.extend(regions.iter().cloned());
            } else {
                duplicates += 1;
            }
            here.extend(regions);
        }

        println!(
            "{name:<16} {files:>7} {:>9} {:>10.1}% {:>12.1}%",
            here.len(),
            share(&here, |m| &m.by_token),
            share(&here, |m| &m.by_expansion),
        );
    }

    let ragged: Vec<&Measured> = all
        .iter()
        .filter(|m| !m.is_self_delimiting(&m.by_token, true))
        .collect();
    let ragged_expanded: Vec<&Measured> = all
        .iter()
        .filter(|m| !m.is_self_delimiting(&m.by_expansion, true))
        .collect();

    println!("{}", "-".repeat(60));
    println!(
        "{:<16} {:>7} {:>9} {:>10.1}% {:>12.1}%",
        "deduplicated",
        seen.len(),
        all.len(),
        share(&all, |m| &m.by_token),
        share(&all, |m| &m.by_expansion),
    );
    println!("\n({duplicates} duplicate files skipped)");
    // The number that says the second reading did any work: without it the two
    // columns agreeing would prove nothing.
    println!(
        "regions whose two readings differ at all: {}",
        all.iter().filter(|m| m.by_token != m.by_expansion).count()
    );
    println!(
        "ragged: {} by token, {} by expansion",
        ragged.len(),
        ragged_expanded.len()
    );

    // The two shapes of raggedness are different problems.
    let disagree = ragged.iter().filter(|m| m.branches_disagree()).count();
    println!("  branches disagree with each other  {disagree}");
    println!(
        "  all branches share one non-zero delta {}",
        ragged.len() - disagree
    );

    let with_alt = all.iter().filter(|m| m.has_else).count();
    println!(
        "\nregions with an `else` branch: {with_alt} of {}",
        all.len()
    );
    println!(
        "regions nesting another region: {}",
        all.iter().filter(|m| m.nested > 0).count()
    );

    let mut histogram: HashMap<Family, usize> = HashMap::new();
    for region in &ragged {
        for branch in &region.by_token {
            for (family, _) in branch.offenders(true) {
                *histogram.entry(family).or_default() += 1;
            }
        }
    }
    if !histogram.is_empty() {
        println!("\nwhich delimiter is handed across a branch:");
        let mut rows: Vec<_> = histogram.into_iter().collect();
        rows.sort_by_key(|&(_, count)| std::cmp::Reverse(count));
        for (family, count) in rows {
            println!("  {:<22} {count}", family.name());
        }
    }

    // Reported, never counted: these have prototype forms with no closer, so
    // an imbalance here is as likely to be this tool's fault as the code's.
    let tripped: Vec<&Measured> = all
        .iter()
        .filter(|m| {
            m.is_self_delimiting(&m.by_token, true) && !m.is_self_delimiting(&m.by_token, false)
        })
        .collect();
    println!(
        "\nstructurally balanced but tripping a declaration keyword: {}",
        tripped.len()
    );
    for region in &tripped {
        let families: Vec<&str> = region
            .by_token
            .iter()
            .flat_map(|delta| delta.offenders(false))
            .filter(|(family, _)| !family.is_structural())
            .map(|(family, _)| family.name())
            .collect();
        println!(
            "  {}:{}  {}",
            region.file.display(),
            region.line,
            families.join(", ")
        );
    }

    println!(
        "\n{}",
        if list_all {
            "all regions ragged by expansion:"
        } else {
            "first 15 regions ragged by expansion:"
        }
    );
    let show = if list_all {
        ragged_expanded.len()
    } else {
        15.min(ragged_expanded.len())
    };
    for region in &ragged_expanded[..show] {
        let detail: Vec<String> = region
            .by_expansion
            .iter()
            .enumerate()
            .filter(|(_, delta)| !delta.is_flat(true))
            .map(|(at, delta)| {
                let items: Vec<String> = delta
                    .offenders(true)
                    .iter()
                    .map(|(family, count)| {
                        let sign = if *count > 0 { "+" } else { "" };
                        format!("{sign}{count} {}", family.name())
                    })
                    .collect();
                format!("branch {at}: {}", items.join(", "))
            })
            .collect();
        println!(
            "  {}:{}  {}",
            region.file.display(),
            region.line,
            detail.join(" | ")
        );
    }
}

/// The share of regions that are self-delimiting, as a percentage.
fn share(regions: &[Measured], which: impl Fn(&Measured) -> &Vec<Delta>) -> f64 {
    if regions.is_empty() {
        return 100.0;
    }
    let good = regions
        .iter()
        .filter(|region| region.is_self_delimiting(which(region), true))
        .count();
    100.0 * good as f64 / regions.len() as f64
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("sv" | "svh" | "v" | "vh")
        ) {
            out.push(path);
        }
    }
    out.sort();
    out
}
