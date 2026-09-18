//! The macro table, and the shape it gives a reference.
//!
//! Assertions are written against source *text* rather than token indices,
//! because an index tells you nothing when the test fails.

use std::collections::BTreeMap;
use std::path::PathBuf;

use std::rc::Rc;

use svirig_preproc::{Arity, Item, MacroRef, Session, TokenSpan};
use svirig_syntax::Token;
use svirig_text::FileId;

struct Scan {
    session: Session<'static>,
    file: FileId,
    tokens: Rc<[Token]>,
    found: svirig_preproc::Scan,
}

impl Scan {
    fn new(source: &str) -> Scan {
        let mut session = Session::new();
        let file = session.add("top.sv", source.to_string());
        let tokens = session.tokens(file);
        let found = session.scan(file);
        Scan {
            session,
            file,
            tokens,
            found,
        }
    }

    fn source(&self) -> &str {
        self.session.source(self.file)
    }

    /// The source a token range covers, whitespace between tokens included.
    fn text(&self, span: &TokenSpan) -> &str {
        if span.is_empty() {
            return "";
        }
        let start = self.tokens[span.start as usize].start as usize;
        let end = self.tokens[span.end as usize - 1].end as usize;
        &self.source()[start..end]
    }

    fn references(&self) -> Vec<&MacroRef> {
        self.found.references().collect()
    }

    fn only(&self) -> &MacroRef {
        let references = self.references();
        assert_eq!(references.len(), 1, "expected one reference");
        references[0]
    }

    /// Each reference as it reads: `` `NAME `` or `` `NAME(arg, arg) ``.
    fn shapes(&self) -> Vec<String> {
        self.references()
            .iter()
            .map(|reference| {
                let name = self.tokens[reference.name.index as usize].text(self.source());
                match &reference.args {
                    Some(args) => {
                        let args: Vec<_> = args.iter().map(|arg| self.text(arg)).collect();
                        format!("{name}({})", args.join("|"))
                    }
                    None => name.to_string(),
                }
            })
            .collect()
    }
}

#[test]
fn a_definition_decides_whether_a_parenthesis_is_an_argument_list() {
    // The same three bytes, `A (`, read two ways -- which is the whole reason
    // raw mode has to build a table at all.
    let with = Scan::new("`define A(x) x\nassign y = `A (b + c);\n");
    assert_eq!(with.shapes(), ["`A(b + c)"]);

    let without = Scan::new("`define A 1\nassign y = `A (b + c);\n");
    assert_eq!(without.shapes(), ["`A"]);
}

#[test]
fn an_unknown_name_takes_a_parenthesis_from_the_rest_of_its_line() {
    // Nothing defines this here, which is the case for 95% of references in
    // real code: the definition is in a header raw mode never follows.
    let scan = Scan::new("`uvm_field_int   (is_active,   UVM_DEFAULT)\n");
    assert_eq!(scan.shapes(), ["`uvm_field_int(is_active|UVM_DEFAULT)"]);

    // But not from the next line. Nothing in the corpus writes a call that way,
    // and the newline is what stops a wrong guess from running off.
    let scan = Scan::new("`SOME_FLAG\n(a + b);\n");
    assert_eq!(scan.shapes(), ["`SOME_FLAG"]);
}

#[test]
fn a_definition_that_disagrees_with_itself_gives_up_its_arity() {
    // Raw mode has every `ifdef branch in front of it at once, so one name can
    // arrive with two shapes. Guessing between them is worse than admitting the
    // shape is unknown -- and unknown falls back to reading the parenthesis.
    let scan = Scan::new("`ifdef E\n`define A(x) x\n`else\n`define A 1\n`endif\n`A (b)\n");
    assert_eq!(scan.found.macros.arity("A"), Arity::Unknown);
    assert_eq!(scan.shapes(), ["`A(b)"]);

    // Two definitions that agree keep it.
    let scan = Scan::new("`ifdef E\n`define A(x) x\n`else\n`define A(x) y\n`endif\n");
    assert_eq!(scan.found.macros.arity("A"), Arity::Formals(1));
}

#[test]
fn arguments_are_balanced_token_soup() {
    // Commas inside `()`, `[]`, `{}` and `'{}` belong to what encloses them. An
    // argument is text; parsing it as an expression would be wrong.
    let scan = Scan::new("`define A(x, y) x\n`A({1, 2}, f(3, 4))\n");
    assert_eq!(scan.shapes(), ["`A({1, 2}|f(3, 4))"]);

    let scan = Scan::new("`A(mem[i, j], '{1, 2})\n");
    assert_eq!(scan.shapes(), ["`A(mem[i, j]|'{1, 2})"]);

    // A nested call's own parentheses protect its commas without anything
    // knowing its arity -- and it is not an item of its own, because an
    // argument is text in the same way a body is.
    let scan = Scan::new("`A(`B(p, q), r)\n");
    assert_eq!(scan.shapes(), ["`A(`B(p, q)|r)"]);

    // A string is one token, so a comma in it was never at risk.
    let scan = Scan::new("`uvm_info(\"TAG\", $sformatf(\"a,b %0d\", x), UVM_LOW)\n");
    assert_eq!(
        scan.shapes(),
        ["`uvm_info(\"TAG\"|$sformatf(\"a,b %0d\", x)|UVM_LOW)"]
    );
}

