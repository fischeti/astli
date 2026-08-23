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
//! # What this cannot see
//!
//! A macro that expands to a delimiter -- `` `MY_BEGIN `` -- is one opaque
//! token here, so a region raggedly split by one counts as self-delimiting.
//! Only the preprocessor can settle those, which is the point: this is a
//! cheap lower bound on raggedness, taken before M2 exists.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use svirig_syntax::{SyntaxKind as K, Token, tokenize};

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

    fn add(&mut self, other: &Delta) {
        for (slot, value) in self.0.iter_mut().zip(other.0) {
            *slot += value;
        }
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

#[derive(Debug, Clone)]
struct Region {
    file: PathBuf,
    line: usize,
    /// One per branch, including the implicit empty `else` when absent.
    branches: Vec<Delta>,
    has_alternative: bool,
    nested: usize,
}

impl Region {
    /// Self-delimiting: every branch balances on its own.
    fn is_self_delimiting(&self, structural_only: bool) -> bool {
        self.branches.iter().all(|d| d.is_flat(structural_only))
    }

    /// Ragged regions come in two shapes, and they are worth telling apart:
    /// branches that disagree with each other cannot share a parse at all,
    /// while branches that agree on the same non-zero delta are handing one
    /// delimiter to the code after `` `endif ``.
    fn branches_disagree(&self) -> bool {
        self.branches.windows(2).any(|w| w[0] != w[1])
    }
}

/// Under construction, while the scan is inside its `` `endif ``.
struct Builder {
    line: usize,
    branches: Vec<Delta>,
    current: Delta,
    has_alternative: bool,
    nested: usize,
}

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

/// Finds every conditional region in one file.
///
/// `` `define `` bodies are skipped whole: they are substitution text, so a
/// `begin` inside one opens nothing here.
fn scan(path: &Path, source: &str, out: &mut Vec<Region>) -> usize {
    let tokens = tokenize(source);
    let lines = line_starts(source);

    let mut stack: Vec<Builder> = Vec::new();
    let mut back: Lookback = [K::EOF; 3];
    let mut in_define = false;
    let mut comment_continues = false;
    let mut malformed = 0usize;

    for (i, token) in tokens.iter().enumerate() {
        if in_define {
            // The body runs to the first newline that is not continued. A
            // trailing `\` inside a line comment still continues it: the
            // comment rule swallows the backslash, but real code relies on the
            // body carrying on, so the continuation has to win.
            if token.kind == K::LINE_COMMENT && token.text(source).ends_with('\\') {
                comment_continues = true;
            } else if token.kind == K::WHITESPACE && token.text(source).contains('\n') {
                if comment_continues {
                    comment_continues = false;
                } else {
                    in_define = false;
                }
            }
            continue;
        }
        if token.kind.is_trivia() || token.kind == K::LINE_CONTINUATION {
            continue;
        }

        if token.kind == K::DIRECTIVE {
            match token.text(source) {
                "`define" => {
                    in_define = true;
                    continue;
                }
                "`ifdef" | "`ifndef" => {
                    stack.push(Builder {
                        line: line_of(&lines, token.start),
                        branches: Vec::new(),
                        current: Delta::default(),
                        has_alternative: false,
                        nested: 0,
                    });
                    continue;
                }
                "`elsif" | "`else" => {
                    match stack.last_mut() {
                        Some(builder) => {
                            let finished = builder.current;
                            builder.branches.push(finished);
                            builder.current = Delta::default();
                            builder.has_alternative = true;
                        }
                        None => malformed += 1,
                    }
                    continue;
                }
                "`endif" => {
                    match stack.pop() {
                        Some(mut builder) => {
                            builder.branches.push(builder.current);
                            // With no `` `else ``, the alternative is the empty
                            // branch, and it is a real branch: a region that
                            // opens a `begin` disagrees with taking neither.
                            if !builder.has_alternative {
                                builder.branches.push(Delta::default());
                            }
                            // A nested region contributes to its parent as one
                            // particular environment would resolve it.
                            if let Some(parent) = stack.last_mut() {
                                parent.current.add(&builder.branches[0]);
                                parent.nested += 1;
                            }
                            out.push(Region {
                                file: path.to_path_buf(),
                                line: builder.line,
                                branches: builder.branches,
                                has_alternative: builder.has_alternative,
                                nested: builder.nested,
                            });
                        }
                        None => malformed += 1,
                    }
                    continue;
                }
                _ => {}
            }
        }

        if !stack.is_empty()
            && let Some((family, by)) = opener(token.kind, &back, next_significant(&tokens, i))
        {
            stack.last_mut().unwrap().current.bump(family, by);
        }

        back = [token.kind, back[0], back[1]];
    }

    malformed + stack.len()
}

fn next_significant(tokens: &[Token], from: usize) -> K {
    tokens[from + 1..]
        .iter()
        .map(|t| t.kind)
        .find(|k| !k.is_trivia() && *k != K::LINE_CONTINUATION)
        .unwrap_or(K::EOF)
}

fn line_starts(source: &str) -> Vec<u32> {
    let mut starts = vec![0u32];
    starts.extend(
        source
            .bytes()
            .enumerate()
            .filter(|&(_, b)| b == b'\n')
            .map(|(i, _)| i as u32 + 1),
    );
    starts
}

fn line_of(starts: &[u32], offset: u32) -> usize {
    starts.partition_point(|&s| s <= offset)
}

fn main() {
    let list_all = std::env::args().any(|a| a == "--list");
    let corpus = Path::new("corpus");
    if !corpus.is_dir() {
        eprintln!("no corpus/ -- run scripts/fetch-corpus.sh");
        std::process::exit(1);
    }

    let mut repos: Vec<PathBuf> = std::fs::read_dir(corpus)
        .expect("corpus/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    repos.sort();

    // cva6 vendors common_cells, so the same file appears more than once.
    // Pooled numbers would count it twice; per-repo numbers should not lose it.
    let mut seen: HashSet<u64> = HashSet::new();
    let mut all: Vec<Region> = Vec::new();
    let mut duplicates = 0usize;
    let mut malformed = 0usize;

    println!(
        "{:<16} {:>7} {:>9} {:>10} {:>8}",
        "repo", "files", "regions", "self-delim", "ragged"
    );
    println!("{}", "-".repeat(54));

    for repo in &repos {
        let name = repo.file_name().unwrap().to_string_lossy().to_string();
        let mut regions: Vec<Region> = Vec::new();
        let mut files = 0usize;

        for path in walk(repo) {
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            files += 1;
            let mut hasher = DefaultHasher::new();
            source.hash(&mut hasher);
            let digest = hasher.finish();

            let mut here = Vec::new();
            malformed += scan(&path, &source, &mut here);

            if seen.insert(digest) {
                all.extend(here.iter().cloned());
            } else {
                duplicates += 1;
            }
            regions.extend(here);
        }

        let total = regions.len();
        let good = regions
            .iter()
            .filter(|r| r.is_self_delimiting(true))
            .count();
        println!(
            "{name:<16} {files:>7} {total:>9} {:>9.1}% {:>8}",
            if total == 0 {
                100.0
            } else {
                100.0 * good as f64 / total as f64
            },
            total - good
        );
    }

    let total = all.len();
    let good = all.iter().filter(|r| r.is_self_delimiting(true)).count();
    let ragged: Vec<&Region> = all.iter().filter(|r| !r.is_self_delimiting(true)).collect();

    println!("{}", "-".repeat(54));
    println!(
        "{:<16} {:>7} {:>9} {:>9.1}% {:>8}",
        "deduplicated",
        seen.len(),
        total,
        100.0 * good as f64 / total as f64,
        ragged.len()
    );
    println!("\n({duplicates} duplicate files skipped, {malformed} unmatched directives)");

    // The two shapes of raggedness are different problems.
    let disagree = ragged.iter().filter(|r| r.branches_disagree()).count();
    println!("\nragged regions: {}", ragged.len());
    println!("  branches disagree with each other  {disagree}");
    println!(
        "  all branches share one non-zero delta {}",
        ragged.len() - disagree
    );

    let with_alt = all.iter().filter(|r| r.has_alternative).count();
    let ragged_with_alt = ragged.iter().filter(|r| r.has_alternative).count();
    println!("\nregions with an `else`/`elsif` branch: {with_alt} of {total}");
    println!("  of which ragged: {ragged_with_alt}");
    println!(
        "regions nesting another region: {}",
        all.iter().filter(|r| r.nested > 0).count()
    );

    let mut histogram: HashMap<Family, usize> = HashMap::new();
    for region in &ragged {
        for branch in &region.branches {
            for (family, _) in branch.offenders(true) {
                *histogram.entry(family).or_default() += 1;
            }
        }
    }
    if !histogram.is_empty() {
        println!("\nwhich delimiter is handed across a branch:");
        let mut rows: Vec<_> = histogram.into_iter().collect();
        rows.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
        for (family, n) in rows {
            println!("  {:<22} {n}", family.name());
        }
    }

    // Reported, never counted: these have prototype forms with no closer, so
    // an imbalance here is as likely to be this tool's fault as the code's.
    let keyword_only = all
        .iter()
        .filter(|r| r.is_self_delimiting(true) && !r.is_self_delimiting(false))
        .count();
    println!("\nstructurally balanced but tripping a declaration keyword: {keyword_only}");
    for region in all
        .iter()
        .filter(|r| r.is_self_delimiting(true) && !r.is_self_delimiting(false))
    {
        let families: Vec<&str> = region
            .branches
            .iter()
            .flat_map(|b| b.offenders(false))
            .filter(|(f, _)| !f.is_structural())
            .map(|(f, _)| f.name())
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
            "all ragged regions:"
        } else {
            "first 15 ragged regions:"
        }
    );
    let show = if list_all {
        ragged.len()
    } else {
        15.min(ragged.len())
    };
    for region in &ragged[..show] {
        let detail: Vec<String> = region
            .branches
            .iter()
            .enumerate()
            .filter(|(_, b)| !b.is_flat(true))
            .map(|(i, b)| {
                let items: Vec<String> = b
                    .offenders(true)
                    .iter()
                    .map(|(f, n)| {
                        let sign = if *n > 0 { "+" } else { "" };
                        format!("{sign}{n} {}", f.name())
                    })
                    .collect();
                format!("branch {i}: {}", items.join(", "))
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
