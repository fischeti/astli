//! Descriptions, their headers, and the items inside them.

use rowan::NodeOrToken;
use svirig_parse::parse;
use svirig_preproc::{Input, Session};
use svirig_syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode};
use svirig_text::FileId;

mod corpus;

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

fn tree(text: &str) -> SyntaxNode {
    let source = Source::new(text);
    let tree = parse(source.input());
    assert_eq!(tree.text().to_string(), text, "the tree is not the file");
    tree
}

/// The node kinds one file builds, and how they nest.
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

/// The text of the first node of `kind`, or `None` if there is none.
fn first(text: &str, kind: SyntaxKind) -> Option<String> {
    tree(text)
        .descendants()
        .find(|node| node.kind() == kind)
        .map(|node| node.text().to_string().trim().to_string())
}

// ------------------------------------------------------------------- shells

#[test]
fn the_four_descriptions_are_one_shape() {
    for (text, kind) in [
        ("module m;\nendmodule\n", MODULE_DECL),
        ("macromodule m;\nendmodule\n", MODULE_DECL),
        ("interface i;\nendinterface\n", INTERFACE_DECL),
        ("program p;\nendprogram\n", PROGRAM_DECL),
        ("package pkg;\nendpackage\n", PACKAGE_DECL),
        ("class C;\nendclass\n", CLASS_DECL),
        ("virtual class C;\nendclass\n", CLASS_DECL),
        ("interface class C;\nendclass\n", CLASS_DECL),
    ] {
        assert_eq!(shape(text), format!("SOURCE_FILE({kind:?})"), "{text:?}");
    }
}

#[test]
fn a_shell_that_never_closes_is_given_back() {
    // All or nothing. A `MODULE_DECL` over a module and everything after it
    // would lower the rate M3 is graded on *by being wrong*, which is the one
    // way the metric could lie.
    assert_eq!(
        shape("module m;\n  assign x = 1;\n"),
        "SOURCE_FILE(VERBATIM)"
    );
}

#[test]
fn a_label_belongs_to_the_shell_that_wrote_it() {
    assert_eq!(
        first("module m;\nendmodule : m\n", MODULE_DECL).as_deref(),
        Some("module m;\nendmodule : m")
    );
}

#[test]
fn a_header_writes_its_parameters_then_its_ports() {
    assert_eq!(
        shape(
            "module m #(parameter int W = 8) (input logic clk, output logic [W-1:0] d);\nendmodule\n"
        ),
        "SOURCE_FILE(MODULE_DECL(\
           PARAM_PORT_LIST(PARAM_DECL(TYPE_REF DECLARATOR(LITERAL_EXPR))) \
           PORT_LIST(\
             PORT(TYPE_REF DECLARATOR) \
             PORT(TYPE_REF(DIMENSION(BIN_EXPR(NAME_REF LITERAL_EXPR) LITERAL_EXPR)) DECLARATOR))))"
    );
}

#[test]
fn a_parameter_list_separates_elements_rather_than_declarators() {
    // The `,` here ends the element, and a declarator loop that took it would
    // find the keyword of the *next* element where a name should be.
    assert_eq!(
        shape("module m #(parameter int A = 1, localparam int B = 2) ();\nendmodule\n"),
        "SOURCE_FILE(MODULE_DECL(\
           PARAM_PORT_LIST(\
             PARAM_DECL(TYPE_REF DECLARATOR(LITERAL_EXPR)) \
             PARAM_DECL(TYPE_REF DECLARATOR(LITERAL_EXPR))) \
           PORT_LIST))"
    );
}

#[test]
fn a_non_ansi_header_names_its_ports_and_declares_them_after() {
    assert_eq!(
        shape("module m (a, b);\n  input logic a;\n  output b;\nendmodule\n"),
        "SOURCE_FILE(MODULE_DECL(\
           PORT_LIST(PORT(DECLARATOR) PORT(DECLARATOR)) \
           PORT_DECL(TYPE_REF DECLARATOR) \
           PORT_DECL(DECLARATOR)))"
    );
}

#[test]
fn one_port_the_rules_cannot_shape_costs_that_port_and_no_more() {
    // The list is what a header hangs on, so an element that no rule fits is
    // a run of its own rather than the end of the header.
    assert_eq!(
        shape("module m (input logic a, output wire (* x *) , input logic b);\nendmodule\n"),
        "SOURCE_FILE(MODULE_DECL(PORT_LIST(\
           PORT(TYPE_REF DECLARATOR) \
           VERBATIM \
           PORT(TYPE_REF DECLARATOR))))"
    );
}

