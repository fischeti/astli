//! Expansion against `slang`'s, over the corpus.
//!
//! The oracle M2 owes. Targeted tests say what substitution should do in the
//! cases someone thought of; this says whether it agrees with `slang` -- which
//! real projects compile against -- over real files, in the cases nobody
//! thought of.
//!
//! # What is compared, and what is normalised away
//!
//! Both outputs are lexed and the token sequences compared -- kinds and text,
//! comments included, whitespace dropped.
//!
//! Comparing the text instead would fail on nothing. A token that a macro
//! placed brings no whitespace with it, so how much ends up between two tokens
//! is a property of whoever wrote them out: `slang` separates `)` from an
//! identifier and we do not, and the two mean the same thing. Lexing is what
//! says so without having to guess, and it cannot hide the difference that
//! would matter -- separation that was actually *needed* and not written shows
//! up as two tokens fused into one.
//!
//! # Which files
//!
//! Every file `slang` will preprocess. Neither side is given an include path
//! or a predefined macro, so both resolve a quoted name next to the file that
//! used it, and a file needing a `+incdir+` or a `+define+` is one `slang`
//! declines rather than one we have to filter out.
//!
//! What is left to widen this is the driver: a filelist or `bender` knows what
//! a build actually passes, and handing it to *both* sides is what reaches the
//! files `slang` declines. The assertion that the counts only ever go up
//! is what keeps this honest meanwhile.

use std::path::{Path, PathBuf};
use std::process::Command;

use svirig_preproc::{Session, render};
use svirig_syntax::SyntaxKind::{self, EOF, LINE_COMMENT, STRING_LITERAL, WHITESPACE};
use svirig_syntax::tokenize;

/// Files that agreed outright when this was last run, over the corpus commits
/// pinned in `corpus/MANIFEST`.
///
/// A floor rather than a target. Every compared file agrees, so a failure says
/// so directly -- but a file that quietly stops being *compared* would pass in
/// silence, and that is the way this test rots.
const AGREED: usize = 1843;

/// Files compared at all: the ones that agreed, plus the ones that agreed
/// except where `slang` is wrong.
///
/// Both floors are asserted, so a file moving from the first count to the
/// second shows up. A `slang` that fixes its defects moves them back, and both
/// still hold.
const COMPARED: usize = 1996;

#[test]
#[ignore]
fn corpus_expansion_agrees_with_slang() {
    let Some(files) = corpus() else {
        eprintln!("skipping: run scripts/fetch-corpus.sh to populate corpus/");
        return;
    };
    if Command::new("slang").arg("--version").output().is_err() {
        eprintln!("skipping: slang is not on PATH");
        return;
    }

    let mut agreed = 0;
    let mut tolerated = 0;
    let mut disagreed = Vec::new();
    let mut declined = 0;

    for path in files {
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let ours = expanded(&path, contents);
        let Some(theirs) = slang(&path) else {
            declined += 1;
            continue;
        };

        match reconcile(&lexed(&ours), &lexed(&theirs)) {
            Verdict::Same => agreed += 1,
            Verdict::SlangDefect => tolerated += 1,
            Verdict::Different => disagreed.push((path, ours, theirs)),
        }
    }

    eprintln!(
        "  {agreed} agreed, {tolerated} agreed but for a defect of slang, \
         {} disagreed; {declined} slang declined",
        disagreed.len()
    );

    for (path, ours, theirs) in &disagreed {
        let (ours, theirs) = first_difference(ours, theirs);
        eprintln!("\n{}\n  ours:   {ours}\n  theirs: {theirs}", path.display());
    }
    assert!(
        disagreed.is_empty(),
        "{} of {} compared files disagree",
        disagreed.len(),
        agreed + disagreed.len()
    );
    assert!(
        agreed >= AGREED,
        "{agreed} files agreed outright, down from {AGREED}"
    );
    assert!(
        agreed + tolerated >= COMPARED,
        "{} files compared, down from {COMPARED}: something stopped being comparable",
        agreed + tolerated
    );
}

/// Our expansion, or `None` if this file needs something we do not have yet.
///
/// A conditional `slang` evaluates and we do not makes the comparison
/// worse than meaningless, because we expand every branch and it expands one.
/// The question has to be asked of every file the expansion *read*, not just
/// the one named: a source with no conditional of its own routinely includes a
/// header that chooses its contents with one.
fn expanded(path: &Path, contents: String) -> String {
    let mut session = Session::new();
    let file = session.add(path, contents);
    let tokens = session.expand(file).tokens;
    render(session.origins(), &tokens)
}

/// Whether two token sequences say the same thing.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Same,
    /// They differ only where `slang` is known to be wrong.
    SlangDefect,
    Different,
}

