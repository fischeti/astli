//! The corpus tests that make up the M3 gate. Each skips, and says so, when
//! `corpus/` has not been fetched.

use rowan::NodeOrToken;
use svirig_parse::SyntaxTree;
use svirig_syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode};

mod corpus;

/// The rate the milestone is graded on: how much of a file the parser still
/// cannot make sense of.
///
/// **Lower this as rules land.** It may never rise: a rate that goes up is a
/// regression even when every other test passes, which is the whole reason it
/// is asserted rather than only reported.
const RATCHET: f64 = 4.3;

/// Every corpus file, parsed and compared with itself: the invariant no rule
/// is allowed to break.
#[test]
fn corpus_round_trips_through_the_tree() {
    let Some(files) = corpus::files() else {
        return;
    };

    let mut parsed = 0usize;
    let mut bytes = 0usize;
    let mut differ = Vec::new();

    for path in &files {
        let Ok(tree) = SyntaxTree::read(path) else {
            continue; // not UTF-8; not ours to parse
        };
        let text = tree.source();

        parsed += 1;
        bytes += text.len();
        if tree.root().text() != text {
            differ.push(path.display().to_string());
        }
    }

    eprintln!("{parsed} files, {bytes} bytes");
    assert!(
        differ.is_empty(),
        "{} files did not come back byte for byte, first few:\n{}",
        differ.len(),
        differ
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// What every shell claims: the keyword it opened with, and the `end…` that
/// matches it.
///
/// Cheap, and the one assertion that would catch a rule closing a node over
/// text it never read -- which is how the ratchet below could fall
/// by being wrong rather than by being right.
#[test]
fn corpus_shells_close_what_they_open() {
    fn bounds(kind: SyntaxKind) -> Option<(&'static [SyntaxKind], SyntaxKind)> {
        Some(match kind {
            MODULE_DECL => (&[MODULE_KW, MACROMODULE_KW][..], ENDMODULE_KW),
            INTERFACE_DECL => (&[INTERFACE_KW], ENDINTERFACE_KW),
            PROGRAM_DECL => (&[PROGRAM_KW], ENDPROGRAM_KW),
            PACKAGE_DECL => (&[PACKAGE_KW], ENDPACKAGE_KW),
            CLASS_DECL => (&[CLASS_KW, VIRTUAL_KW, INTERFACE_KW], ENDCLASS_KW),
            GENERATE_REGION => (&[GENERATE_KW], ENDGENERATE_KW),
            CASE_STMT => (
                &[
                    CASE_KW,
                    CASEX_KW,
                    CASEZ_KW,
                    UNIQUE_KW,
                    UNIQUE0_KW,
                    PRIORITY_KW,
                ],
                ENDCASE_KW,
            ),
            _ => return None,
        })
    }

    fn walk(node: &SyntaxNode, path: &str, bad: &mut Vec<String>) {
        if let Some((open, close)) = bounds(node.kind()) {
            let mut own: Vec<SyntaxKind> = node
                .children_with_tokens()
                .filter_map(NodeOrToken::into_token)
                .map(|token| token.kind())
                .filter(|kind| !kind.is_trivia())
                .collect();
            // `endmodule : m` -- the label is the node's too.
            if own.len() >= 3 && own[own.len() - 2] == COLON {
                own.truncate(own.len() - 2);
            }
            let opens = own.first().is_some_and(|kind| open.contains(kind));
            let closes = own.last() == Some(&close);
            if !opens || !closes {
                bad.push(format!("{:?} in {path}: {own:?}", node.kind()));
            }
        }
        for child in node.children() {
            walk(&child, path, bad);
        }
    }

    let Some(files) = corpus::files() else {
        return;
    };

    let mut bad = Vec::new();
    for path in &files {
        let Ok(tree) = SyntaxTree::read(path) else {
            continue;
        };
        walk(tree.root(), &path.display().to_string(), &mut bad);
    }

    assert!(
        bad.is_empty(),
        "{} malformed:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// Grammar tokens inside a `VERBATIM`, and grammar tokens in all.
fn rate(tree: &SyntaxNode) -> (usize, usize) {
    fn walk(node: &SyntaxNode, inside: bool, verbatim: &mut usize, total: &mut usize) {
        let inside = inside || node.kind() == VERBATIM;
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(node) => walk(&node, inside, verbatim, total),
                NodeOrToken::Token(token) if !token.kind().is_trivia() => {
                    *total += 1;
                    *verbatim += usize::from(inside);
                }
                NodeOrToken::Token(_) => {}
            }
        }
    }

    let (mut verbatim, mut total) = (0, 0);
    walk(tree, false, &mut verbatim, &mut total);
    (verbatim, total)
}

/// The number M3 is graded on, over the whole corpus.
#[test]
fn corpus_verbatim_rate_does_not_rise() {
    let Some(files) = corpus::files() else {
        return;
    };

    let mut per_repo: Vec<(String, usize, usize)> = Vec::new();
    let (mut all_verbatim, mut all_total) = (0usize, 0usize);

    for path in &files {
        let Ok(tree) = SyntaxTree::read(path) else {
            continue;
        };

        let (verbatim, total) = rate(tree.root());
        all_verbatim += verbatim;
        all_total += total;

        let repo = corpus::repo(path);
        match per_repo.iter_mut().find(|(name, _, _)| *name == repo) {
            Some(entry) => {
                entry.1 += verbatim;
                entry.2 += total;
            }
            None => per_repo.push((repo, verbatim, total)),
        }
    }

    per_repo.sort();
    for (repo, verbatim, total) in &per_repo {
        eprintln!(
            "{repo:16} {:6.2}%  ({verbatim}/{total})",
            100.0 * *verbatim as f64 / *total as f64
        );
    }

    let overall = 100.0 * all_verbatim as f64 / all_total as f64;
    eprintln!("{:16} {overall:6.2}%  ({all_verbatim}/{all_total})", "all");
    assert!(
        overall <= RATCHET,
        "the verbatim rate rose to {overall:.2}%, above the recorded {RATCHET:.2}%"
    );
}
