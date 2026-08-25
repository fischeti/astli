//! Directive recognition, and the corpus census that keeps it honest.
//!
//! Assertions are written against source *text* rather than token indices,
//! because an index tells you nothing when the test fails.

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::PathBuf;

use svirig_syntax::preproc::{Directive, DirectiveName, IncludePath, MacroDef, Operands, scan};
use svirig_syntax::{Token, tokenize};

struct Scan<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    directives: Vec<Directive>,
}

impl<'a> Scan<'a> {
    fn new(source: &'a str) -> Scan<'a> {
        let tokens = tokenize(source);
        let directives = scan(source, &tokens).directives().cloned().collect();
        Scan {
            source,
            tokens,
            directives,
        }
    }

    /// The source a token range covers, whitespace between tokens included.
    fn text(&self, range: &Range<u32>) -> &'a str {
        if range.is_empty() {
            return "";
        }
        let start = self.tokens[range.start as usize].start as usize;
        let end = self.tokens[range.end as usize - 1].end as usize;
        &self.source[start..end]
    }

    fn token(&self, at: u32) -> &'a str {
        self.tokens[at as usize].text(self.source)
    }

    fn only(&self) -> &Directive {
        assert_eq!(
            self.directives.len(),
            1,
            "expected one directive, got {:?}",
            self.directives.iter().map(|d| d.name).collect::<Vec<_>>()
        );
        &self.directives[0]
    }

    fn define(&self) -> &MacroDef {
        match &self.only().operands {
            Operands::Define(define) => define,
            other => panic!("expected a `define, got {other:?}"),
        }
    }

    /// Each formal as `name` or `name = default`, which is how they read.
    fn formals(&self) -> Option<Vec<String>> {
        self.define().formals.as_ref().map(|formals| {
            formals
                .iter()
                .map(|formal| match &formal.default {
                    Some(default) => {
                        format!("{} = {}", self.token(formal.name), self.text(default))
                    }
                    None => self.token(formal.name).to_string(),
                })
                .collect()
        })
    }
}

#[test]
fn a_name_is_a_directive_or_it_is_a_macro() {
    use DirectiveName::*;

    assert_eq!(DirectiveName::lookup("`define"), Some(Define));
    assert_eq!(DirectiveName::lookup("`__LINE__"), Some(LineNumber));
    assert_eq!(
        DirectiveName::lookup("`nounconnected_drive"),
        Some(NoUnconnectedDrive)
    );
    // The overwhelming majority of `` ` `` tokens in real code.
    assert_eq!(DirectiveName::lookup("`uvm_info"), None);
    assert_eq!(DirectiveName::lookup("`ASSERT"), None);
    // Tool-specific directives are outside the standard, and read as macros.
    assert_eq!(DirectiveName::lookup("`protect"), None);
    // Directives are case-sensitive.
    assert_eq!(DirectiveName::lookup("`DEFINE"), None);
}

#[test]
fn an_object_like_macro() {
    let scan = Scan::new("`define WORDSIZE 8\nlogic [1:`WORDSIZE] data;\n");
    let define = scan.define();
    assert_eq!(scan.token(define.name), "WORDSIZE");
    assert_eq!(define.formals, None);
    assert_eq!(scan.text(&define.body), "8");
    // The use on the next line is a macro reference, not a directive.
    assert_eq!(scan.directives.len(), 1);
}

#[test]
fn a_macro_with_arguments() {
    let scan = Scan::new("`define var_nand(dly) nand #dly\n");
    assert_eq!(scan.formals(), Some(vec!["dly".to_string()]));
    assert_eq!(scan.text(&scan.define().body), "nand #dly");
}

#[test]
fn an_argument_list_must_touch_the_name() {
    // With a space in between, the parenthesis is body text (22.5.1).
    let scan = Scan::new("`define A (x) y\n");
    assert_eq!(scan.define().formals, None);
    assert_eq!(scan.text(&scan.define().body), "(x) y");
}

#[test]
fn an_escaped_name_may_still_take_arguments() {
    // The one space terminating an escaped identifier is allowed to separate it
    // from the `(`, and that space is part of the token -- so the same
    // adjacency test covers both spellings.
    let scan = Scan::new("`define \\a.b (x) x+1\n");
    assert_eq!(scan.token(scan.define().name), "\\a.b ");
    assert_eq!(scan.formals(), Some(vec!["x".to_string()]));
    assert_eq!(scan.text(&scan.define().body), "x+1");
}

#[test]
fn defaults_are_arbitrary_text() {
    let scan = Scan::new("`define DV(NAME_, COND_ = 1'b1, ARGS_ = ()) x\n");
    assert_eq!(
        scan.formals(),
        Some(vec![
            "NAME_".to_string(),
            "COND_ = 1'b1".to_string(),
            // A default may hold parentheses, so the list ends at a balanced
            // `)` rather than the first one.
            "ARGS_ = ()".to_string(),
        ])
    );
    assert_eq!(scan.text(&scan.define().body), "x");
}

#[test]
fn an_empty_argument_list_is_not_the_absence_of_one() {
    // `` `define A() `` may be invoked as `` `A() ``; `` `define A `` may not.
    assert_eq!(Scan::new("`define A() x\n").formals(), Some(vec![]));
    assert_eq!(Scan::new("`define A x\n").formals(), None);
    // An explicitly empty default is a third thing again.
    assert_eq!(
        Scan::new("`define A(x =) y\n").formals(),
        Some(vec!["x = ".to_string()])
    );
}