#[test]
fn a_header_may_import_before_it_takes_parameters() {
    assert_eq!(
        shape("module m import pkg::*; #(parameter W = 1) ();\nendmodule\n"),
        "SOURCE_FILE(MODULE_DECL(\
           IMPORT_DECL \
           PARAM_PORT_LIST(PARAM_DECL(DECLARATOR(LITERAL_EXPR))) \
           PORT_LIST))"
    );
}

#[test]
fn an_import_that_never_finds_its_semicolon_gives_back_only_itself() {
    // Its own tokens, not the module's: a rule that rolled the shell back
    // would be rolling across a marker still open, which `Events` refuses.
    assert_eq!(
        shape("module m import pkg::*\nendmodule\n"),
        "SOURCE_FILE(MODULE_DECL(VERBATIM))"
    );
}

#[test]
fn a_class_writes_what_it_extends_and_what_it_implements() {
    assert_eq!(
        shape("class C #(type T = int) extends B #(T) implements I, J;\nendclass\n"),
        "SOURCE_FILE(CLASS_DECL(\
           PARAM_PORT_LIST(PARAM_DECL(DECLARATOR(TYPE_REF))) \
           TYPE_REF(ARG_LIST(ARG(NAME_REF))) \
           TYPE_REF \
           TYPE_REF))"
    );
}

// -------------------------------------------------------------------- items

#[test]
fn an_instantiation_is_told_from_a_declaration_by_its_parenthesis() {
    // Neither name is one this file gave, and it makes no difference: the
    // `(` is the whole of what separates the two.
    assert_eq!(
        shape("module m;\n  foo_t x;\n  foo u_foo (.a(b), .*);\nendmodule\n"),
        "SOURCE_FILE(MODULE_DECL(\
           VAR_DECL(TYPE_REF DECLARATOR) \
           INSTANTIATION(TYPE_REF INSTANCE(ARG_LIST(ARG(PAREN_EXPR(NAME_REF)) ARG)))))"
    );
}

#[test]
fn an_instantiation_may_override_parameters_and_name_several_instances() {
    assert_eq!(
        shape("module m;\n  foo #(.W(8)) u_a (), u_b [1:0] ();\nendmodule\n"),
        "SOURCE_FILE(MODULE_DECL(INSTANTIATION(\
           TYPE_REF \
           ARG_LIST(ARG(PAREN_EXPR(LITERAL_EXPR))) \
           INSTANCE(ARG_LIST(ARG)) \
           INSTANCE(DIMENSION(LITERAL_EXPR LITERAL_EXPR) ARG_LIST(ARG)))))"
    );
}

#[test]
fn a_continuous_assign_takes_every_assignment_it_writes() {
    assert_eq!(
        shape("module m;\n  assign a = b, c = d;\nendmodule\n"),
        "SOURCE_FILE(MODULE_DECL(CONTINUOUS_ASSIGN(\
           ASSIGNMENT(NAME_REF NAME_REF) \
           ASSIGNMENT(NAME_REF NAME_REF))))"
    );
}

#[test]
fn the_six_procedural_keywords_are_one_kind() {
    for keyword in [
        "always",
        "always_comb",
        "always_ff",
        "always_latch",
        "initial",
        "final",
    ] {
        let text = format!("module m;\n  {keyword} begin end\nendmodule\n");
        assert_eq!(
            shape(&text),
            "SOURCE_FILE(MODULE_DECL(PROCEDURAL_BLOCK(BLOCK)))",
            "{keyword}"
        );
    }
}

#[test]
fn a_subroutine_body_holds_statements_and_a_module_body_holds_items() {
    // The one field the scope is: `x = 1;` is a statement in the first and
    // nothing the item rules admit in the second.
    assert_eq!(
        shape("module m;\n  function int f();\n    x = 1;\n  endfunction\nendmodule\n"),
        "SOURCE_FILE(MODULE_DECL(FUNCTION_DECL(\
           TYPE_REF PORT_LIST EXPR_STMT(ASSIGNMENT(NAME_REF LITERAL_EXPR)))))"
    );
    assert_eq!(
        shape("module m;\n  x = 1;\nendmodule\n"),
        "SOURCE_FILE(MODULE_DECL(VERBATIM))"
    );
}

