//! Conditional compilation regions (`` `ifdef `` … `` `endif ``).
//!
//! Conditional compilation constructs in SystemVerilog consist of opening directives
//! (`` `ifdef `` / `` `ifndef ``), optional alternative branches (`` `elsif `` / `` `else ``),
//! and a terminating `` `endif `` directive.
//!
//! This module parses the branch boundaries and guard conditions so that:
//! - Expanded mode can evaluate conditions against the active macro table and select the active branch.
//! - Raw/formatting mode can preserve the structural hierarchy across all branches.

use std::ops::Range;

use super::directive::{self, Directive, DirectiveType, Operands};
use super::tokens::{Input, TokenId, TokenSpan};
use astli_syntax::SyntaxKind::{self, *};

/// Condition under which a branch is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taken {
    /// `` `ifdef NAME `` or `` `elsif NAME ``: taken when `NAME` is defined.
    Defined(TokenId),
    /// `` `ifndef NAME ``: taken when `NAME` is undefined.
    Undefined(TokenId),
    /// `` `ifdef (EXPR) `` or `` `elsif (EXPR) ``: taken when the expression
    /// of names holds, each name true when defined. `` `ifndef (EXPR) `` is
    /// taken when it does not, which `negated` says. The span is the
    /// parentheses and what is between them.
    Expression { tokens: TokenSpan, negated: bool },
    /// `` `else ``: default fallback branch taken when preceding branches evaluate to false.
    Otherwise,
    /// Branch with missing condition name (IEEE 1800-2023 §22.6 syntax error; never taken).
    Never,
}

/// A single branch within a conditional compilation region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Branch {
    /// The selection condition for this branch.
    pub taken: Taken,
    /// Token span of the directive opening this branch, including operands.
    pub directive: TokenSpan,
    /// Guarded body token span between this branch directive and the next branch or `` `endif ``.
    pub body: TokenSpan,
}

/// A complete conditional compilation block (`` `ifdef `` / `` `ifndef `` … `` `endif ``).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    /// Token span encompassing the entire region, from opening directive through closing `` `endif ``.
    pub tokens: TokenSpan,
    /// Ordered list of branches within this region.
    pub branches: Vec<Branch>,
    /// Whether this region was properly closed with an `` `endif `` directive.
    pub closed: bool,
}

impl Region {
    /// Returns `true` if the region contains an explicit `` `else `` branch.
    pub fn has_else(&self) -> bool {
        self.branches
            .last()
            .is_some_and(|branch| branch.taken == Taken::Otherwise)
    }
}

/// Parses the conditional region opening at token index `at`, up to `limit`.
pub fn region(input: &Input, at: u32, limit: u32) -> Region {
    use DirectiveType::*;

    let mut open = parse(input, at).expect("a region is introduced by a directive");
    let mut from = open.tokens.end;
    let mut branches = Vec::new();
    let mut nested = 0u32;
    let mut cursor = from;
    let mut end = limit;
    let mut closed = false;

    while cursor < limit {
        let Some(found) = parse(input, cursor) else {
            cursor += 1;
            continue;
        };
        let after = found.tokens.end.max(cursor + 1).min(limit);

        match found.ty {
            Ifdef | Ifndef => nested += 1,
            Endif if nested > 0 => nested -= 1,
            Elsif | Else if nested == 0 => {
                branches.push(branch(input, &open, from..cursor));
                open = found;
                from = after;
            }
            Endif => {
                branches.push(branch(input, &open, from..cursor));
                end = after;
                closed = true;
                break;
            }
            _ => {}
        }
        cursor = after;
    }

    if !closed {
        branches.push(branch(input, &open, from..limit));
    }

    Region {
        tokens: input.span(at..end),
        branches,
        closed,
    }
}

/// Discovers all outermost conditional regions directly contained within `span`.
pub fn regions(input: &Input, span: TokenSpan) -> Vec<Region> {
    use DirectiveType::*;

    let mut out = Vec::new();
    let mut cursor = span.start;

    while cursor < span.end {
        let Some(found) = parse(input, cursor) else {
            cursor += 1;
            continue;
        };
        cursor = match found.ty {
            Ifdef | Ifndef => {
                let found = region(input, cursor, span.end);
                let end = found.tokens.end;
                out.push(found);
                end
            }
            _ => found.tokens.end,
        }
        .max(cursor + 1);
    }
    out
}

