//! Whether a pass of fixes kept every expression grouped as it was.

use astli_syntax::SyntaxKind::{L_PAREN, R_PAREN};
use astli_syntax::ast::{AstNode, BinExpr, Expr};
use astli_syntax::{SyntaxNode, significant_range};
use astli_text::Edit;
use rowan::{TextRange, TextSize};

/// The range in `before` of the first binary expression that `edits`, which
/// turned `before` into `after`, grouped otherwise; none if each operator
/// they left alone has the same operands in `after`, parentheses aside.
///
/// A fix keeps what the code means, so one that adds parentheses has to
/// add them where the parser already grouped: around one operand whole.
/// Placed anywhere else, they regroup the expression and the text still
/// parses, so reparsing alone does not catch it. An operator an edit
/// touches is the edit's to change, and not compared; nor is an operand
/// with an end an edit replaces. What an edit inserts at an operand's end
/// may fall on either side of it: a `(` where it starts may open it or one
/// of its own operands.
pub fn regrouped(before: &SyntaxNode, after: &SyntaxNode, edits: &[&Edit]) -> Option<TextRange> {
    let shift = Shift::new(edits);
    for expr in before.descendants().filter_map(BinExpr::cast) {
        let Some(op) = expr.op() else {
            continue;
        };
        let range = significant_range(expr.syntax()).unwrap_or(expr.syntax().text_range());
        if shift.touches(op.text_range()) {
            continue;
        }
        let Some(at) = shift.past(op.text_range().start()) else {
            continue;
        };
        let moved = (after.token_at_offset(at).right_biased())
            .filter(|it| it.kind() == op.kind() && it.text() == op.text())
            .and_then(|it| it.parent())
            .and_then(BinExpr::cast)
            .filter(|it| it.op().is_some_and(|it| it.text_range().start() == at));
        let Some(moved) = moved else {
            return Some(range);
        };
        for (old, new) in [(expr.lhs(), moved.lhs()), (expr.rhs(), moved.rhs())] {
            let (old, new) = match (old, new) {
                (Some(old), Some(new)) => (unwrapped(old), unwrapped(new)),
                (None, None) => continue,
                _ => return Some(range),
            };
            let Some(old) = significant_range(&old) else {
                continue;
            };
            let (Some(start), Some(end)) = (shift.span(old.start()), shift.span(old.end())) else {
                continue;
            };
            let Some(new) = significant_range(&new) else {
                return Some(range);
            };
            if !start.contains_inclusive(new.start()) || !end.contains_inclusive(new.end()) {
                return Some(range);
            }
        }
    }
    None
}

/// `expr` without the parentheses around it, however many.
fn unwrapped(mut expr: Expr) -> SyntaxNode {
    while let Expr::ParenExpr(paren) = &expr {
        let mut nodes = paren.syntax().children();
        let only = |kind| matches!(kind, L_PAREN | R_PAREN);
        let tokens = (paren.syntax().children_with_tokens())
            .filter_map(|it| it.into_token())
            .all(|it| it.kind().is_trivia() || only(it.kind()));
        match (nodes.next().and_then(Expr::cast), nodes.next()) {
            (Some(inner), None) if tokens => expr = inner,
            _ => break,
        }
    }
    expr.syntax().clone()
}

/// Where an offset of the text before the edits is after them.
struct Shift {
    /// Each edit's start and end before, and how much longer it leaves the
    /// text, by start.
    edits: Vec<(u32, u32, i64)>,
}

impl Shift {
    fn new(edits: &[&Edit]) -> Shift {
        let mut edits: Vec<_> = (edits.iter())
            .map(|it| {
                let grown = it.text.len() as i64 - i64::from(it.at.end - it.at.start);
                (it.at.start, it.at.end, grown)
            })
            .collect();
        edits.sort_unstable();
        Shift { edits }
    }

    /// Whether an edit changes or inserts into `range`; one that inserts at
    /// either end leaves it whole.
    fn touches(&self, range: TextRange) -> bool {
        let (from, to) = (u32::from(range.start()), u32::from(range.end()));
        (self.edits.iter()).any(|&(start, end, _)| match start == end {
            true => from < start && start < to,
            false => start < to && from < end,
        })
    }

    /// Where `offset` is after the edits, past what an edit inserts there,
    /// as the token that starts there is. None if an edit replaces it.
    fn past(&self, offset: TextSize) -> Option<TextSize> {
        let offset = u32::from(offset);
        self.moved(offset, |_, end| end <= offset)
    }

    /// Where `offset` is after the edits, from ahead of what an edit inserts
    /// there to past it. None if an edit replaces it.
    fn span(&self, offset: TextSize) -> Option<TextRange> {
        let at = u32::from(offset);
        let ahead = self.moved(at, |start, end| end < at || (end == at && start < end))?;
        Some(TextRange::new(ahead, self.past(offset)?))
    }

    fn moved(&self, offset: u32, before: impl Fn(u32, u32) -> bool) -> Option<TextSize> {
        let mut grown = 0;
        for &(start, end, by) in &self.edits {
            if start < offset && offset < end {
                return None;
            }
            if before(start, end) {
                grown += by;
            }
        }
        u32::try_from(i64::from(offset) + grown)
            .ok()
            .map(TextSize::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use astli_parse::SyntaxTree;
    use astli_text::apply;

    /// Whether inserting each of `parens`, an offset and the text there,
    /// into `assign y = EXPR;` regroups it.
    fn regroups(expr: &str, parens: &[(u32, &str)]) -> bool {
        let text = format!("module m; assign y = {expr}; endmodule\n");
        let at = text.find(expr).unwrap() as u32;
        let before = SyntaxTree::parse("before.sv", text);
        let edits: Vec<Edit> = (parens.iter())
            .map(|&(offset, text)| Edit {
                at: before.span(TextRange::empty((at + offset).into())),
                text: text.to_string(),
            })
            .collect();
        let fixes: Vec<&[Edit]> = edits.iter().map(std::slice::from_ref).collect();
        let (text, _) = apply(before.source(), &fixes);
        let after = SyntaxTree::parse("after.sv", text);
        assert!(after.diagnostics().is_empty());
        let edits: Vec<&Edit> = edits.iter().collect();
        regrouped(before.root(), after.root(), &edits).is_some()
    }

    #[test]
    fn parentheses_the_parser_already_put_keep_the_grouping() {
        // a || (b && c)
        assert!(!regroups("a || b && c", &[(5, "("), (11, ")")]));
        // (a == b) & c, already in parentheses once
        assert!(!regroups("(a == b) & c", &[(0, "("), (8, ")")]));
        // a || ((b && c))
        assert!(!regroups("a || (b && c)", &[(5, "("), (13, ")")]));
        // (a < b) && (c > d) || e: the `(` at the `||`'s left operand opens
        // an operand of that operand
        assert!(!regroups(
            "a < b && c > d || e",
            &[(0, "("), (5, ")"), (9, "("), (14, ")")]
        ));
    }

    #[test]
    fn parentheses_elsewhere_regroup() {
        // (a || b) && c
        assert!(regroups("a || b && c", &[(0, "("), (6, ")")]));
        // a & (b == c) is what it was; (a & b) == c is not
        assert!(regroups("a & b == c", &[(0, "("), (5, ")")]));
        // !(a && b)
        assert!(regroups("!a && b", &[(1, "("), (7, ")")]));
    }
}
