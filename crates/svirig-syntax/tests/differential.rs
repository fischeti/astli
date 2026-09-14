//! Expansion against another implementation's, over the corpus.
//!
//! The oracle M2 owes. Targeted tests say what substitution should do in the
//! cases someone thought of; this says whether it agrees with a preprocessor
//! that real projects compile against, over real files, in the cases nobody
//! thought of.
//!
//! # What is compared, and what is normalised away
//!
//! Both outputs are lexed and the token sequences compared -- kinds and text,
//! comments included, whitespace dropped.
//!
//! Comparing the text instead would fail on nothing. A token that a macro
//! placed brings no whitespace with it, so how much ends up between two tokens
//! is a property of whoever wrote them out: the other tool separates `)` from
//! an identifier and we do not, and the two mean the same thing. Lexing is what
//! says so without having to guess, and it cannot hide the difference that
//! would matter -- separation that was actually *needed* and not written shows
//! up as two tokens fused into one.
//!
//! # Which files
//!
//! The ones where expansion and `` `include `` are the whole of the answer,
//! which is to say the ones with no conditional. Neither side is given an
//! include path, so both resolve a quoted name next to the file that used it
//! and a header that needs a `+incdir+` is one the reference declines anyway.
//! Step 5 widens this the rest of the way, and the assertion that the count
//! only ever goes up is what keeps it honest as that lands.

use std::path::{Path, PathBuf};
use std::process::Command;

use svirig_syntax::SyntaxKind::{self, EOF, WHITESPACE};
use svirig_syntax::preproc::{DirectiveType, Includes, Input, expand, render, scan};
use svirig_syntax::tokenize;
use svirig_text::{FileId, Origins};

/// Files that agreed when this was last run, over the corpus commits pinned in
/// `corpus/MANIFEST`.
///
/// A floor rather than a target. Every comparable file agrees, so a failure
/// says so directly -- but a file that stops being *comparable* would
/// otherwise pass silently, and that is the way this test can rot. The number
/// rises as conditionals land.
const AGREED: usize = 1507;

#[test]
fn expansion_agrees_with_another_preprocessor() {
    let Some(files) = corpus() else {
        eprintln!("skipping: run scripts/fetch-corpus.sh to populate corpus/");
        return;
    };
    if Command::new("slang").arg("--version").output().is_err() {
        eprintln!("skipping: no reference preprocessor on PATH");
        return;
    }

    let mut agreed = 0;
    let mut disagreed = Vec::new();
    // Kept apart because they say different things. The first shrinks as steps
    // 4 and 5 land; the second is about what the other tool needs to be told.
    let mut needs_more_of_us = 0;
    let mut declined = 0;

    for path in files {
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(ours) = expanded(&path, contents) else {
            needs_more_of_us += 1;
            continue;
        };
        let Some(theirs) = reference(&path) else {
            declined += 1;
            continue;
        };

        if lexed(&ours) == lexed(&theirs) {
            agreed += 1;
        } else {
            disagreed.push((path, ours, theirs));
        }
    }

    eprintln!(
        "  {agreed} agreed, {} disagreed; {needs_more_of_us} use a conditional, \
         {declined} the reference declined",
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
        "{agreed} files agreed, down from {AGREED}: something stopped being comparable"
    );
}

/// Our expansion, or `None` if this file needs something we do not have yet.
///
/// A conditional the other tool evaluates and we do not makes the comparison
/// worse than meaningless, because we expand every branch and it expands one.
/// The question has to be asked of every file the expansion *read*, not just
/// the one named: a source with no conditional of its own routinely includes a
/// header that chooses its contents with one.
fn expanded(path: &Path, contents: String) -> Option<String> {
    let mut origins = Origins::new();
    let file = origins.add_file(path, contents);
    let tokens = expand(&mut origins, file, &Includes::new());

    origins
        .files()
        .all(|file| !has_a_conditional(&origins, file))
        .then(|| render(&origins, &tokens))
}

fn has_a_conditional(origins: &Origins, file: FileId) -> bool {
    use DirectiveType::*;

    let source = origins.text(file);
    let tokens = tokenize(source);
    scan(&Input::new(file, source, &tokens))
        .directives()
        .any(|directive| matches!(directive.ty, Ifdef | Ifndef | Elsif | Else | Endif))
}

/// The other tool's preprocessed output, or `None` if it would not produce any
/// -- most often because it wants a definition from somewhere it has not been
/// told about.
fn reference(path: &Path) -> Option<String> {
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
/// expands to the path it was given and the other tool reports one relative to
/// where it was run, so an absolute path here would disagree about the
/// spelling and about nothing else.
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
