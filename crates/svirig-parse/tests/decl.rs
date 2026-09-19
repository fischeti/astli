//! Types, declarations, and what decides that something is one.

use svirig_parse::{Parser, Raw, build, declaration};
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

/// One declaration, and whatever it did not take.
fn parse(text: &str) -> (Option<SyntaxNode>, String) {
    let source = Source::new(text);
    let input = source.input();
    let mut parser = Parser::new(Raw::new(input));

    let file = parser.start();
    let taken = declaration(&mut parser).is_some();
    while !parser.at_end() {
        parser.bump();
    }
    parser.complete(file, SOURCE_FILE);

    let tree = SyntaxNode::new_root(build(&parser.finish().events, input));
    assert_eq!(tree.text().to_string(), text, "the tree is not the file");

    let node = taken.then(|| tree.children().next().expect("a declaration"));
    let rest = match node {
        Some(ref node) => text[usize::from(node.text_range().end())..].to_string(),
        None => text.to_string(),
    };
    (node, rest)
}

/// The node kinds a declaration builds, and how they nest.
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

    let (node, rest) = parse(text);
    assert_eq!(rest, "", "the declaration did not take all of `{text}`");
    let mut out = String::new();
    walk(&node.expect("a declaration"), &mut out);
    out
}

/// Whether the text declares anything at all.
fn declares(text: &str) -> bool {
    parse(text).0.is_some()
}

// ------------------------------------------------------------ the ambiguity