/// Compares the two streams, excusing `slang`'s two known defects.
///
/// An oracle is not infallible, and pretending otherwise means either failing
/// on its bugs or loosening the comparison until it proves nothing. Both of
/// these are narrow, reproducible in three lines, and checked against a third
/// preprocessor, which agrees with us:
///
/// * **Padded stringification.** Whitespace before a `\` continuation in a
///   macro body leaks into the *next* `` `" `` in that body, so a stringified
///   name comes back indented: `` `"__x`" `` yields `"         Name_A"`.
/// * **Comments out of a continued body.** A `//` comment in a body that
///   carries on past it survives into the expansion, line continuation and
///   all. 22.5.1 says comments are not part of the substitution text.
///
/// Anything else is a disagreement. Both are counted, so a defect that widens
/// shows up as a number moving rather than as a test that quietly stops
/// asking.
fn reconcile(ours: &[(SyntaxKind, &str)], theirs: &[(SyntaxKind, &str)]) -> Verdict {
    let (mut here, mut there) = (0, 0);
    let mut excused = false;

    while here < ours.len() && there < theirs.len() {
        if ours[here] == theirs[there] {
            here += 1;
            there += 1;
        } else if padded(ours[here], theirs[there]) {
            excused = true;
            here += 1;
            there += 1;
        } else if continued_comment(theirs[there]) {
            excused = true;
            there += 1;
        } else {
            return Verdict::Different;
        }
    }
    while there < theirs.len() && continued_comment(theirs[there]) {
        excused = true;
        there += 1;
    }

    if here != ours.len() || there != theirs.len() {
        Verdict::Different
    } else if excused {
        Verdict::SlangDefect
    } else {
        Verdict::Same
    }
}

/// Whether `theirs` is `ours` with whitespace inserted after the opening
/// quote.
fn padded(ours: (SyntaxKind, &str), theirs: (SyntaxKind, &str)) -> bool {
    let (STRING_LITERAL, ours) = ours else {
        return false;
    };
    let (STRING_LITERAL, theirs) = theirs else {
        return false;
    };
    match (ours.strip_prefix('"'), theirs.strip_prefix('"')) {
        (Some(ours), Some(theirs)) => theirs.trim_start_matches([' ', '\t']) == ours,
        _ => false,
    }
}

/// A `//` comment carrying the line continuation that ended it, which is what
/// a comment in a continued macro body looks like once it has escaped.
fn continued_comment(token: (SyntaxKind, &str)) -> bool {
    token.0 == LINE_COMMENT && token.1.trim_end().ends_with('\\')
}

/// `slang`'s preprocessed output, or `None` if it would not produce any -- most
/// often because it wants a definition from somewhere it has not been told
/// about.
fn slang(path: &Path) -> Option<String> {
    let output = Command::new("slang")
        .args(["-E", "--comments"])
        .arg(path)
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    (output.status.success() && !text.trim().is_empty()).then_some(text)
}

/// One output as what it lexes as. See
/// [what is normalised](self#what-is-compared-and-what-is-normalised-away).
fn lexed(text: &str) -> Vec<(SyntaxKind, &str)> {
    tokenize(text)
        .into_iter()
        .filter(|token| !matches!(token.kind, WHITESPACE | EOF))
        .map(|token| (token.kind, token.text(text)))
        .collect()
}

/// The two streams from where they stop matching, for a message that says what
/// differs rather than dumping two files.
fn first_difference(ours: &str, theirs: &str) -> (String, String) {
    let (ours, theirs) = (lexed(ours), lexed(theirs));
    let at = ours
        .iter()
        .zip(&theirs)
        .position(|(ours, theirs)| ours != theirs)
        .unwrap_or(ours.len().min(theirs.len()));
    (excerpt(&ours, at), excerpt(&theirs, at))
}

/// A window of a stream from `at`, written back out as text.
fn excerpt(tokens: &[(SyntaxKind, &str)], at: usize) -> String {
    let window: Vec<_> = tokens
        .iter()
        .skip(at.saturating_sub(2))
        .take(10)
        .map(|(_, text)| *text)
        .collect();
    if window.is_empty() {
        "<end of stream>".to_string()
    } else {
        window.join(" ")
    }
}

/// Every SystemVerilog file in the corpus, or `None` if it has not been
/// fetched.
///
/// Relative to the working directory, which cargo sets to the package root, so
/// that both tools are handed a file under the same name. `` `__FILE__ ``
/// expands to the path it was given and `slang` reports one relative to where
/// it was run, so an absolute path here would disagree about the spelling and
/// about nothing else.
fn corpus() -> Option<Vec<PathBuf>> {
    let root = PathBuf::from("../../corpus");
    if !root.is_dir() {
        return None;
    }

    let mut files = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("sv" | "svh")
            ) {
                files.push(path);
            }
        }
    }
    files.sort();
    Some(files)
}
