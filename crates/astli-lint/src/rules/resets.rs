//! Resets: how a flop takes one, and where else a reset may be read.

use astli_syntax::SyntaxKind::{
    ALWAYS_FF_KW, ALWAYS_KW, ARG, ARG_LIST, ASSIGNMENT, BANG, COMMA, CONCAT_EXPR,
    CONCURRENT_ASSERTION, COVERGROUP_DECL, DECLARATOR, DEFAULT_DISABLE, EVENT_CONTROL, FIELD_EXPR,
    IMMEDIATE_ASSERTION, INDEX_EXPR, INSTANCE, NEGEDGE_KW, OR_KW, PAREN_EXPR, POSEDGE_KW,
    PROPERTY_DECL, REPLICATION_EXPR, SEQUENCE_DECL, TILDE,
};
use astli_syntax::ast::{
    Assignment, AstNode, Block, Declarator, EventControl, Expr, ForStmt, ForeachStmt, IfStmt, Item,
    LabeledStmt, NameRef, ParenExpr, ProceduralBlock, TimingStmt, TypeOrExpr,
};
use astli_syntax::{SyntaxNode, SyntaxToken};
use rustc_hash::FxHashSet;

use super::expressions::one_bit_comparison_of;
use super::procedural::{edges, waits_again};
use crate::options::{Pattern, ResetKind};
use crate::rule::Cx;

/// A reset that reaches each flop as it entered the design, and is read
/// only where a flop takes it, keeps reset timing what the reset tree
/// makes it: logic on a reset glitches it, and logic reading one ties
/// what the design does to when reset ends. So a flop takes its reset in
/// one way, the kind the project chose, as an `if` that tests it alone;
/// elsewhere a reset is passed on unchanged, waited for, read by an
/// assertion, or tied off into a signal named unused. Which signals are
/// resets is known by name alone, from the rule's patterns.
pub(crate) fn reset_style(cx: &mut Cx) {
    let patterns = cx.patterns("patterns");
    let kinds = cx.reset_kinds("kinds");
    // The reads that are a flop's own reset test, which are its to make.
    let mut tests = FxHashSet::default();
    for block in cx.nodes::<ProceduralBlock>() {
        let Some(timing) = sequential(&block) else {
            continue;
        };
        let mut ifs = Vec::new();
        if let Some(item) = timing.item() {
            reset_ifs(item.syntax(), true, patterns, &mut ifs);
        }
        let Some((reset, falling)) =
            (timing.event_control()).and_then(|control| reset_event(&control, patterns))
        else {
            for test in ifs {
                tests.insert(test.name.syntax().clone());
                if !kinds.contains(&ResetKind::Sync) {
                    let text = spelled(&test.name);
                    let diagnostic = cx
                        .diagnostic(
                            Cx::range(test.name.syntax()),
                            format!("synchronous reset `{text}`"),
                        )
                        .pointing(format!("wait for its edge as well, `or negedge {text}`"));
                    cx.report(diagnostic);
                }
            }
            continue;
        };
        let text = spelled(&reset);
        if !kinds.contains(&ResetKind::Async) {
            let diagnostic = cx
                .diagnostic(
                    Cx::range(reset.syntax()),
                    format!("asynchronous reset `{text}`"),
                )
                .pointing("reset synchronously: wait for the clock alone");
            cx.report(diagnostic);
        }
        // A test of another reset is a read, and reported as one.
        let own: Vec<_> = (ifs.into_iter())
            .filter(|test| spelled(&test.name) == text)
            .collect();
        for test in &own {
            tests.insert(test.name.syntax().clone());
            let range = Cx::range(test.name.syntax());
            if test.low != falling {
                let (level, edge) = match falling {
                    true => ("high", "falling"),
                    false => ("low", "rising"),
                };
                let diagnostic = cx
                    .diagnostic(
                        range,
                        format!(
                            "`{text}` tested {level}, but the process resets on its {edge} edge"
                        ),
                    )
                    .pointing("test the level the edge leads to");
                cx.report(diagnostic);
            }
            if !test.alone {
                let diagnostic = cx
                    .diagnostic(
                        range,
                        format!("process resets on `{text}`, but not all of it"),
                    )
                    .pointing("give what this `if` leaves out a process of its own")
                    .note("a flop that waits for a reset it does not take holds its value badly");
                cx.report(diagnostic);
            }
        }
        // A process that reads its reset elsewhere is told so where it
        // reads it.
        if own.is_empty() && !read_in(timing.syntax(), &text) {
            let diagnostic = cx
                .diagnostic(
                    Cx::range(reset.syntax()),
                    format!("process waits for `{text}`, but does not test it"),
                )
                .pointing(format!(
                    "make `if ({}{text})` the whole of its body",
                    if falling { "!" } else { "" }
                ));
            cx.report(diagnostic);
        }
    }

    for name in cx.nodes::<NameRef>() {
        if !reset(&name, patterns) || tests.contains(name.syntax()) || passes(&name, patterns) {
            continue;
        }
        let text = spelled(&name);
        let in_flop = (name.syntax().ancestors())
            .filter_map(ProceduralBlock::cast)
            .any(|block| sequential(&block).is_some());
        let diagnostic = match in_flop {
            true => cx
                .diagnostic(
                    Cx::range(name.syntax()),
                    format!(
                        "reset `{text}` read in a flop's process, but not as an `if` on it alone"
                    ),
                )
                .pointing("test the reset in a branch of its own: `if (!rst) ... else if ...`"),
            false => cx
                .diagnostic(
                    Cx::range(name.syntax()),
                    format!("reset `{text}` read as logic"),
                )
                .pointing("read a reset only as a flop's `if` on it")
                .note("to clear state while running, use a synchronous clear"),
        };
        cx.report(diagnostic);
    }

    for (target, value) in writes(cx) {
        if value
            .as_ref()
            .is_none_or(|value| constant_or_copy(value, patterns))
        {
            continue;
        }
        let diagnostic = cx
            .diagnostic(
                target.text_range(),
                format!("reset `{}` driven by logic", target.text()),
            )
            .pointing("pass a reset on unchanged, or copy it")
            .note("to clear state while running, use a synchronous clear");
        cx.report(diagnostic);
    }
}

