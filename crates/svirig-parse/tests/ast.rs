//! The typed views, over trees the parser really builds.
//!
//! What is under test is the accessors that are not a plain "first child of
//! this type": those that resolve by position among children of overlapping
//! types, and those written by hand.

use svirig_parse::SyntaxTree;
use svirig_syntax::SyntaxKind::*;
use svirig_syntax::ast::{self, AstNode};

/// The first node of type `N` in `text`.
fn first<N: AstNode>(text: &str) -> N {
    let tree = SyntaxTree::parse("top.sv", text.to_string());
    tree.root()
        .descendants()
        .find_map(N::cast)
        .unwrap_or_else(|| panic!("nothing of that type in {text:?}"))
}

fn text(node: &impl AstNode) -> String {
    node.syntax().text().to_string().trim().to_string()
}

#[test]
fn a_binary_expression_tells_its_operands_apart_by_position() {
    let bin: ast::BinExpr = first("assign x = a + b * c;");
    assert_eq!(text(&bin.lhs().expect("lhs")), "a");
    assert_eq!(text(&bin.rhs().expect("rhs")), "b * c");
    assert_eq!(bin.op().expect("op").kind(), PLUS);
}

#[test]
fn a_cast_is_a_type_then_a_parenthesised_operand() {
    // Both children are expressions, and the second is also a ParenExpr, so
    // only position separates them -- including when the type is itself
    // parenthesised.
    let cast: ast::CastExpr = first("assign x = int'(y);");
    assert_eq!(text(&cast.ty().expect("ty")), "int");
    assert_eq!(text(&cast.operand().expect("operand")), "(y)");

    let sized: ast::CastExpr = first("assign x = (W)'(y);");
    assert_eq!(text(&sized.ty().expect("ty")), "(W)");
    assert_eq!(text(&sized.operand().expect("operand")), "(y)");
}

#[test]
fn an_if_has_a_condition_and_one_or_two_branches() {
    let with_else: ast::IfStmt = first("initial if (a) x = 1; else x = 2;");
    assert_eq!(text(&with_else.condition().expect("condition")), "(a)");
    assert_eq!(text(&with_else.then_branch().expect("then")), "x = 1;");
    assert_eq!(text(&with_else.else_branch().expect("else")), "x = 2;");

    let without: ast::IfStmt = first("initial if (a) x = 1;");
    assert!(without.else_branch().is_none());
}

#[test]
fn an_index_expression_has_an_end_only_when_it_selects_a_range() {
    let single: ast::IndexExpr = first("assign x = a[i];");
    assert_eq!(text(&single.base().expect("base")), "a");
    assert_eq!(text(&single.index().expect("index")), "i");
    assert!(single.end().is_none());

    let range: ast::IndexExpr = first("assign x = a[hi:lo];");
    assert_eq!(text(&range.end().expect("end")), "lo");
}

#[test]
fn a_module_names_itself_and_holds_its_items() {
    let module: ast::ModuleDecl = first(
        "module m #(parameter W = 1) (input a);\n  logic x;\n  assign y = x;\nendmodule : m\n",
    );
    assert_eq!(module.name().expect("name").text(), "m");
    assert!(module.param_port_list().is_some());
    assert_eq!(module.port_list().expect("ports").ports().count(), 1);
    let items: Vec<ast::Item> = module.items().collect();
    assert!(matches!(
        items.as_slice(),
        [ast::Item::VarDecl(_), ast::Item::ContinuousAssign(_)]
    ));
}

#[test]
fn a_union_views_whatever_is_in_it() {
    let module: ast::ModuleDecl = first("module m;\n  `uvm_info(\"T\", \"m\", LOW)\nendmodule\n");
    let item = module.items().next().expect("an item");
    let ast::Item::Preproc(ast::Preproc::MacroCall(call)) = item else {
        panic!("a macro call, not {item:?}");
    };
    assert_eq!(call.name().expect("name").text(), "`uvm_info");
    assert_eq!(
        call.macro_arg_list()
            .expect("arguments")
            .macro_args()
            .count(),
        3
    );
}

#[test]
fn a_pattern_item_has_a_key_only_when_it_writes_one() {
    let keyed: ast::PatternItem = first("assign x = '{a: 1};");
    assert_eq!(text(&keyed.key().expect("key")), "a");
    assert_eq!(text(&keyed.value().expect("value")), "1");

    let positional: ast::PatternItem = first("assign x = '{b};");
    assert!(positional.key().is_none());
    assert_eq!(text(&positional.value().expect("value")), "b");
}

#[test]
fn a_stream_has_a_slice_only_when_it_writes_one() {
    let sliced: ast::StreamExpr = first("assign x = {>>4{a, b}};");
    assert_eq!(text(&sliced.slice().expect("slice")), "4");
    assert_eq!(text(&sliced.concat().expect("concat")), "{a, b}");

    let plain: ast::StreamExpr = first("assign x = {<<{a}};");
    assert!(plain.slice().is_none());
    assert_eq!(text(&plain.concat().expect("concat")), "{a}");
}

#[test]
fn a_class_says_what_it_extends_and_what_it_implements() {
    let class: ast::ClassDecl = first("class C extends B #(T) implements I, J;\nendclass\n");
    assert_eq!(text(&class.extends().expect("extends")), "B #(T)");
    let implements: Vec<String> = class.implements().map(|ty| text(&ty)).collect();
    assert_eq!(implements, ["I", "J"]);

    let bare: ast::ClassDecl = first("class C implements I;\nendclass\n");
    assert!(bare.extends().is_none());
}
