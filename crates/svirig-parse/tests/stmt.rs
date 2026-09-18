//! Statements, and the one thing that separates an assignment from a
//! comparison.

use svirig_parse::{Parser, Raw, Scope, build, statement};
use svirig_preproc::{Input, Session};
use svirig_syntax::{SyntaxKind::*, SyntaxNode};
use svirig_text::FileId;

struct Source {
    session: Session<'static>,
    file: FileId,
}

impl Source {
    fn new(text: &str) -> Source {
        let mut session = Session::new();
        let file = session.add("top.sv", text.to_string());
        Source { session, file }
    }

    fn input(&self) -> Input<'_> {
        self.session.input(self.file)
    }
}

/// The whole of `text` read as statements, with the tree checked against it.
fn tree(text: &str) -> SyntaxNode {
    let source = Source::new(text);
    let input = source.input();
    let mut parser = Parser::new(Raw::new(input));
    parser.set_scope(Scope::Statement);

    let file = parser.start();
    while !parser.at_end() {
        let before = parser.position();
        statement(&mut parser, None);
        assert!(parser.position() > before, "no progress in `{text}`");
    }
    parser.complete(file, SOURCE_FILE);

    let tree = SyntaxNode::new_root(build(&parser.finish(), input));
    assert_eq!(tree.text().to_string(), text, "the tree is not the file");
    tree
}

/// The node kinds `text` builds, and how they nest.
fn shape(text: &str) -> String {
    fn walk(node: &SyntaxNode, out: &mut String) {
        out.push_str(&format!("{:?}", node.kind()));
        let children: Vec<SyntaxNode> = node.children().collect();
        if children.is_empty() {
            return;
        }
        out.push('(');
        for (at, child) in children.iter().enumerate() {
            if at > 0 {
                out.push(' ');
            }
            walk(child, out);
        }
        out.push(')');
    }

    let mut out = String::new();
    walk(&tree(text), &mut out);
    out
}

// ------------------------------------------------------------- assignments

#[test]
fn a_nonblocking_assignment_is_not_a_comparison() {
    // `<=` sits at level 9 of the precedence table *and* assigns. Parsing the
    // left-hand side as an expression would give a `BIN_EXPR` over the whole
    // line with the assignment nowhere in the tree; parsing it as an lvalue
    // stops before the operator and lets the statement rule see it.
    assert_eq!(
        shape("q <= d;\n"),
        "SOURCE_FILE(EXPR_STMT(ASSIGNMENT(NAME_REF NAME_REF)))"
    );
    // And the comparison still reads as one where it is written.
    assert_eq!(
        shape("q <= (a <= b);\n"),
        "SOURCE_FILE(EXPR_STMT(ASSIGNMENT(\
           NAME_REF PAREN_EXPR(BIN_EXPR(NAME_REF NAME_REF)))))"
    );
}

#[test]
fn every_assignment_operator_builds_the_same_node() {
    for operator in [
        "=", "<=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=", "<<<=", ">>>=",
    ] {
        let text = format!("a {operator} b;\n");
        assert_eq!(
            shape(&text),
            "SOURCE_FILE(EXPR_STMT(ASSIGNMENT(NAME_REF NAME_REF)))",
            "{operator}"
        );
    }
}

#[test]
fn a_left_hand_side_may_be_selected_or_concatenated() {
    for text in [
        "a[3:0] = b;\n",
        "{a, b} = c;\n",
        "x.y.z = 1;\n",
        "mem[i][j] = 0;\n",
    ] {
        assert!(
            shape(text).starts_with("SOURCE_FILE(EXPR_STMT(ASSIGNMENT("),
            "{text:?} gave {}",
            shape(text)
        );
    }
}

#[test]
fn an_assignment_may_be_delayed_by_its_own_operator() {
    assert_eq!(
        shape("a <= #1 b;\n"),
        "SOURCE_FILE(EXPR_STMT(ASSIGNMENT(\
           NAME_REF DELAY_CONTROL(LITERAL_EXPR) NAME_REF)))"
    );
}

