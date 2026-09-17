//! Precedence, and the shapes an expression takes.

use rowan::NodeOrToken;
use svirig_syntax::parser::{Parser, Raw, build, expr};
use svirig_syntax::preproc::{Input, Preprocessor};
use svirig_syntax::{SyntaxKind::*, SyntaxNode};
use svirig_text::FileId;

struct Source {
    pp: Preprocessor<'static>,
    file: FileId,
}

impl Source {
    fn new(text: &str) -> Source {
        let mut pp = Preprocessor::new();
        let file = pp.add("top.sv", text.to_string());
        Source { pp, file }
    }

    fn input(&self) -> Input<'_> {
        self.pp.input(self.file)
    }
}

/// One expression, and whatever it did not take.
///
/// Whatever is left over is bumped in so that the tree is still the file --
/// the invariant holds for a rule that stops early exactly as it does for one
/// that finishes.
fn parse(text: &str) -> (Option<SyntaxNode>, String) {
    let source = Source::new(text);
    let input = source.input();
    let mut parser = Parser::new(Raw::new(input));

    let file = parser.start();
    let taken = expr(&mut parser).is_some();
    while !parser.at_end() {
        parser.bump();
    }
    parser.complete(file, SOURCE_FILE);

    let tree = SyntaxNode::new_root(build(&parser.finish(), input));
    assert_eq!(tree.text().to_string(), text, "the tree is not the file");

    let node = taken.then(|| tree.children().next().expect("an expression"));
    let rest = match node {
        Some(ref node) => text[usize::from(node.text_range().end())..].to_string(),
        None => text.to_string(),
    };
    (node, rest)
}

/// The expression's nesting, written with parentheses and the tokens as
/// themselves.
///
/// For precedence, which is about shape rather than about which node kind
/// carries it: `a + b * c` reads back as `(a + (b * c))`.
fn nesting(text: &str) -> String {
    fn walk(node: &SyntaxNode, out: &mut String) {
        out.push('(');
        let mut first = true;
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(child) => {
                    if !first {
                        out.push(' ');
                    }
                    walk(&child, out);
                    first = false;
                }
                NodeOrToken::Token(token) if !token.kind().is_trivia() => {
                    if !first {
                        out.push(' ');
                    }
                    out.push_str(token.text());
                    first = false;
                }
                NodeOrToken::Token(_) => {}
            }
        }
        out.push(')');
        // A node with one token in it is that token; the parentheses would be
        // noise on every leaf.
        let inner = out[out.rfind('(').map_or(0, |at| at + 1)..out.len() - 1].to_string();
        if !inner.contains(' ') && !inner.contains('(') {
            let at = out.rfind('(').unwrap();
            out.truncate(at);
            out.push_str(&inner);
        }
    }

    let (node, _) = parse(text);
    let mut out = String::new();
    walk(&node.expect("an expression"), &mut out);
    out
}

/// The node kinds an expression builds, and how they nest.
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

    let (node, _) = parse(text);
    let mut out = String::new();
    walk(&node.expect("an expression"), &mut out);
    out
}

// ---------------------------------------------------------------- precedence

#[test]
fn multiplication_binds_tighter_than_addition() {
    assert_eq!(nesting("a + b * c"), "(a + (b * c))");
    assert_eq!(nesting("a * b + c"), "((a * b) + c)");
}

#[test]
fn the_ordinary_operators_are_left_associative() {
    assert_eq!(nesting("a - b - c"), "((a - b) - c)");
    assert_eq!(nesting("a / b / c"), "((a / b) / c)");
}

#[test]
fn power_is_right_associative() {
    assert_eq!(nesting("a ** b ** c"), "(a ** (b ** c))");
}

#[test]
fn a_prefix_operator_binds_tighter_than_power() {
    // Where SystemVerilog parts company with most languages: 11.3.2 puts the
    // unary operators above `**`, so this is `(-2) ** 2` and not `-(2 ** 2)`.
    assert_eq!(nesting("-2 ** 2"), "((- 2) ** 2)");
}

#[test]
fn the_logical_operators_climb_in_the_order_the_table_gives() {
    assert_eq!(nesting("a || b && c"), "(a || (b && c))");
    assert_eq!(nesting("a && b | c"), "(a && (b | c))");
    assert_eq!(nesting("a | b ^ c"), "(a | (b ^ c))");
    assert_eq!(nesting("a ^ b & c"), "(a ^ (b & c))");
    assert_eq!(nesting("a & b == c"), "(a & (b == c))");
    assert_eq!(nesting("a == b < c"), "(a == (b < c))");
    assert_eq!(nesting("a < b << c"), "(a < (b << c))");
}

#[test]
fn the_conditional_operator_nests_to_the_right() {
    assert_eq!(nesting("a ? b : c ? d : e"), "(a ? b : (c ? d : e))");
}