/// Every declaration in `text`, by kind.
fn last(text: &str) -> String {
    let source = Source::new(text);
    let input = source.input();
    let mut parser = Parser::new(Raw::new(input));

    let file = parser.start();
    while !parser.at_end() {
        if declaration(&mut parser).is_none() {
            parser.bump();
        }
    }
    parser.complete(file, SOURCE_FILE);
    let tree = SyntaxNode::new_root(build(&parser.finish().events, input));
    assert_eq!(tree.text().to_string(), text, "the tree is not the file");

    tree.children()
        .map(|node| format!("{:?}", node.kind()))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_name_declares_whether_or_not_this_file_gave_it() {
    // The `typedef` above makes no difference, and that is the point: two
    // names in a row are a declaration because nothing else is written that
    // way, not because the first one was resolved.
    assert_eq!(last("typedef int my_t;\nmy_t x;\n"), "TYPEDEF VAR_DECL");
    assert!(declares("unknown_t x;\n"));
    assert!(declares("uvm_reg_data_t mask [4];\n"));
}

#[test]
fn a_name_with_nothing_after_it_declares_nothing() {
    // One name is an expression, and a name the `(` follows is a call or an
    // instantiation. Neither is this rule's, and claiming them would be the
    // one way the shape test could be wrong.
    assert!(!declares("unknown_t;\n"));
    assert!(!declares("my_module inst (.a(b));\n"));
    assert!(!declares("x [3:0] = y;\n"));
}

#[test]
fn a_qualifier_carries_a_type_the_file_cannot_resolve() {
    // `const` cannot begin anything but a declaration, so the name after it
    // is a type whether or not this file knows the name.
    assert_eq!(
        shape("const uvm_reg_data_t mask = 0;"),
        "VAR_DECL(TYPE_REF DECLARATOR(LITERAL_EXPR))"
    );
}

#[test]
fn a_cast_is_not_a_declaration() {
    // `int'(x)` begins exactly as `int x` does, and the `'` is the whole
    // difference.
    assert!(!declares("int'(vs1) % 2 == 0;"));
    assert!(declares("int x;"));
}

#[test]
fn brackets_belong_to_the_type_when_a_name_follows_them() {
    // The same three shapes of token, and the brackets are the type's in one
    // and the name's in the other. Settled by looking rather than by knowing,
    // which is why an unresolvable type name still works here.
    assert_eq!(
        shape("localparam cfg_t [N-1:0] Configs = '0;"),
        "PARAM_DECL(TYPE_REF(DIMENSION(BIN_EXPR(NAME_REF LITERAL_EXPR) LITERAL_EXPR)) \
           DECLARATOR(LITERAL_EXPR))"
    );
    assert_eq!(
        shape("wire regs [4];"),
        "VAR_DECL(DECLARATOR(DIMENSION(LITERAL_EXPR)))"
    );
}

// ----------------------------------------------------------------- the types

#[test]
fn a_net_may_write_no_type_at_all() {
    assert_eq!(shape("wire x;"), "VAR_DECL(DECLARATOR)");
    assert_eq!(
        shape("wire [7:0] x;"),
        "VAR_DECL(TYPE_REF(DIMENSION(LITERAL_EXPR LITERAL_EXPR)) DECLARATOR)"
    );
}

#[test]
fn a_net_keeps_its_delay() {
    assert_eq!(shape("wire #0.1 a = b;"), "VAR_DECL(DECLARATOR(NAME_REF))");
}

#[test]
fn a_type_may_be_scoped_and_parameterised() {
    // A scope settles the shape on its own: nothing but a declaration is
    // written `pkg::t x;`.
    assert_eq!(shape("pkg::t x;"), "VAR_DECL(TYPE_REF DECLARATOR)");
    assert_eq!(
        shape("pkg::C #(W) x;"),
        "VAR_DECL(TYPE_REF(ARG_LIST(ARG(NAME_REF))) DECLARATOR)"
    );

    // Unscoped, the same shape needs the name to be one this file gave --
    // `my_mod #(W) inst ();` is an instantiation and looks identical until
    // the port list, which is the instantiation rule's to recognise.
    assert_eq!(last("typedef int C;\nC #(W) x;\n"), "TYPEDEF VAR_DECL");
}

#[test]
fn an_enum_keeps_a_node_per_name() {
    assert_eq!(
        shape("typedef enum logic [1:0] { A = 0, B } state_e;"),
        "TYPEDEF(ENUM_TYPE(TYPE_REF(DIMENSION(LITERAL_EXPR LITERAL_EXPR)) \
           ENUM_VARIANT(LITERAL_EXPR) ENUM_VARIANT) DECLARATOR)"
    );
}

#[test]
fn a_struct_keeps_a_node_per_member() {
    assert_eq!(
        shape("typedef struct packed { logic a; int b; } hdr_t;"),
        "TYPEDEF(STRUCT_TYPE(\
           STRUCT_MEMBER(TYPE_REF DECLARATOR) \
           STRUCT_MEMBER(TYPE_REF DECLARATOR)) DECLARATOR)"
    );
    assert!(shape("typedef union { int a; } u_t;").starts_with("TYPEDEF(UNION_TYPE("));
}

#[test]
fn a_randomised_member_carries_its_qualifier() {
    assert_eq!(
        shape("typedef struct { rand mubi4_t en; } r_t;"),
        "TYPEDEF(STRUCT_TYPE(STRUCT_MEMBER(TYPE_REF DECLARATOR)) DECLARATOR)"
    );
}

#[test]
fn a_forward_typedef_is_told_from_a_definition_by_what_follows() {
    // Both begin `typedef enum`, and only what comes after the keyword says
    // whether a type is being promised or written out.
    assert_eq!(shape("typedef enum e;"), "TYPEDEF(DECLARATOR)");
    assert!(shape("typedef enum logic [1:0] { A } e;").starts_with("TYPEDEF(ENUM_TYPE("));
    assert_eq!(shape("typedef class C;"), "TYPEDEF(DECLARATOR)");
    assert_eq!(shape("typedef interface class C;"), "TYPEDEF(DECLARATOR)");
}

#[test]
fn a_type_parameter_takes_a_type_rather_than_a_value() {
    assert_eq!(
        shape("localparam type t = struct packed { logic a; };"),
        "PARAM_DECL(DECLARATOR(STRUCT_TYPE(STRUCT_MEMBER(TYPE_REF DECLARATOR))))"
    );
}

#[test]
fn a_virtual_interface_is_a_type() {
    assert_eq!(
        shape("virtual interface i_if vif;"),
        "VAR_DECL(TYPE_REF DECLARATOR)"
    );
}

// --------------------------------------------------------- the preprocessor

#[test]
fn a_macro_may_stand_for_the_name_being_declared() {
    // Level B: an atom in every position, and a declarator's name is one.
    assert_eq!(
        shape("logic [31:0] `X(mcause);"),
        "VAR_DECL(TYPE_REF(DIMENSION(LITERAL_EXPR LITERAL_EXPR)) \
           DECLARATOR(MACRO_CALL(MACRO_ARG_LIST(MACRO_ARG))))"
    );
}

#[test]
fn a_macro_in_an_enum_brings_its_own_comma() {
    // A macro that writes several names writes the separators between them
    // too, so the list must not insist on seeing one after it.
    assert_eq!(
        shape("typedef enum { A, `MORE(x) B } e;"),
        "TYPEDEF(ENUM_TYPE(\
           ENUM_VARIANT \
           ENUM_VARIANT(MACRO_CALL(MACRO_ARG_LIST(MACRO_ARG))) \
           ENUM_VARIANT) DECLARATOR)"
    );
}

#[test]
fn a_macro_may_supply_a_literals_digits() {
    assert_eq!(
        shape("parameter int unsigned Base = 32'h`DM_ADDR;"),
        "PARAM_DECL(TYPE_REF DECLARATOR(LITERAL_EXPR(MACRO_CALL)))"
    );
}

// ------------------------------------------------------------ all or nothing

#[test]
fn a_declaration_that_never_reaches_its_semicolon_is_given_back_whole() {
    // A node over some prefix would put the fallback in the middle of what
    // was misread. Nothing taken means the fallback starts where the
    // declaration did.
    let (node, rest) = parse("logic a b c");
    assert!(node.is_none());
    assert_eq!(rest, "logic a b c");
}