/// The timing statement around a flop's process: an `always_ff`'s, or an
/// `always`'s that waits for edges alone, and nowhere else.
fn sequential(block: &ProceduralBlock) -> Option<TimingStmt> {
    let keyword = block.keyword()?.kind();
    let timing = TimingStmt::cast(block.body()?.syntax().clone())?;
    match keyword {
        ALWAYS_FF_KW => Some(timing),
        ALWAYS_KW => {
            let control = timing.event_control()?;
            (edges(&control) && !waits_again(timing.syntax(), control.syntax())).then_some(timing)
        }
        _ => None,
    }
}

/// The reset `control` waits for an edge of, and whether that edge falls.
fn reset_event(control: &EventControl, patterns: &[Pattern]) -> Option<(NameRef, bool)> {
    let list = (control.syntax().first_child()).filter(|it| it.kind() == PAREN_EXPR)?;
    let mut falling = None;
    for element in list.children_with_tokens() {
        match element.kind() {
            POSEDGE_KW => falling = Some(false),
            NEGEDGE_KW => falling = Some(true),
            OR_KW | COMMA => falling = None,
            _ => {
                let name = element.into_node().and_then(NameRef::cast);
                if let (Some(name), Some(falling)) = (name, falling)
                    && reset(&name, patterns)
                {
                    return Some((name, falling));
                }
            }
        }
    }
    None
}

/// An `if` that tests a reset alone, where a flop may take one.
struct ResetIf {
    name: NameRef,
    /// Whether it tests the reset low.
    low: bool,
    /// Whether it is all of the process, with nothing beside it.
    alone: bool,
}

/// Each `if` under `node` that tests a reset alone and stands where a flop
/// takes its reset: reached through `begin` blocks, labels and loops over
/// an array, never inside another branch.
fn reset_ifs(node: &SyntaxNode, alone: bool, patterns: &[Pattern], ifs: &mut Vec<ResetIf>) {
    let mut visit = |item: Option<Item>, alone| {
        if let Some(item) = item {
            reset_ifs(item.syntax(), alone, patterns, ifs);
        }
    };
    if let Some(statement) = IfStmt::cast(node.clone()) {
        let test = (statement.condition()).and_then(|it| reset_test(&it, patterns));
        if let Some((name, low)) = test {
            ifs.push(ResetIf { name, low, alone });
        }
    } else if let Some(block) = Block::cast(node.clone()) {
        let alone = alone && block.items().nth(1).is_none();
        for item in block.items() {
            visit(Some(item), alone);
        }
    } else if let Some(labelled) = LabeledStmt::cast(node.clone()) {
        visit(labelled.item(), alone);
    } else if let Some(each) = ForStmt::cast(node.clone()) {
        visit(each.body(), alone);
    } else if let Some(each) = ForeachStmt::cast(node.clone()) {
        visit(each.body(), alone);
    }
}

/// The reset `condition` tests, alone, and whether it tests it low.
fn reset_test(condition: &ParenExpr, patterns: &[Pattern]) -> Option<(NameRef, bool)> {
    let mut exprs = condition.exprs();
    match (exprs.next(), exprs.next()) {
        (Some(only), None) => test(&only, patterns),
        _ => None,
    }
}