#[test]
fn the_conditional_operator_is_looser_than_any_binary_one() {
    assert_eq!(nesting("a || b ? c + 1 : d"), "((a || b) ? (c + 1) : d)");
}

#[test]
fn implication_is_looser_than_the_conditional_operator() {
    assert_eq!(nesting("a -> b ? c : d"), "(a -> (b ? c : d))");
}

// ------------------------------------------------------------------ literals

#[test]
fn a_number_written_in_pieces_is_one_value() {
    // 5.7.1 lets whitespace separate a size from its base and a base from its
    // digits. All three of these are one literal, and the node is what stops a
    // formatter coming between them.
    assert_eq!(shape("8'hFF"), "LITERAL_EXPR");
    assert_eq!(nesting("8'hFF"), "(8 'hFF)");
    assert_eq!(nesting("8 'h FF"), "(8 'h FF)");
    assert_eq!(nesting("'h FF"), "('h FF)");
    assert_eq!(shape("'0"), "LITERAL_EXPR");
}

#[test]
fn a_number_in_pieces_is_still_one_operand() {
    assert_eq!(nesting("8 'h FF + 1"), "((8 'h FF) + 1)");
}

#[test]
fn separated_digits_may_be_more_than_one_token() {
    // `4a43_f880` lexes as an integer and then an identifier, because the run
    // starts as a number and stops being one. Nothing separates them, which
    // is what says they are one value.
    assert_eq!(shape("256'h 4a43_f880"), "LITERAL_EXPR");
    assert_eq!(nesting("256'h 4a43_f880"), "(256 'h 4 a43_f880)");
    // And the adjacency is load-bearing: a space makes the next token an
    // operand of its own rather than more digits.
    assert_eq!(nesting("8 'h FF + 1"), "((8 'h FF) + 1)");
}

// ------------------------------------------------------------------ postfix

#[test]
fn postfix_reopens_what_precedes_it() {
    assert_eq!(
        shape("a.b[3].c(1)"),
        "CALL_EXPR(FIELD_EXPR(INDEX_EXPR(FIELD_EXPR(NAME_REF) LITERAL_EXPR)) ARG_LIST(ARG(LITERAL_EXPR)))"
    );
}

#[test]
fn a_scope_reference_chains_like_any_other() {
    assert_eq!(shape("pkg::T::x"), "SCOPE_EXPR(SCOPE_EXPR(NAME_REF))");
}

#[test]
fn a_part_select_keeps_both_of_its_bounds() {
    assert_eq!(nesting("a[hi:lo]"), "(a [ hi : lo ])");
    assert_eq!(nesting("a[base+:width]"), "(a [ base +: width ])");
    assert_eq!(nesting("a[base-:width]"), "(a [ base -: width ])");
}

#[test]
fn a_cast_is_a_postfix() {
    assert_eq!(shape("int'(x)"), "CAST_EXPR(NAME_REF PAREN_EXPR(NAME_REF))");
    assert_eq!(
        shape("8'(y)"),
        "CAST_EXPR(LITERAL_EXPR PAREN_EXPR(NAME_REF))"
    );
    assert_eq!(
        shape("T::U'(z)"),
        "CAST_EXPR(SCOPE_EXPR(NAME_REF) PAREN_EXPR(NAME_REF))"
    );
}

#[test]
fn an_increment_is_a_prefix_or_a_postfix() {
    assert_eq!(shape("++a"), "UNARY_EXPR(NAME_REF)");
    assert_eq!(shape("a++"), "POSTFIX_EXPR(NAME_REF)");
}

// ------------------------------------------------------------------- braces

#[test]
fn a_brace_opens_a_concatenation_or_a_replication() {
    assert_eq!(shape("{a, b}"), "CONCAT_EXPR(NAME_REF NAME_REF)");
    assert_eq!(
        shape("{4{a}}"),
        "REPLICATION_EXPR(LITERAL_EXPR CONCAT_EXPR(NAME_REF))"
    );
}

#[test]
fn a_stream_keeps_its_direction_and_its_slice() {
    assert_eq!(shape("{<<{a}}"), "STREAM_EXPR(CONCAT_EXPR(NAME_REF))");
    assert_eq!(
        shape("{>>4{a, b}}"),
        "STREAM_EXPR(LITERAL_EXPR CONCAT_EXPR(NAME_REF NAME_REF))"
    );
}

#[test]
fn a_pattern_may_name_the_type_it_builds() {
    // `T'{...}` is one pattern with a type, not a cast applied to a pattern:
    // the lexer gives `'{` as a single token, so there is no `'` to cast with.
    assert_eq!(
        shape("req_t'{a: 1}"),
        "ASSIGNMENT_PATTERN(NAME_REF PATTERN_ITEM(NAME_REF LITERAL_EXPR))"
    );
}