/// Every conditional region within `span`, nested ones included, in the
/// order they open.
///
/// The same regions as [`regions`] and then [`regions`] again on each branch,
/// found in one pass: that is quadratic in how deep they nest, which is no
/// bound at all.
pub fn all_regions(input: &Input, span: TokenSpan) -> Vec<Region> {
    use DirectiveType::*;

    /// A region still open: where it is in `out`, and its current branch.
    struct Open {
        index: usize,
        directive: Directive,
        from: u32,
    }

    let mut out: Vec<Region> = Vec::new();
    let mut stack: Vec<Open> = Vec::new();
    let mut cursor = span.start;

    while cursor < span.end {
        let Some(found) = parse(input, cursor) else {
            cursor += 1;
            continue;
        };
        let after = found.tokens.end.max(cursor + 1).min(span.end);

        match (found.ty, stack.last_mut()) {
            (Ifdef | Ifndef, _) => {
                stack.push(Open {
                    index: out.len(),
                    directive: found,
                    from: after,
                });
                out.push(Region {
                    tokens: input.span(cursor..span.end),
                    branches: Vec::new(),
                    closed: false,
                });
            }
            (Elsif | Else, Some(open)) => {
                let branch = branch(input, &open.directive, open.from..cursor);
                out[open.index].branches.push(branch);
                open.directive = found;
                open.from = after;
            }
            (Endif, Some(open)) => {
                let branch = branch(input, &open.directive, open.from..cursor);
                let region = &mut out[open.index];
                region.branches.push(branch);
                region.tokens = input.span(region.tokens.start..after);
                region.closed = true;
                stack.pop();
            }
            _ => {}
        }
        cursor = after;
    }

    // What never closed runs to the end, as does its last branch.
    for open in stack {
        let branch = branch(input, &open.directive, open.from..span.end);
        out[open.index].branches.push(branch);
    }
    out
}

/// Parses the directive at `at`, if one exists at that token.
fn parse(input: &Input, at: u32) -> Option<Directive> {
    (input.kind(at) == TICK_IDENT)
        .then(|| DirectiveType::lookup(input.text(at)))
        .flatten()
        .map(|ty| directive::parse(ty, input, at))
}

fn branch(input: &Input, open: &Directive, body: Range<u32>) -> Branch {
    Branch {
        taken: taken(open),
        directive: open.tokens,
        body: input.span(body),
    }
}

fn taken(directive: &Directive) -> Taken {
    match (directive.ty, &directive.operands) {
        (DirectiveType::Else, _) => Taken::Otherwise,
        (DirectiveType::Ifndef, Operands::Name(name)) => Taken::Undefined(*name),
        (ty, Operands::Expression(tokens)) => Taken::Expression {
            tokens: *tokens,
            negated: ty == DirectiveType::Ifndef,
        },
        (_, Operands::Name(name)) => Taken::Defined(*name),
        _ => Taken::Never,
    }
}

/// Evaluates the expression of names in `tokens`, the parentheses included,
/// with `defined` saying which names are. `Err` says why it does not read.
///
/// `!` binds tightest, then `&&`, then `||`, and `->` and `<->` loosest,
/// grouping to the right, as the logical operators do in an expression.
pub(crate) fn evaluate(
    input: &Input,
    tokens: TokenSpan,
    defined: impl Fn(&str) -> bool,
) -> Result<bool, &'static str> {
    let words: Vec<u32> = (tokens.range())
        .filter(|&at| !input.kind(at).is_trivia() && input.kind(at) != LINE_CONTINUATION)
        .collect();
    let mut reader = Reader {
        input,
        words: &words,
        next: 0,
        defined: &defined,
    };
    let value = reader.primary()?;
    match reader.peek() {
        None => Ok(value),
        Some(_) => Err("more follows the closing parenthesis"),
    }
}

struct Reader<'a, 'i> {
    input: &'a Input<'i>,
    words: &'a [u32],
    next: usize,
    defined: &'a dyn Fn(&str) -> bool,
}

impl Reader<'_, '_> {
    fn peek(&self) -> Option<SyntaxKind> {
        (self.words.get(self.next)).map(|&at| self.input.kind(at))
    }

    fn bump(&mut self) -> Option<u32> {
        let at = self.words.get(self.next).copied();
        self.next += 1;
        at
    }

    fn implication(&mut self) -> Result<bool, &'static str> {
        let left = self.or()?;
        match self.peek() {
            Some(MINUS_GT) => {
                self.bump();
                let right = self.implication()?;
                Ok(!left || right)
            }
            Some(LT_MINUS_GT) => {
                self.bump();
                let right = self.implication()?;
                Ok(left == right)
            }
            _ => Ok(left),
        }
    }

    fn or(&mut self) -> Result<bool, &'static str> {
        let mut value = self.and()?;
        while self.peek() == Some(PIPE_PIPE) {
            self.bump();
            value |= self.and()?;
        }
        Ok(value)
    }

    fn and(&mut self) -> Result<bool, &'static str> {
        let mut value = self.unary()?;
        while self.peek() == Some(AMP_AMP) {
            self.bump();
            value &= self.unary()?;
        }
        Ok(value)
    }

    fn unary(&mut self) -> Result<bool, &'static str> {
        match self.peek() {
            Some(BANG) => {
                self.bump();
                Ok(!self.unary()?)
            }
            Some(IDENT | ESCAPED_IDENT) => {
                let at = self.bump().expect("peeked");
                Ok((self.defined)(self.input.text(at)))
            }
            Some(L_PAREN) => self.primary(),
            Some(_) => Err("expected a name, `!` or `(`"),
            None => Err("the expression ends early"),
        }
    }

    /// A parenthesised expression.
    fn primary(&mut self) -> Result<bool, &'static str> {
        if self.bump().is_none_or(|at| self.input.kind(at) != L_PAREN) {
            return Err("expected `(`");
        }
        let value = self.implication()?;
        match self.bump() {
            Some(at) if self.input.kind(at) == R_PAREN => Ok(value),
            Some(_) => Err("expected an operator or `)`"),
            None => Err("no closing parenthesis"),
        }
    }
}