#[test]
fn a_call_is_a_statement_and_so_is_nothing_at_all() {
    assert_eq!(
        shape("$display(\"x\");\n"),
        "SOURCE_FILE(EXPR_STMT(CALL_EXPR(NAME_REF ARG_LIST(ARG(LITERAL_EXPR)))))"
    );
    assert_eq!(shape(";\n"), "SOURCE_FILE(EXPR_STMT)");
}

// -------------------------------------------------------------------- flow

#[test]
fn an_else_if_nests_to_the_right() {
    assert_eq!(
        shape("if (a) x = 1; else if (b) x = 2; else x = 3;\n"),
        "SOURCE_FILE(IF_STMT(\
           PAREN_EXPR(NAME_REF) \
           EXPR_STMT(ASSIGNMENT(NAME_REF LITERAL_EXPR)) \
           IF_STMT(\
             PAREN_EXPR(NAME_REF) \
             EXPR_STMT(ASSIGNMENT(NAME_REF LITERAL_EXPR)) \
             EXPR_STMT(ASSIGNMENT(NAME_REF LITERAL_EXPR)))))"
    );
}

#[test]
fn a_qualifier_belongs_to_the_conditional_it_qualifies() {
    for text in [
        "unique if (a) x = 1;\n",
        "unique0 if (a) x = 1;\n",
        "priority if (a) x = 1;\n",
    ] {
        assert!(shape(text).starts_with("SOURCE_FILE(IF_STMT("), "{text:?}");
    }
    assert!(
        shape("priority case (a) 1: x = 1; endcase\n").starts_with("SOURCE_FILE(CASE_STMT("),
        "a qualified case"
    );
}

#[test]
fn a_case_arm_may_match_several_values_and_one_matches_the_rest() {
    assert_eq!(
        shape("case (s) 0, 1: x = 1; default: x = 2; endcase\n"),
        "SOURCE_FILE(CASE_STMT(\
           PAREN_EXPR(NAME_REF) \
           CASE_ITEM(LITERAL_EXPR LITERAL_EXPR EXPR_STMT(ASSIGNMENT(NAME_REF LITERAL_EXPR))) \
           CASE_ITEM(EXPR_STMT(ASSIGNMENT(NAME_REF LITERAL_EXPR)))))"
    );
}

#[test]
fn a_case_that_never_closes_is_given_back() {
    assert_eq!(shape("case (s) 0: x = 1;\n"), "SOURCE_FILE(VERBATIM)");
}

#[test]
fn a_for_header_declares_or_assigns() {
    assert_eq!(
        shape("for (int i = 0; i < 4; i++) x = i;\n"),
        "SOURCE_FILE(FOR_STMT(\
           PAREN_EXPR(\
             VAR_DECL(TYPE_REF DECLARATOR(LITERAL_EXPR)) \
             BIN_EXPR(NAME_REF LITERAL_EXPR) \
             POSTFIX_EXPR(NAME_REF)) \
           EXPR_STMT(ASSIGNMENT(NAME_REF NAME_REF))))"
    );
    assert_eq!(
        shape("for (i = 0; i < 4; i = i + 1) x = i;\n"),
        "SOURCE_FILE(FOR_STMT(\
           PAREN_EXPR(\
             ASSIGNMENT(NAME_REF LITERAL_EXPR) \
             BIN_EXPR(NAME_REF LITERAL_EXPR) \
             ASSIGNMENT(NAME_REF BIN_EXPR(NAME_REF LITERAL_EXPR))) \
           EXPR_STMT(ASSIGNMENT(NAME_REF NAME_REF))))"
    );
}

#[test]
fn the_loops_that_are_a_header_and_a_body() {
    for (text, kind) in [
        ("while (a) x = 1;\n", "WHILE_STMT"),
        ("repeat (4) x = 1;\n", "REPEAT_STMT"),
        ("foreach (a[i]) x = 1;\n", "FOREACH_STMT"),
    ] {
        assert!(
            shape(text).starts_with(&format!("SOURCE_FILE({kind}(")),
            "{text:?}"
        );
    }
    assert_eq!(
        shape("forever x = 1;\n"),
        "SOURCE_FILE(FOREVER_STMT(EXPR_STMT(ASSIGNMENT(NAME_REF LITERAL_EXPR))))"
    );
    assert_eq!(
        shape("do x = 1; while (a);\n"),
        "SOURCE_FILE(DO_WHILE_STMT(\
           EXPR_STMT(ASSIGNMENT(NAME_REF LITERAL_EXPR)) PAREN_EXPR(NAME_REF)))"
    );
}

