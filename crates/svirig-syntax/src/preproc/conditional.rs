//! `` `ifdef `` … `` `endif ``: the flat directive stream nested into regions.
//!
//! A conditional is not one directive. The five that build it --
//! `` `ifdef ``, `` `ifndef ``, `` `elsif ``, `` `else ``, `` `endif `` --
//! only mean anything together, and what they delimit is *text*: which branch
//! survives decides what the rest of the preprocessor even reads.
//!
//! # One shape, two readings
//!
//! The expanded mode evaluates a region against the macro table and expands
//! the one branch that is taken. Raw mode keeps every branch, because the
//! formatter has to lay out code it cannot evaluate -- it does not know what a
//! build system will define -- and so needs the region as structure rather
//! than as a choice already made.
//!
//! Both want the same thing from here: where each branch starts, where it
//! ends, and what it tests. What they do with it is where they part.
//!
//! # A region does not cross a file boundary
//!
//! A region is read within the one stretch of text it opens in, so an
//! `` `ifdef `` in a file and an `` `endif `` in a file it includes do not
//! pair. That is a deliberate reading rather than an omission: the include
//! that would join them sits *inside* the region, so whether it is even
//! followed is the question the region was supposed to answer. An unclosed
//! region runs to the end of its own text, and an `` `endif `` with nothing
//! above it is consumed like any other directive. The corpus has zero of
//! either.
//!
//! # Directives are stepped over whole
//!
//! The scan walks directives rather than tokens, so a `` `define `` body goes
//! past in one step. A body is substitution text: the `` `endif `` in
//! `` `define GUARD(x) `ifdef E x `endif `` closes the region the *body*
//! opens, wherever the macro is used, and closes nothing here.

use std::ops::Range;

use super::directive::{self, Directive, DirectiveType, Operands};
use super::tokens::{Input, TokenId, TokenSpan};
use crate::SyntaxKind::*;

/// When a branch is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taken {
    /// `` `ifdef NAME `` or `` `elsif NAME ``: when the name is defined.
    Defined(TokenId),
    /// `` `ifndef NAME ``: when it is not.
    Undefined(TokenId),
    /// `` `else ``: when nothing above it was.
    Otherwise,
    /// A conditional with no name at all, which 22.6 makes an error.
    ///
    /// Never taken. The name is the whole of the condition, so with none there
    /// is nothing to be true -- and a region that has an `` `else `` then
    /// falls to it, which is the branch that *is* well formed.
    Never,
}

/// One branch of a conditional region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Branch {
    pub taken: Taken,
    /// The directive that opens the branch, its operand included.
    pub directive: TokenSpan,
    /// The text this branch guards: everything from the end of its own
    /// directive to the start of the next one at this level.
    ///
    /// Untrimmed, unlike a macro body or a directive's operands. A branch is
    /// ordinary source text rather than something a directive consumes, so its
    /// comments and its blank lines are its own and a formatter has to be able
    /// to reach them.
    pub body: TokenSpan,
}

/// One `` `ifdef `` … `` `endif ``, and the branches between.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    /// The introducer through the `` `endif ``, or through the end of the text
    /// when there is not one.
    pub tokens: TokenSpan,
    /// In source order, starting with the `` `ifdef `` or `` `ifndef ``. Never
    /// empty.
    pub branches: Vec<Branch>,
    /// Whether an `` `endif `` closed it.
    pub closed: bool,
}

impl Region {
    /// Whether the region writes out the "none of the above" branch.
    ///
    /// Without an `` `else `` that branch is the empty text, and it is a real
    /// branch: a region that opens a `begin` in every branch it *writes* still
    /// disagrees with taking neither.
    pub fn has_else(&self) -> bool {
        self.branches
            .last()
            .is_some_and(|branch| branch.taken == Taken::Otherwise)
    }
}

/// Reads the conditional region introduced at `at`, taking no token from
/// `limit` onwards.
///
/// The caller has established that the token at `at` opens one.
pub fn region(input: &Input, at: u32, limit: u32) -> Region {
    use DirectiveType::*;

    let mut open = parse(input, at).expect("a region is introduced by a directive");
    let mut from = open.tokens.end;
    let mut branches = Vec::new();
    // How many regions opened inside this one and have not closed. Their
    // branch directives are theirs, not ours.
    let mut nested = 0u32;
    let mut cursor = from;
    let mut end = limit;
    let mut closed = false;

    while cursor < limit {
        let Some(found) = parse(input, cursor) else {
            cursor += 1;
            continue;
        };
        // `max` is insurance against a directive that covered no tokens; `min`
        // keeps a directive whose operands ran past the text inside it.
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

/// Every conditional region directly inside `span`, in source order.
///
/// Only the outermost: a branch's own regions are found by asking again about
/// that branch's body, which is what lets one function answer for a file and
/// for a branch alike.
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

/// The directive introduced at `at`, if there is one there.
///
/// A `` `name `` that names a macro rather than a directive comes back `None`;
/// its arguments are not delimited here, because no argument list contains a
/// conditional directive and delimiting one would want the macro table.
fn parse(input: &Input, at: u32) -> Option<Directive> {
    (input.kind(at) == DIRECTIVE)
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