fn test(expr: &Expr, patterns: &[Pattern]) -> Option<(NameRef, bool)> {
    match expr {
        Expr::NameRef(name) if reset(name, patterns) => Some((name.clone(), false)),
        Expr::ParenExpr(inner) => reset_test(inner, patterns),
        Expr::UnaryExpr(unary)
            if unary
                .op()
                .is_some_and(|op| matches!(op.kind(), BANG | TILDE)) =>
        {
            test(&unary.expr()?, patterns).map(|(name, low)| (name, !low))
        }
        Expr::BinExpr(comparison) => {
            let (other, _, truth) = one_bit_comparison_of(comparison)?;
            test(&other, patterns).map(|(name, low)| (name, low == truth))
        }
        _ => None,
    }
}

/// Whether `node` reads `reset` other than in an event control.
fn read_in(node: &SyntaxNode, reset: &str) -> bool {
    (node.descendants())
        .filter_map(NameRef::cast)
        .filter(|name| !(name.syntax().ancestors()).any(|it| it.kind() == EVENT_CONTROL))
        .any(|name| spelled(&name) == reset)
}

/// How `name` is spelled.
fn spelled(name: &NameRef) -> String {
    name.name()
        .map(|it| it.text().to_string())
        .unwrap_or_default()
}

/// Whether `name` names a reset.
fn reset(name: &NameRef, patterns: &[Pattern]) -> bool {
    name.name().is_some_and(|it| named(&it, patterns))
}

/// Whether `token` is a name one of `patterns` takes for a reset's.
fn named(token: &SyntaxToken, patterns: &[Pattern]) -> bool {
    patterns
        .iter()
        .any(|pattern| pattern.stem(token.text()).is_some())
}

/// Whether a read of the reset `name` is one a reset may have: in an event
/// control, an assertion or a property; passed whole to an instance's port;
/// in what is written to a reset, which `writes` judges; or tied off.
fn passes(name: &NameRef, patterns: &[Pattern]) -> bool {
    let allowed = |kind| {
        matches!(
            kind,
            EVENT_CONTROL
                | CONCURRENT_ASSERTION
                | IMMEDIATE_ASSERTION
                | PROPERTY_DECL
                | SEQUENCE_DECL
                | DEFAULT_DISABLE
                | COVERGROUP_DECL
        )
    };
    if name.syntax().ancestors().any(|it| allowed(it.kind())) || written(name.syntax()) {
        return true;
    }
    let whole = copied(name.syntax());
    let Some(parent) = whole.parent() else {
        return false;
    };
    match parent.kind() {
        ARG => (parent.parent())
            .filter(|it| it.kind() == ARG_LIST)
            .and_then(|it| it.parent())
            .is_some_and(|it| it.kind() == INSTANCE),
        _ => (name.syntax().ancestors())
            .find(|node| matches!(node.kind(), ASSIGNMENT | DECLARATOR))
            .and_then(|node| match node.kind() {
                ASSIGNMENT => Assignment::cast(node)?.lhs().and_then(|it| target(&it)),
                _ => Declarator::cast(node)?.name(),
            })
            // A signal named unused ties off what a design does not read,
            // as `unused-signal` takes it.
            .is_some_and(|it| named(&it, patterns) || it.text().to_lowercase().contains("unused")),
    }
}

/// The largest expression around `node` that copies it: a select of it, a
/// member, parentheses, a concatenation of it alone, a replication.
fn copied(node: &SyntaxNode) -> SyntaxNode {
    let mut node = node.clone();
    while let Some(parent) = node.parent() {
        let copies = match parent.kind() {
            PAREN_EXPR | REPLICATION_EXPR => true,
            CONCAT_EXPR => parent.children().nth(1).is_none(),
            INDEX_EXPR | FIELD_EXPR => parent.first_child().as_ref() == Some(&node),
            _ => false,
        };
        if !copies {
            break;
        }
        node = parent;
    }
    node
}

/// Whether `node` is what an assignment writes, or a part of it.
fn written(node: &SyntaxNode) -> bool {
    let mut node = node.clone();
    while let Some(parent) = node.parent() {
        match parent.kind() {
            INDEX_EXPR | FIELD_EXPR if parent.first_child().as_ref() == Some(&node) => {}
            CONCAT_EXPR => {}
            ASSIGNMENT => {
                return Assignment::cast(parent)
                    .and_then(|it| it.lhs())
                    .is_some_and(|lhs| *lhs.syntax() == node);
            }
            _ => return false,
        }
        node = parent;
    }
    false
}

