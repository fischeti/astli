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
use astli_syntax::SyntaxKind::*;

/// Condition under which a branch is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taken {
    /// `` `ifdef NAME `` or `` `elsif NAME ``: taken when `NAME` is defined.
    Defined(TokenId),
    /// `` `ifndef NAME ``: taken when `NAME` is undefined.
    Undefined(TokenId),
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
        (_, Operands::Name(name)) => Taken::Defined(*name),
        _ => Taken::Never,
    }
}