#[test]
fn a_block_carries_its_labels_at_both_ends() {
    assert_eq!(
        shape("begin : b x = 1; end : b\n"),
        "SOURCE_FILE(BLOCK(EXPR_STMT(ASSIGNMENT(NAME_REF LITERAL_EXPR))))"
    );
    for closer in ["join", "join_any", "join_none"] {
        let text = format!("fork x = 1; {closer}\n");
        assert_eq!(
            shape(&text),
            "SOURCE_FILE(BLOCK(EXPR_STMT(ASSIGNMENT(NAME_REF LITERAL_EXPR))))",
            "{closer}"
        );
    }
}

#[test]
fn the_statements_that_are_a_keyword_and_a_semicolon() {
    for (text, kind) in [
        ("return;\n", "RETURN_STMT"),
        ("return a + 1;\n", "RETURN_STMT"),
        ("break;\n", "BREAK_STMT"),
        ("continue;\n", "CONTINUE_STMT"),
        ("disable fork;\n", "DISABLE_STMT"),
        ("disable blk;\n", "DISABLE_STMT"),
        ("wait fork;\n", "WAIT_STMT"),
        ("-> ev;\n", "EVENT_TRIGGER"),
    ] {
        assert!(
            shape(text).starts_with(&format!("SOURCE_FILE({kind}")),
            "{text:?}"
        );
    }
}

// ------------------------------------------------------------------ timing

#[test]
fn a_sensitivity_list_is_read_rather_than_skipped() {
    assert_eq!(
        shape("@(posedge clk or negedge rst_n) q <= d;\n"),
        "SOURCE_FILE(TIMING_STMT(\
           EVENT_CONTROL(PAREN_EXPR(NAME_REF NAME_REF)) \
           EXPR_STMT(ASSIGNMENT(NAME_REF NAME_REF))))"
    );
}

#[test]
fn the_wildcard_forms_of_an_event_control() {
    for text in ["@* x = 1;\n", "@(*) x = 1;\n", "@ev x = 1;\n"] {
        assert!(
            shape(text).starts_with("SOURCE_FILE(TIMING_STMT(EVENT_CONTROL"),
            "{text:?} gave {}",
            shape(text)
        );
    }
}

#[test]
fn a_timing_control_may_be_the_whole_statement() {
    assert_eq!(
        shape("@(posedge clk);\n"),
        "SOURCE_FILE(TIMING_STMT(EVENT_CONTROL(PAREN_EXPR(NAME_REF))))"
    );
    assert_eq!(
        shape("#10;\n"),
        "SOURCE_FILE(TIMING_STMT(DELAY_CONTROL(LITERAL_EXPR)))"
    );
}

// ----------------------------------------------------------- falling back

#[test]
fn a_declaration_is_a_statement_too() {
    assert_eq!(
        shape("logic [3:0] x;\n"),
        "SOURCE_FILE(VAR_DECL(TYPE_REF(DIMENSION(LITERAL_EXPR LITERAL_EXPR)) DECLARATOR))"
    );
}

#[test]
fn a_label_belongs_to_what_follows_it() {
    assert_eq!(
        shape("lbl : x = 1;\n"),
        "SOURCE_FILE(LABELED_STMT(EXPR_STMT(ASSIGNMENT(NAME_REF LITERAL_EXPR))))"
    );
}

#[test]
fn a_statement_that_never_finds_its_semicolon_is_given_back() {
    // The fallback has to start where the statement started, not in the
    // middle of what the rule half understood.
    assert_eq!(shape("x = 1 end\n"), "SOURCE_FILE(VERBATIM VERBATIM)");
}

#[test]
fn what_stays_verbatim_stays_verbatim() {
    for text in [
        "assert property (@(posedge clk) a |-> b);\n",
        "randcase 1 : x = 1; endcase\n",
    ] {
        assert!(
            shape(text).contains("VERBATIM"),
            "{text:?} gave {}",
            shape(text)
        );
    }
}