#[test]
fn a_prototype_has_no_body_to_look_for() {
    for text in [
        "class C;\n  extern function void f();\nendclass\n",
        "class C;\n  pure virtual function int g();\nendclass\n",
        "package p;\n  import \"DPI-C\" function void h(input int a);\nendpackage\n",
        "package p;\n  import \"DPI-C\" context c_name = function void k();\nendpackage\n",
    ] {
        let tree = tree(text);
        let found = tree
            .descendants()
            .any(|node| node.kind() == FUNCTION_DECL && !node.text().to_string().contains("end"));
        assert!(found, "{text:?} gave {}", shape(text));
    }
}

#[test]
fn a_return_type_is_told_from_a_name_by_what_follows_the_scope() {
    assert_eq!(
        first(
            "class C;\n  function pkg::t f();\n  endfunction\nendclass\n",
            TYPE_REF
        )
        .as_deref(),
        Some("pkg::t")
    );
    // No return type: the `::` chain runs straight into the ports.
    assert_eq!(
        first("function void C::f();\nendfunction\n", TYPE_REF).as_deref(),
        Some("void")
    );
    assert_eq!(first("function f();\nendfunction\n", TYPE_REF), None);
}

#[test]
fn a_generate_construct_is_the_statement_shape_over_an_item_body() {
    assert_eq!(
        shape(
            "module m;\ngenerate\n  for (genvar i = 0; i < 2; i++) begin : g\n    assign x = i;\n  end\nendgenerate\nendmodule\n"
        ),
        "SOURCE_FILE(MODULE_DECL(GENERATE_REGION(FOR_STMT(\
           PAREN_EXPR(VAR_DECL(DECLARATOR(LITERAL_EXPR)) BIN_EXPR(NAME_REF LITERAL_EXPR) POSTFIX_EXPR(NAME_REF)) \
           BLOCK(CONTINUOUS_ASSIGN(ASSIGNMENT(NAME_REF NAME_REF)))))))"
    );
}

#[test]
fn a_generate_conditional_needs_no_generate_around_it() {
    assert_eq!(
        shape(
            "module m;\n  if (W > 1) begin\n    assign a = b;\n  end else begin\n  end\nendmodule\n"
        ),
        "SOURCE_FILE(MODULE_DECL(IF_STMT(\
           PAREN_EXPR(BIN_EXPR(NAME_REF LITERAL_EXPR)) \
           BLOCK(CONTINUOUS_ASSIGN(ASSIGNMENT(NAME_REF NAME_REF))) \
           BLOCK)))"
    );
}

#[test]
fn a_modport_writes_a_port_list_per_name() {
    assert_eq!(
        shape("interface i;\n  modport ctrl (input a, output b), dev (input b);\nendinterface\n"),
        "SOURCE_FILE(INTERFACE_DECL(MODPORT_DECL(\
           MODPORT(PORT_LIST(PORT(DECLARATOR) PORT(DECLARATOR))) \
           MODPORT(PORT_LIST(PORT(DECLARATOR))))))"
    );
}

#[test]
fn a_constraint_ends_at_its_brace_rather_than_at_a_semicolon() {
    // The whole reason it has a rule: the fallback reads a closing bracket as
    // no boundary, so without this the run would carry on into the member
    // after it.
    assert_eq!(
        shape("class C;\n  constraint c { a inside {[0:3]}; }\n  int x;\nendclass\n"),
        "SOURCE_FILE(CLASS_DECL(CONSTRAINT_DECL(VERBATIM) VAR_DECL(TYPE_REF DECLARATOR)))"
    );
    assert_eq!(
        shape("class C;\n  extern constraint c;\nendclass\n"),
        "SOURCE_FILE(CLASS_DECL(VERBATIM))"
    );
}

#[test]
fn attributes_belong_to_the_item_they_are_written_in_front_of() {
    assert_eq!(
        shape("module m;\n  (* keep *) logic x;\nendmodule\n"),
        "SOURCE_FILE(MODULE_DECL(VAR_DECL(ATTRIBUTES(ATTRIBUTE_SPEC) TYPE_REF DECLARATOR)))"
    );
}

// ---------------------------------------------------------------- the corpus

/// What every shell claims: the keyword it opened with, and the `end…` that
/// matches it.
///
/// Cheap, and the one assertion that would catch a rule closing a node over
/// text it never read -- which is how the [ratchet](../verbatim.rs) could fall
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
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let mut session = Session::new();
        let file = session.add(path, text);
        walk(
            &parse(session.input(file)),
            &path.display().to_string(),
            &mut bad,
        );
    }

    assert!(
        bad.is_empty(),
        "{} malformed:\n{}",
        bad.len(),
        bad.join("\n")
    );
}