/// The variable `lhs` writes, if it writes one whole: a name, not a part.
fn target(lhs: &Expr) -> Option<SyntaxToken> {
    match lhs {
        Expr::NameRef(name) => name.name(),
        _ => None,
    }
}

/// Each write of a reset in the file, a name and the value it is given,
/// where it is declared or assigned.
fn writes(cx: &Cx) -> Vec<(SyntaxToken, Option<Expr>)> {
    let patterns = cx.patterns("patterns");
    let assigned = cx.nodes::<Assignment>().filter_map(move |assignment| {
        let name = target(&assignment.lhs()?).filter(|it| named(it, patterns))?;
        Some((name, assignment.rhs()))
    });
    let declared = cx.nodes::<Declarator>().filter_map(move |declarator| {
        let name = declarator.name().filter(|it| named(it, patterns))?;
        let value = match declarator.init()? {
            TypeOrExpr::Expr(value) => Some(value),
            TypeOrExpr::DataType(_) => None,
        };
        Some((name, value))
    });
    assigned.chain(declared).collect()
}

/// Whether `value` is a constant, or a reset passed on: what a reset may be
/// given.
fn constant_or_copy(value: &Expr, patterns: &[Pattern]) -> bool {
    match value {
        Expr::LiteralExpr(_) => true,
        Expr::NameRef(name) => reset(name, patterns) || name.preproc().is_some(),
        Expr::ParenExpr(inner) => {
            let mut exprs = inner.exprs();
            match (exprs.next(), exprs.next()) {
                (Some(only), None) => constant_or_copy(&only, patterns),
                _ => false,
            }
        }
        Expr::IndexExpr(select) => {
            (select.base()).is_some_and(|it| constant_or_copy(&it, patterns))
        }
        Expr::FieldExpr(field) => (field.expr()).is_some_and(|it| constant_or_copy(&it, patterns)),
        Expr::ConcatExpr(concat) => {
            let mut exprs = concat.exprs();
            match (exprs.next(), exprs.next()) {
                (Some(only), None) => constant_or_copy(&only, patterns),
                _ => false,
            }
        }
        Expr::ReplicationExpr(replication) => (replication.concat())
            .is_some_and(|it| constant_or_copy(&Expr::ConcatExpr(it), patterns)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::Config;
    use crate::rules::found;

    /// Resets named two ways, taken synchronously, as a project whose
    /// domains each bring their own reset might set them.
    #[test]
    fn a_project_names_its_resets_and_their_kind() {
        let source =
            "module top (input logic i_clk, i_reset_n, i_noc_clk, i_noc_reset_n, rst_ni, d,
                                  output logic q);
  always_ff @(posedge i_clk) if (!i_reset_n) q <= 1'b0; else q <= d;
  always_ff @(posedge i_noc_clk) if (!i_noc_reset_n) q <= 1'b0; else q <= d;
  always_ff @(posedge i_clk or negedge rst_ni) if (!rst_ni) q <= 1'b0; else q <= d;
  assign q = d & i_noc_reset_n;
endmodule
";
        let options: &[(&str, &[&str])] =
            &[("patterns", &["*reset_n", "rst*ni"]), ("kinds", &["sync"])];
        assert_eq!(
            found("reset-style", options, source),
            [
                "asynchronous reset `rst_ni`",
                "reset `i_noc_reset_n` read as logic",
            ]
        );
    }

    /// A synchronous reset leaves flops it does not take beside its branch,
    /// and may reset an array one element at a time.
    #[test]
    fn a_synchronous_reset_shares_its_process() {
        let source =
            "module top (input logic i_clk, i_reset_n, d, output logic q, r, output logic [3:0] a);
  always_ff @(posedge i_clk) begin
    r <= d;
    if (!i_reset_n) q <= 1'b0;
    else q <= d;
  end
  always_ff @(posedge i_clk)
    for (int i = 0; i < 4; i++) if (!i_reset_n) a[i] <= 1'b0; else a[i] <= d;
  always_ff @(posedge i_clk) if (d) if (!i_reset_n) q <= 1'b0;
endmodule
";
        let options: &[(&str, &[&str])] = &[("patterns", &["*reset_n"]), ("kinds", &["sync"])];
        assert_eq!(
            found("reset-style", options, source),
            ["reset `i_reset_n` read in a flop's process, but not as an `if` on it alone"]
        );
    }

    #[test]
    fn a_kind_of_reset_is_sync_or_async() {
        let mut config = Config::default();
        let err = config.set_option("reset-style", "kinds", &["synchronous"]);
        assert!(
            err.unwrap_err()
                .to_string()
                .starts_with("`kinds`: `synchronous` is not a kind of reset")
        );
    }
}