#[test]
fn a_pattern_element_may_be_a_replication() {
    assert_eq!(
        shape("'{4{a}}"),
        "ASSIGNMENT_PATTERN(PATTERN_ITEM(REPLICATION_EXPR(LITERAL_EXPR CONCAT_EXPR(NAME_REF))))"
    );
}

#[test]
fn an_assignment_pattern_keeps_the_key_with_its_value() {
    assert_eq!(
        shape("'{default: 0}"),
        "ASSIGNMENT_PATTERN(PATTERN_ITEM(NAME_REF LITERAL_EXPR))"
    );
    assert_eq!(
        shape("'{a, b}"),
        "ASSIGNMENT_PATTERN(PATTERN_ITEM(NAME_REF) PATTERN_ITEM(NAME_REF))"
    );
}

// ------------------------------------------------------- inside, dist, with

#[test]
fn inside_takes_a_list_of_values_and_ranges() {
    assert_eq!(
        shape("a inside {1, [2:3]}"),
        "INSIDE_EXPR(NAME_REF RANGE_LIST(LITERAL_EXPR LITERAL_EXPR LITERAL_EXPR))"
    );
    assert_eq!(
        nesting("a inside {1, [2:3]}"),
        "(a inside ({ 1 , [ 2 : 3 ] }))"
    );
}

#[test]
fn dist_weights_each_of_its_elements() {
    assert_eq!(
        shape("a dist {1 := 2, 3 :/ 4}"),
        "DIST_EXPR(NAME_REF RANGE_LIST(\
           DIST_ITEM(LITERAL_EXPR LITERAL_EXPR) \
           DIST_ITEM(LITERAL_EXPR LITERAL_EXPR)))"
    );
}

#[test]
fn a_with_clause_hangs_off_the_call_it_qualifies() {
    assert_eq!(
        shape("q.find with (item > 3)"),
        "CALL_EXPR(FIELD_EXPR(NAME_REF) WITH_CLAUSE(PAREN_EXPR(BIN_EXPR(NAME_REF LITERAL_EXPR))))"
    );
}

// -------------------------------------------------------------- attributes

#[test]
fn an_attribute_may_sit_between_an_operator_and_its_operand() {
    // The one place 11.3.2 admits one inside an expression. It is told from a
    // parenthesised expression by lookahead: `*` is not a prefix operator, so
    // `(` followed by one can only open an attribute.
    assert_eq!(
        shape("a + (* full_case *) b"),
        "BIN_EXPR(NAME_REF ATTRIBUTES(ATTRIBUTE_SPEC) NAME_REF)"
    );
    assert_eq!(
        shape("a + (* x = 1 *) b"),
        "BIN_EXPR(NAME_REF ATTRIBUTES(ATTRIBUTE_SPEC(LITERAL_EXPR)) NAME_REF)"
    );
}

#[test]
fn a_parenthesised_expression_is_not_an_attribute() {
    assert_eq!(shape("(a)"), "PAREN_EXPR(NAME_REF)");
    assert_eq!(nesting("(a * b)"), "(( (a * b) ))");
}

// ------------------------------------------------------------ preprocessor

#[test]
fn a_macro_reference_is_an_operand() {
    // It may stand for a value, a name, or a whole subexpression, and raw mode
    // cannot know which -- so it is an atom, and that is what lets ordinary
    // RTL written against macros parse at all.
    assert_eq!(
        shape("`WIDTH - 1"),
        "BIN_EXPR(NAME_REF(MACRO_CALL) LITERAL_EXPR)"
    );
    assert_eq!(
        shape("`MAX(a, b) + 1"),
        "BIN_EXPR(\
           NAME_REF(MACRO_CALL(MACRO_ARG_LIST(MACRO_ARG MACRO_ARG))) \
           LITERAL_EXPR)"
    );
}

// --------------------------------------------------------------- stopping

#[test]
fn nothing_that_starts_an_expression_means_nothing_taken() {
    let (node, rest) = parse("; a");
    assert!(node.is_none());
    assert_eq!(rest, "; a");
}

#[test]
fn a_prefix_operator_with_no_operand_is_given_back_whole() {
    // `None` has to mean nothing was consumed, or a caller cannot fall back:
    // the tokens it is about to hand to the fallback would already be gone.
    let (node, rest) = parse("- ;");
    assert!(node.is_none());
    assert_eq!(rest, "- ;");
}

#[test]
fn an_expression_stops_at_the_first_token_it_cannot_use() {
    let (_, rest) = parse("a + b; c");
    assert_eq!(rest, "; c");
}

#[test]
fn a_trailing_operator_leaves_the_operator_behind() {
    // There is no error node to put a missing operand in, so the rule takes
    // what it can and hands the rest back for the caller to fall back on.
    let (node, rest) = parse("a +");
    assert!(node.is_some());
    assert_eq!(rest, " +");
}
