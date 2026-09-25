//! Substitution, and the provenance it records.
//!
//! What comes out is asserted as *text*, through the stream renderer, because
//! that is what the result means. Where the point is where a token came from
//! rather than what it is, the assertion is on its origin instead.

use astli_preproc::{Build, COMMAND_LINE, ExpandedToken, Session, render};
use astli_text::Span;

struct Expanded {
    session: Session<'static>,
    tokens: Vec<ExpandedToken>,
}

impl Expanded {
    fn new(source: &str) -> Expanded {
        Expanded::building(Build::new(), source)
    }

    fn building(build: Build, source: &str) -> Expanded {
        let mut session = Session::new().building(build);
        let file = session.add("top.sv", source.to_string());
        let tokens = session.expand(file).tokens;
        Expanded { session, tokens }
    }

    /// The expansion as one line, whitespace collapsed.
    ///
    /// A token stream carries no whitespace of its own once a macro has placed
    /// it, so how much there is between two tokens is not a fact about the
    /// expansion. Where it is the point, [`Expanded::rendered`] keeps it.
    fn text(&self) -> String {
        self.rendered()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn rendered(&self) -> String {
        render(self.session.origins(), &self.tokens)
    }

    /// Every expanded token spelled exactly `text`.
    fn spelled(&self, text: &str) -> Vec<ExpandedToken> {
        self.tokens
            .iter()
            .copied()
            .filter(|token| self.session.origins().slice(token.span) == text)
            .collect()
    }

    fn only(&self, text: &str) -> ExpandedToken {
        let found = self.spelled(text);
        assert_eq!(found.len(), 1, "expected one `{text}`");
        found[0]
    }

    /// The line a token's bytes are written on.
    fn line(&self, span: Span) -> u32 {
        self.session
            .origins()
            .line_col(span.src_id, span.start)
            .line
    }
}

#[test]
fn a_body_is_substituted_and_the_directive_is_gone() {
    let expanded = Expanded::new("`define WIDTH 8 + 1\nlogic [`WIDTH-1:0] q;\n");
    assert_eq!(expanded.text(), "logic [8 + 1-1:0] q;");
}

#[test]
fn a_formal_takes_the_text_the_call_passed() {
    let expanded =
        Expanded::new("`define MAX(a, b) ((a) > (b) ? (a) : (b))\nx = `MAX(p, q + 1);\n");
    assert_eq!(expanded.text(), "x = ((p) > (q + 1) ? (p) : (q + 1));");
}

#[test]
fn an_argument_is_spelled_at_the_call_and_placed_by_the_expansion() {
    // The case per-byte provenance has to swap roles for: `f` is written in the
    // body and `p` in the argument, and one call places both.
    let expanded = Expanded::new("`define M(x) f(x)\nassign y = `M(p + q);\n");

    let body = expanded.only("f").span;
    let argument = expanded.only("p").span;
    let origins = expanded.session.origins();

    assert_eq!(expanded.line(body), 1);
    assert_eq!(expanded.line(argument), 2);
    assert_eq!(
        origins.placed_by(body.src_id),
        origins.placed_by(argument.src_id)
    );
    assert!(origins.placed_by(body.src_id).is_some());

    // And a message about either points at the call the reader wrote.
    let call = expanded.session.origins().reported_at(body);
    assert_eq!(expanded.session.origins().slice(call), "`M(p + q)");
    assert_eq!(expanded.session.origins().reported_at(argument), call);
}

#[test]
fn a_macro_that_expands_a_macro_reads_back_as_a_chain() {
    let expanded = Expanded::new(concat!(
        "`define ASSERT(c) if (!(c)) $error(\"failed\")\n",
        "`define CHECK(c) `ASSERT(c)\n",
        "initial `CHECK(x > 0);\n",
    ));
    assert_eq!(expanded.text(), "initial if (!(x > 0)) $error(\"failed\");");

    let token = expanded.only("$error").span;
    let chain: Vec<_> = expanded
        .session
        .origins()
        .trace(token.src_id)
        .map(|expansion| expanded.session.origins().slice(expansion.name).to_string())
        .collect();
    assert_eq!(chain, ["`ASSERT", "`CHECK"]);
}

#[test]
fn an_argument_expands_in_the_scope_it_was_written_in() {
    // `x` is the outer macro's formal, and the argument that mentions it is
    // written where the outer macro was called -- so the inner macro's own
    // formal, also `x`, must not capture it.
    let expanded = Expanded::new(concat!(
        "`define INNER(x) [x]\n",
        "`define OUTER(x) `INNER(x + 1)\n",
        "y = `OUTER(p);\n",
    ));
    assert_eq!(expanded.text(), "y = [p + 1];");
}

#[test]
fn a_nested_call_in_an_argument_expands() {
    let expanded = Expanded::new("`define ID(x) x\n`define ONE 1\ny = `ID(`ONE + `ONE);\n");
    assert_eq!(expanded.text(), "y = 1 + 1;");
}

#[test]
fn an_empty_argument_list_is_no_arguments_only_when_there_are_no_formals() {
    // `` `A() `` splits into one empty argument, because the list is split on
    // commas and nothing else.
    let expanded = Expanded::new("`define A() z\ny = `A();\n");
    assert_eq!(expanded.text(), "y = z;");

    // With a formal, that one empty argument is an empty argument.
    let expanded = Expanded::new("`define A(x) [x]\ny = `A();\n");
    assert_eq!(expanded.text(), "y = [];");
}

#[test]
fn a_default_fills_an_omitted_argument_but_not_an_empty_one() {
    let expanded = Expanded::new("`define A(x = 1, y = 2) x + y\nz = `A(5);\n");
    assert_eq!(expanded.text(), "z = 5 + 2;");

    // Present and empty is not absent, so `x` takes nothing rather than its
    // default and only `y` is filled in.
    let expanded = Expanded::new("`define A(x = 1, y = 2) x + y\nz = `A();\n");
    assert_eq!(expanded.text(), "z = + 2;");
}

#[test]
fn a_formal_with_neither_argument_nor_default_expands_to_nothing() {
    // An error by 22.5.1. Expanding the formal to nothing keeps the rest of the
    // body, which is the cheaper of the two ways to be wrong.
    let expanded = Expanded::new("`define A(x, y) x + y\nz = `A(1);\n");
    assert_eq!(expanded.text(), "z = 1 + ;");
}

#[test]
fn a_line_continuation_in_a_body_expands_to_the_newline_alone() {
    let expanded = Expanded::new("`define TWO a \\\n  b\nx = `TWO;\n");
    assert!(
        expanded.rendered().contains("a \n  b"),
        "the `\\` goes and the newline stays: {:?}",
        expanded.rendered()
    );
    assert!(!expanded.rendered().contains('\\'));
}

#[test]
fn a_comment_in_a_body_is_not_substituted_text() {
    let expanded = Expanded::new("`define A 1 /* note */ + 2\nx = `A;\n");
    assert_eq!(expanded.text(), "x = 1 + 2;");
    assert!(!expanded.rendered().contains("note"));

    // A `//` comment mid-body, which only exists at all because the `\` that
    // continues the definition is kept out of it.
    let expanded = Expanded::new("`define A 1 + // note\\\n 2\nx = `A;\n");
    assert_eq!(expanded.text(), "x = 1 + 2;");
    assert!(!expanded.rendered().contains("note"));
}

#[test]
fn a_macro_that_reaches_itself_stands_as_written() {
    let expanded = Expanded::new("`define A `A\nx = `A;\n");
    assert_eq!(expanded.text(), "x = `A;");

    // Mutual recursion is the same question, and the same answer.
    let expanded = Expanded::new("`define A `B\n`define B `A\nx = `A;\n");
    assert_eq!(expanded.text(), "x = `A;");

    // Reaching the same macro twice without nesting is not recursion.
    let expanded = Expanded::new("`define A 1\n`define B `A + `A\nx = `B;\n");
    assert_eq!(expanded.text(), "x = 1 + 1;");
}

#[test]
fn an_undefined_reference_stands_as_written() {
    // An error on this path, unlike in raw mode where 95% of references have no
    // definition in their own file. The tokens are the stand-in until there is
    // a diagnostics layer to say so.
    let expanded = Expanded::new("`uvm_info(\"TAG\", \"msg\", UVM_LOW)\n");
    assert_eq!(expanded.text(), "`uvm_info(\"TAG\", \"msg\", UVM_LOW)");
}

#[test]
fn a_call_that_gives_arguments_to_a_macro_without_formals_keeps_them_as_text() {
    // The `WITH` case: two branches define one name with different shapes, so
    // the scan reads the parenthesis as a list. The definition that wins takes
    // no arguments, which makes the parentheses the expression's own.
    let expanded = Expanded::new(concat!(
        "`ifdef E\n`define WITH(x) x\n`else\n`define WITH iff\n`endif\n",
        "y = `WITH (!a);\n",
    ));
    assert_eq!(expanded.text(), "y = iff (!a);");
}

#[test]
fn a_macro_needing_an_argument_list_and_given_none_stands_as_written() {
    let expanded = Expanded::new("`define A(x) [x]\ny = `A;\n");
    assert_eq!(expanded.text(), "y = `A;");
}

#[test]
fn the_built_in_macros_answer_for_the_outermost_call_site() {
    let expanded = Expanded::new("x = `__LINE__;\ny = `__FILE__;\n");
    assert_eq!(expanded.text(), "x = 1; y = \"top.sv\";");

    // A `` `__LINE__ `` in a body reports where the macro was used, not where
    // it was written -- which is what the origin map already computes.
    let expanded = Expanded::new("`define WHERE `__LINE__\n\n\nx = `WHERE;\n");
    assert_eq!(expanded.text(), "x = 4;");
}

#[test]
fn a_redefinition_substitutes_the_later_body() {
    let expanded = Expanded::new("`define A 1\nw = `A;\n`define A 2\nx = `A;\n");
    assert_eq!(expanded.text(), "w = 1; x = 2;");

    // And `` `undef `` leaves the name with nothing to substitute.
    let expanded = Expanded::new("`define A 1\n`undef A\nx = `A;\n");
    assert_eq!(expanded.text(), "x = `A;");
}

#[test]
fn tokens_placed_side_by_side_are_rendered_a_space_apart() {
    // Adjacent in the stream, apart where written: `logicq` would lex as one.
    let expanded = Expanded::new("`define TYPE logic\n`define NAME q\n`TYPE`NAME;\n");
    assert_eq!(expanded.rendered().trim(), "logic q;");
}

#[test]
fn pasting_fuses_the_tokens_that_meet() {
    // The `` `` `` idiom every register macro is built on: the argument makes
    // part of a name, which is one identifier and not three.
    let expanded = Expanded::new("`define REG(n) reg_``n``_q\nx = `REG(addr);\n");
    assert_eq!(expanded.text(), "x = reg_addr_q;");

    // The operator is deleted and nothing else is: whitespace around it is
    // the author's, and it keeps the two apart.
    let expanded = Expanded::new("`define J(a, b) a `` b\nx = `J(foo, bar);\n");
    assert_eq!(expanded.text(), "x = foo bar;");

    // So a keyword that precedes an argument's name stays a keyword. The
    // corpus builds signal names this way, and reading the operator as one
    // that eats its own whitespace fuses `force` onto the name.
    let expanded = Expanded::new("`define F(n) force ``n``_if = 0;\n`F(bus)\n");
    assert_eq!(expanded.text(), "force bus_if = 0;");

    // What the bytes make is what comes out. Two tokens that do not fuse into
    // one stay two.
    let expanded = Expanded::new("`define J(a, b) a``b\nx = `J(1, +2);\n");
    assert_eq!(expanded.text(), "x = 1+2;");
}

#[test]
fn a_pasted_token_is_spelled_in_no_file() {
    let expanded = Expanded::new("`define REG(n) reg_``n``_q\nx = `REG(addr);\n");
    let fused = expanded.only("reg_addr_q").span;

    assert_eq!(expanded.session.origins().path(fused.src_id), None);
    // And a message about it still points at the call that was written.
    let call = expanded.session.origins().reported_at(fused);
    assert_eq!(expanded.session.origins().slice(call), "`REG(addr)");
}

#[test]
fn stringification_expands_what_it_quotes() {
    let expanded = Expanded::new("`define SHOW(x) $display(`\"x = %0d`\", x)\n`SHOW(count);\n");
    assert_eq!(expanded.text(), "$display(\"count = %0d\", count);");

    // A nested macro inside the quotes expands too: the text is macro text
    // like any other.
    let expanded = Expanded::new("`define W 8\n`define N `\"width `W`\"\nx = `N;\n");
    assert_eq!(expanded.text(), "x = \"width 8\";");
}

#[test]
fn an_escaped_quote_survives_into_the_string() {
    let expanded = Expanded::new("`define Q(x) `\"he said `\\`\"x`\\`\" today`\"\ny = `Q(no);\n");
    assert_eq!(expanded.text(), "y = \"he said \\\"no\\\" today\";");
}

#[test]
fn a_quote_in_stringified_text_is_escaped() {
    // A string literal inside the quotes is text, and its own quotes cannot be
    // allowed to end the literal being built.
    let expanded = Expanded::new("`define S(x) `\"got x`\"\ny = `S(\"a\");\n");
    assert_eq!(expanded.text(), "y = \"got \\\"a\\\"\";");
}

#[test]
fn a_stringified_token_is_spelled_in_no_file() {
    let expanded = Expanded::new("`define SHOW(x) `\"x`\"\ny = `SHOW(z);\n");
    let string = expanded.only("\"z\"").span;

    assert_eq!(expanded.session.origins().path(string.src_id), None);
    assert_eq!(
        expanded
            .session
            .origins()
            .slice(expanded.session.origins().reported_at(string)),
        "`SHOW(z)"
    );
}

#[test]
fn a_build_defines_before_the_first_line() {
    let build = Build::new().define("WIDTH", "8").define("SYNTHESIS", "1");
    let source = "`ifdef SYNTHESIS\nlogic [`WIDTH-1:0] q;\n`endif\n";
    let expanded = Expanded::building(build, source);
    assert_eq!(expanded.text(), "logic [8-1:0] q;");

    // The body is spelled somewhere a reader can be pointed at.
    let origins = expanded.session.origins();
    let eight = origins.spelled(expanded.only("8").span);
    assert_eq!(
        origins.path(eight.src_id).unwrap().to_str(),
        Some(COMMAND_LINE)
    );
}

#[test]
fn a_file_redefines_what_the_build_defined() {
    let build = Build::new().define("W", "8");
    let expanded = Expanded::building(build, "`define W 16\nx = `W;\n");
    assert_eq!(expanded.text(), "x = 16;");
}