#[test]
fn an_argument_may_be_empty_and_the_list_may_span_lines() {
    let scan = Scan::new("`A(,x,)\n");
    assert_eq!(scan.shapes(), ["`A(|x|)"]);

    // A call is not a directive and does not end at a newline. UVM code wraps
    // long argument lists constantly.
    let scan = Scan::new("`uvm_info(\n  \"TAG\",\n  \"msg\",\n  UVM_LOW\n)\n");
    assert_eq!(scan.shapes(), ["`uvm_info(\"TAG\"|\"msg\"|UVM_LOW)"]);
    assert_eq!(scan.text(&scan.only().tokens), scan.source().trim_end());
}

#[test]
fn an_unclosed_list_is_not_a_list() {
    // Reading it as a call would swallow the rest of the file, which is far
    // worse than losing one call's shape.
    let scan = Scan::new("`A(b, c\nmodule m; endmodule\n");
    assert_eq!(scan.shapes(), ["`A"]);
}

#[test]
fn undef_and_undefineall_empty_the_table() {
    let scan = Scan::new("`define A(x) x\n`undef A\n`A (b)\n");
    assert_eq!(scan.found.macros.arity("A"), Arity::Unknown);
    // And with no definition in scope the parenthesis is read as arguments.
    assert_eq!(scan.shapes(), ["`A(b)"]);

    let scan = Scan::new("`define A 1\n`define B 2\n`undefineall\n");
    assert!(scan.found.macros.is_empty());

    // `undef before the definition leaves it standing.
    let scan = Scan::new("`undef A\n`define A 1\n`A (b)\n");
    assert_eq!(scan.found.macros.arity("A"), Arity::Nullary);
    assert_eq!(scan.shapes(), ["`A"]);
}

#[test]
fn a_redefinition_wins() {
    let scan = Scan::new("`define A 1\n`define A 2\n");
    let entry = scan.found.macros.get("A").expect("A is defined");
    assert_eq!(scan.text(&entry.def.body), "2");
}

#[test]
fn a_name_is_the_same_however_it_is_spelled() {
    // 5.6.1: the `\` and the whitespace that terminates an escaped identifier
    // are not part of the name, so `` \A `` and `A` are one macro.
    let scan = Scan::new("`define \\A (x) x\n`A (b)\n");
    assert_eq!(scan.found.macros.arity("A"), Arity::Formals(1));
    assert_eq!(scan.shapes(), ["`A(b)"]);
    // And a lookup may be written either way round.
    assert_eq!(scan.found.macros.arity("`A"), Arity::Formals(1));
}

#[test]
fn a_body_holds_no_items_of_its_own() {
    // Everything in a body is the macro's text, processed where it is used
    // rather than where it is defined (22.2).
    let scan = Scan::new("`define A(x) `uvm_info(\"T\", x, UVM_LOW)\n`A(m)\n");
    assert_eq!(scan.shapes(), ["`A(m)"]);
    assert_eq!(scan.found.items.len(), 2);
}

#[test]
fn items_are_reported_in_order_and_do_not_overlap() {
    let scan = Scan::new("`ifdef E\n`define A(x) x\n`A(1)\n`include \"f.svh\"\n`endif\n");
    let names: Vec<_> = scan
        .found
        .items
        .iter()
        .map(|item| match item {
            Item::Directive(directive) => format!("{:?}", directive.ty),
            Item::Macro(reference) => scan.tokens[reference.name.index as usize]
                .text(scan.source())
                .to_string(),
        })
        .collect();
    assert_eq!(names, ["Ifdef", "Define", "`A", "Include", "Endif"]);

    let mut previous = 0;
    for item in &scan.found.items {
        let tokens = item.tokens();
        assert!(tokens.start >= previous, "{tokens:?} overlaps");
        previous = tokens.end;
    }
}

/// The corpus census. Skipped, loudly, when `corpus/` has not been fetched.
///
/// The counts are printed rather than asserted -- they move with the corpus.
/// What is asserted is that a reference never reaches backwards and never runs
/// past the end of the token stream, which holds whatever is on disk.
#[test]
fn corpus_references_stay_inside_their_file() {
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

    let mut census: BTreeMap<&str, usize> = BTreeMap::new();
    for path in &files {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let mut session = Session::new();
        let file = session.add(path, source);
        let source = session.source(file);
        let tokens = session.tokens(file);
        let found = session.scan(file);

        for reference in found.references() {
            assert!(
                reference.tokens.start == reference.name.index
                    && reference.tokens.end as usize <= tokens.len(),
                "{}: {reference:?} escapes the file",
                path.display()
            );
            let name = tokens[reference.name.index as usize].text(source);
            *census
                .entry(match found.macros.arity(name) {
                    Arity::Nullary => "nullary",
                    Arity::Formals(_) => "with formals",
                    Arity::Unknown => "arity unknown",
                })
                .or_default() += 1;
            *census
                .entry(match reference.args {
                    Some(_) => "given an argument list",
                    None => "no argument list",
                })
                .or_default() += 1;
        }
        *census.entry("files").or_default() += 1;
    }

    for (name, count) in &census {
        eprintln!("  {name:24} {count}");
    }
}

#[test]
fn a_closer_with_no_opener_is_soup_like_anything_else() {
    // Nothing here may take the depth below the list's own, or the list stops
    // closing -- and, before it was guarded, arithmetic overflowed.
    let scan = Scan::new("`A(x], y})\n");
    assert_eq!(scan.shapes(), ["`A(x]|y})"]);
}