#[test]
fn a_body_runs_across_continuations() {
    let scan = Scan::new("`define A(x) \\\n  begin \\\n    f(x); \\\n  end\nwire w;\n");
    assert_eq!(
        scan.text(&scan.define().body),
        "begin \\\n    f(x); \\\n  end"
    );
    // And stops at the newline it does not continue.
    assert_eq!(scan.directives.len(), 1);
}

#[test]
fn a_body_runs_across_a_continuation_inside_a_comment() {
    // The lexer hands the `\` back from the comment; this is the directive
    // layer seeing the result.
    let scan = Scan::new("`define A \\\n  // why \\\n  b\nwire w;\n");
    assert_eq!(
        scan.text(&scan.only().tokens),
        "`define A \\\n  // why \\\n  b"
    );
    // The comment is trivia in front of the text, not part of it.
    assert_eq!(scan.text(&scan.define().body), "b");
}

#[test]
fn a_newline_in_a_block_comment_does_not_end_a_body() {
    // 22.5.1 exempts block comments, and it falls out of one being one token.
    let scan = Scan::new("`define A /* one\ntwo */ x\nwire w;\n");
    assert_eq!(scan.text(&scan.only().tokens), "`define A /* one\ntwo */ x");
    assert_eq!(scan.text(&scan.define().body), "x");
}

#[test]
fn directives_inside_a_body_belong_to_the_macro() {
    // They are processed where the macro is used, not where it is defined
    // (22.2), so the scan does not report them.
    let scan = Scan::new("`define V(F) \\\n  `ifdef E \\\n    `include F \\\n  `endif\n`endif\n");
    let names: Vec<_> = scan.directives.iter().map(|d| d.name).collect();
    assert_eq!(names, [DirectiveName::Define, DirectiveName::Endif]);
}

#[test]
fn conditionals_carry_a_name_or_nothing() {
    let scan = Scan::new("`ifdef A\n`elsif B\n`else\n`endif\n`undef A\n");
    let read: Vec<_> = scan
        .directives
        .iter()
        .map(|d| match &d.operands {
            Operands::Name(name) => format!("{:?} {}", d.name, scan.token(*name)),
            Operands::Bare => format!("{:?}", d.name),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(read, ["Ifdef A", "Elsif B", "Else", "Endif", "Undef A"]);
}

#[test]
fn the_three_spellings_of_include() {
    let scan = Scan::new("`include \"a/b.svh\"\n");
    assert!(matches!(
        &scan.only().operands,
        Operands::Include(IncludePath::Quoted(at)) if scan.token(*at) == "\"a/b.svh\""
    ));

    // `<` and `>` are ordinary operators, so the name is several tokens.
    let scan = Scan::new("`include <uvm_macros.svh>\n");
    match &scan.only().operands {
        Operands::Include(IncludePath::Angle(name)) => {
            assert_eq!(scan.text(name), "uvm_macros.svh")
        }
        other => panic!("expected an angle include, got {other:?}"),
    }

    // Not in 22.4's syntax, but legal by 22.2 and used in the corpus.
    let scan = Scan::new("`include `REQUESTS_FILE\n");
    match &scan.only().operands {
        Operands::Include(IncludePath::Expanded(name)) => {
            assert_eq!(scan.text(name), "`REQUESTS_FILE")
        }
        other => panic!("expected an expanded include, got {other:?}"),
    }
}

#[test]
fn operands_nothing_reads_yet_are_kept_as_tokens() {
    let scan = Scan::new("`timescale 1ns / 1ps\n");
    match &scan.only().operands {
        Operands::Unparsed(rest) => assert_eq!(scan.text(rest), "1ns / 1ps"),
        other => panic!("expected unparsed operands, got {other:?}"),
    }
}

#[test]
fn a_directive_missing_its_operand_is_recorded_not_dropped() {
    // No name on the line, and the scan must not go looking on the next one.
    for source in ["`define\nA 1\n", "`ifdef\nA\n", "`include\n\"f.svh\"\n"] {
        let scan = Scan::new(source);
        assert_eq!(
            scan.directives[0].operands,
            Operands::Malformed,
            "for {source:?}"
        );
    }
    // And at the very end of a file.
    assert_eq!(Scan::new("`ifdef").only().operands, Operands::Malformed);
    assert_eq!(Scan::new("`define").only().operands, Operands::Malformed);
}

/// The corpus census. Skipped, loudly, when `corpus/` has not been fetched.
///
/// The counts are printed rather than asserted -- they move with the corpus.
/// What is asserted is that nothing comes back malformed, which is a property
/// of well-formed input and does not depend on which commits are on disk.
#[test]
fn corpus_has_no_malformed_directives() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    if !root.is_dir() {
        eprintln!("skipping: run scripts/fetch-corpus.sh to populate corpus/");
        return;
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

    let mut census: BTreeMap<String, usize> = BTreeMap::new();
    let mut malformed = Vec::new();

    for path in &files {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let tokens = tokenize(&source);
        for directive in scan(&source, &tokens).directives() {
            *census.entry(format!("{:?}", directive.name)).or_default() += 1;
            if directive.operands == Operands::Malformed {
                let at = tokens[directive.tokens.start as usize];
                let line = source[..at.start as usize].lines().count();
                malformed.push(format!("{}:{line}: {:?}", path.display(), directive.name));
            }
        }
    }

    for (name, count) in &census {
        eprintln!("  {name:20} {count}");
    }
    assert!(
        malformed.is_empty(),
        "{} malformed directives, first few:\n{}",
        malformed.len(),
        malformed[..malformed.len().min(10)].join("\n")
    );
}
