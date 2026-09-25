//! The expanded tree: what a compiler reads, and where each token of it was
//! written.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use astli_parse::{Parsed, parse_expanded};
use astli_preproc::{Build, Session, render};
use astli_syntax::{SyntaxKind, SyntaxKind::*, SyntaxToken};
use astli_text::{Reader, SourceId};

mod corpus;

/// Files held in memory, so a case can include one.
#[derive(Default)]
struct Files(HashMap<PathBuf, String>);

impl Reader for Files {
    fn read(&self, path: &Path) -> Option<String> {
        self.0.get(path).cloned()
    }
}

/// One expanded file, its tree, and the store both point into.
struct Case<'a> {
    session: Session<'a>,
    file: SourceId,
    parsed: Parsed,
}

impl<'a> Case<'a> {
    fn new(files: &'a Files, build: Build, text: &str) -> Case<'a> {
        let mut session = Session::reading(files).building(build);
        let file = session.add("top.sv", text.to_string());
        let expanded = session.expand(file);
        let parsed = parse_expanded(&session, &expanded.tokens);

        // What every case also holds: the text is the expansion's, and each
        // token written somewhere was written as the tree has it.
        assert_eq!(
            parsed.root.text(),
            render(session.origins(), &expanded.tokens).as_str()
        );
        for token in tokens(&parsed) {
            if let Some(span) = parsed.span(&token) {
                assert_eq!(session.origins().slice(span), token.text());
            }
        }
        Case {
            session,
            file,
            parsed,
        }
    }

    fn kinds(&self) -> Vec<SyntaxKind> {
        self.parsed
            .root
            .descendants()
            .map(|node| node.kind())
            .collect()
    }

    fn token(&self, text: &str) -> SyntaxToken {
        tokens(&self.parsed)
            .find(|token| token.text() == text)
            .unwrap_or_else(|| panic!("no `{text}` in the tree"))
    }

    /// The file `token` was spelled in.
    fn spelled_in(&self, token: &SyntaxToken) -> Option<&Path> {
        let span = self.parsed.span(token).expect("a written token");
        let spelled = self.session.origins().spelled(span);
        self.session.origins().path(spelled.src_id)
    }
}

fn tokens(parsed: &Parsed) -> impl Iterator<Item = SyntaxToken> {
    parsed
        .root
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
}

#[test]
fn a_macro_that_writes_an_instantiation_gives_one() {
    let files = Files::default();
    let case = Case::new(
        &files,
        Build::new(),
        "`define INST(t, n) t u_``n ();\nmodule top;\n  `INST(core, a)\nendmodule\n",
    );
    assert!(case.kinds().contains(&INSTANTIATION));
    assert!(!case.kinds().contains(&MACRO_CALL));
    assert!(case.parsed.diagnostics.is_empty());
}

#[test]
fn an_included_file_is_part_of_the_tree() {
    let mut files = Files::default();
    files
        .0
        .insert("inc/pkg.svh".into(), "package p;\nendpackage\n".into());
    let case = Case::new(
        &files,
        Build::new().include_dir("inc"),
        "`include \"pkg.svh\"\nmodule top;\nendmodule\n",
    );

    assert!(case.kinds().contains(&PACKAGE_DECL));
    assert!(case.kinds().contains(&MODULE_DECL));
    let package = case.token("p");
    assert_eq!(case.spelled_in(&package), Some(Path::new("inc/pkg.svh")));
    let module = case.token("top");
    assert_eq!(case.spelled_in(&module), Some(Path::new("top.sv")));
}

#[test]
fn only_the_branch_taken_is_in_the_tree() {
    let files = Files::default();
    let text =
        "module top;\n`ifdef FAST\n  fast_pkg::t x;\n`else\n  slow_pkg::t x;\n`endif\nendmodule\n";

    let fast = Case::new(&files, Build::new().define("FAST", ""), text);
    assert!(tokens(&fast.parsed).any(|token| token.text() == "fast_pkg"));
    assert!(!tokens(&fast.parsed).any(|token| token.text() == "slow_pkg"));

    let slow = Case::new(&files, Build::new(), text);
    assert!(tokens(&slow.parsed).any(|token| token.text() == "slow_pkg"));
}

#[test]
fn tokens_a_macro_placed_side_by_side_are_kept_apart() {
    // Written apart but adjacent in the expansion, `logic` and `q` would read
    // back as one identifier without the space the tree adds.
    let files = Files::default();
    let case = Case::new(
        &files,
        Build::new(),
        "`define TYPE logic\n`define NAME q\nmodule top;\n  `TYPE`NAME;\nendmodule\n",
    );
    assert!(case.parsed.root.text().to_string().contains("logic q;"));
    assert!(case.kinds().contains(&VAR_DECL));

    let separator = tokens(&case.parsed)
        .find(|token| {
            token.kind() == WHITESPACE && token.text() == " " && {
                let previous = token
                    .prev_token()
                    .map(|previous| previous.text().to_string());
                previous.as_deref() == Some("logic")
            }
        })
        .expect("a separator after `logic`");
    assert_eq!(case.parsed.span(&separator), None);
    assert_eq!(case.spelled_in(&case.token("q")), Some(Path::new("top.sv")));
}

#[test]
fn a_file_with_nothing_to_expand_reads_as_in_raw_mode() {
    let files = Files::default();
    let text = "module top (input logic a);\n  // why\n  assign y = a;\nendmodule\n";
    let case = Case::new(&files, Build::new(), text);

    assert_eq!(case.parsed.root.text(), text);
    let raw = astli_parse::SyntaxTree::parse("top.sv", text.to_string());
    assert_eq!(
        format!("{:#?}", case.parsed.root),
        format!("{:#?}", raw.root())
    );

    let assign = case.token("assign");
    let span = case.parsed.span(&assign).unwrap();
    assert_eq!(span.src_id, case.file);
}

/// The share of grammar tokens left `VERBATIM` in expanded trees.
///
/// Higher than in raw mode because each file carries the headers it includes,
/// whose covergroups, constraints and assertions the grammar leaves to the
/// fallback. **Lower this as rules land**, as with the raw ratchet.
const RATCHET: f64 = 12.1;

/// Every corpus file, expanded and parsed: the tree is the expansion, and
/// each token it did not add maps back to where it was written.
///
/// A file is expanded against every directory in its repository that holds a
/// header, which stands in for the filelist it would be compiled with.
#[test]
fn corpus_expanded_trees_are_their_expansion() {
    let Some(files) = corpus::files() else {
        return;
    };

    let mut headers: HashMap<String, BTreeSet<PathBuf>> = HashMap::new();
    for path in files
        .iter()
        .filter(|path| path.extension().is_some_and(|e| e == "svh"))
    {
        if let Some(dir) = path.parent() {
            headers
                .entry(corpus::repo(path))
                .or_default()
                .insert(dir.to_path_buf());
        }
    }

    let (mut parsed, mut verbatim, mut total) = (0usize, 0usize, 0usize);
    let mut bad = Vec::new();
    for path in files
        .iter()
        .filter(|path| path.extension().is_some_and(|e| e == "sv"))
    {
        let dirs = headers.get(&corpus::repo(path)).into_iter().flatten();
        let build = dirs.fold(Build::new(), |build, dir| build.include_dir(dir));
        let mut session = Session::new().building(build);
        let Some(file) = session.open(path) else {
            continue; // not UTF-8; not ours to parse
        };
        let expanded = session.expand(file);
        let tree = parse_expanded(&session, &expanded.tokens);
        parsed += 1;

        let origins = session.origins();
        let text = render(origins, &expanded.tokens);
        let wrong = if tree.root.text() != text.as_str() {
            Some("its text is not the expansion".to_string())
        } else {
            tokens(&tree).find_map(|token| match tree.span(&token) {
                Some(span) if origins.slice(span) != token.text() => Some(format!(
                    "`{}` maps to `{}`",
                    token.text(),
                    origins.slice(span)
                )),
                None if token.kind() != WHITESPACE => {
                    Some(format!("`{}` maps nowhere", token.text()))
                }
                _ => None,
            })
        };
        if let Some(wrong) = wrong {
            bad.push(format!("{}: {wrong}", path.display()));
        }

        for token in tokens(&tree).filter(|token| !token.kind().is_trivia()) {
            total += 1;
            verbatim += usize::from(token.parent_ancestors().any(|node| node.kind() == VERBATIM));
        }
    }

    let rate = 100.0 * verbatim as f64 / total as f64;
    eprintln!("{parsed} files, {rate:.2}% of grammar tokens verbatim ({verbatim}/{total})");
    assert!(
        bad.is_empty(),
        "{} files, first few:\n{}",
        bad.len(),
        bad.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
    );
    assert!(
        rate <= RATCHET,
        "the expanded verbatim rate rose to {rate:.2}%, above the recorded {RATCHET:.2}%"
    );
}
