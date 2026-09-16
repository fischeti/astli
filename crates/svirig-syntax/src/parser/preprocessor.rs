//! The grammar's side of the preprocessor: what a `` ` `` builds in the tree.
//!
//! [`crate::preproc`] finds these and works out their extents; this is where
//! they become nodes. The split is the crate boundary `svirig-preproc` will
//! one day be -- nothing in the preprocessor may reach into the parser, and a
//! rule here asks the [token source](super::source) for a shape rather than
//! scanning for itself.
//!
//! # A `` `name `` is an atom, wherever it stands
//!
//! Macro calls appear in every syntactic position SystemVerilog has -- item,
//! member, statement, expression, port element, type -- and verification code
//! is written almost entirely out of them. So these rules are reachable from
//! the top level *and* from inside a [`VERBATIM`]
//! run, and the preprocessor's structure lands in the tree whether or not the
//! grammar can shape what surrounds it.
//!
//! Reading a region as one atom is also what keeps a *ragged* one -- one that
//! hands a delimiter across a branch boundary -- from corrupting the
//! fallback's delimiter stack. The run never looks inside, so nothing
//! unbalanced reaches it, and what a ragged region costs is the one construct
//! that encloses it rather than the rest of the file. See
//! `docs/preprocessor.md`.
//!
//! # What is *not* parsed
//!
//! A macro argument is **balanced token soup, never an expression**, because
//! an argument is text: it may be half a declaration, an operator, or a type.
//! A `` `define `` body is text for the same reason and more strongly, since
//! whitespace in it is observable through `` `" ``.
//!
//! # A branch is parsed only when every branch balances
//!
//! A **self-delimiting** region is one whose branches each open and close
//! whatever they open, and its branches are ordinary text: each is parsed in
//! the enclosing context, so an `` `ifdef `` between two statements holds
//! statements and one between two module items holds items.
//!
//! A **ragged** region hands a delimiter across a branch boundary --
//! `` `ifdef SYNTHESIS `` opening an `always_comb begin` that the `` `else ``
//! closes -- and no branch of one is a construct at all. There, nothing runs
//! but the preprocessor's own rules, declarations, and the fallback: a
//! declaration is self-contained, and the fallback is bounded, so neither can
//! go looking for the `end` that the *next* branch writes.
//!
//! Which a region is, is [decided when the stream is
//! built](super::source::RegionShape::live), because it is a question about
//! raw tokens rather than about anything a rule can see.

use super::Parser;
use super::decl::declaration;
use super::source::{DirectiveShape, Position, RegionShape, Tokens};
use super::verbatim::{Context, verbatim};
use crate::SyntaxKind::*;
use crate::preproc::DirectiveType;

/// Parses whatever the preprocessor has at the cursor.
///
/// `false`, having consumed nothing, when it has nothing there -- so a caller
/// can try this first and fall back. When it returns `true` it has always
/// taken at least one token.
pub fn any<T: Tokens>(parser: &mut Parser<T>) -> bool {
    if let Some(len) = parser.macro_call() {
        macro_call(parser, len);
        return true;
    }

    let Some(shape) = parser.directive() else {
        return false;
    };

    // A conditional is not one directive: the five that build it only mean
    // anything together, and what they delimit is text. So the introducer is
    // asked about twice, and the region -- which contains this directive --
    // is what wins.
    if matches!(shape.ty, DirectiveType::Ifdef | DirectiveType::Ifndef)
        && let Some(shape) = parser.region()
    {
        region(parser, shape);
        return true;
    }

    directive(parser, shape);
    true
}

/// A macro reference and, where the macro takes one, its argument list.
fn macro_call<T: Tokens>(parser: &mut Parser<T>, len: u32) {
    let marker = parser.start();
    let end = parser.ahead(len);

    parser.bump();
    if parser.position() < end && parser.at(L_PAREN) {
        arguments(parser, end);
    }

    parser.complete(marker, MACRO_CALL);
}

/// The arguments, split on commas at depth zero and on nothing else.
///
/// The shape has already decided where the list ends; what is left is finding
/// the commas that separate one argument from the next, which is all a macro
/// argument's structure ever is.
fn arguments<T: Tokens>(parser: &mut Parser<T>, end: Position) {
    let list = parser.start();
    parser.bump();

    let mut depth = 0u32;
    let mut argument = parser.start();

    while parser.position() < end {
        match parser.kind(0) {
            L_PAREN | L_BRACK | L_BRACE | APOSTROPHE_L_BRACE => depth += 1,
            R_PAREN if depth == 0 => break,
            R_PAREN | R_BRACK | R_BRACE => depth = depth.saturating_sub(1),
            // The separator belongs to the list rather than to either
            // argument beside it.
            COMMA if depth == 0 => {
                parser.complete(argument, MACRO_ARG);
                parser.bump();
                argument = parser.start();
                continue;
            }
            _ => {}
        }
        parser.bump();
    }

    // Completed even when it is empty: `` `A(,) `` passes two arguments and
    // `` `A() `` passes one, so the children have to count the commas plus
    // one or nothing can compare them against an arity.
    parser.complete(argument, MACRO_ARG);

    if parser.position() < end {
        parser.bump();
    }
    parser.complete(list, MACRO_ARG_LIST);
}

/// A directive and its operands.
///
/// Operands are ordinary tokens, except a `` `define ``'s body, which gets a
/// node of its own so that the formatter has something to refuse to touch.
fn directive<T: Tokens>(parser: &mut Parser<T>, shape: DirectiveShape) {
    let marker = parser.start();
    let body = shape.body.unwrap_or(shape.len..shape.len);

    take(parser, body.start);
    if body.start < body.end {
        let text = parser.start();
        take(parser, body.end - body.start);
        parser.complete(text, MACRO_BODY);
    }
    take(parser, shape.len.saturating_sub(body.end));

    parser.complete(marker, DIRECTIVE);
}

/// A conditional region, with every branch it writes.
fn region<T: Tokens>(parser: &mut Parser<T>, shape: RegionShape) {
    let marker = parser.start();
    let mut taken = 0;

    for branch in &shape.branches {
        let open = parser.start();
        take(parser, branch.directive);
        body(parser, branch.body, shape.live);
        parser.complete(open, CONDITIONAL_BRANCH);
        taken += branch.directive + branch.body;
    }

    // Whatever the branches did not cover is the `` `endif ``, which closes
    // the region rather than belonging to its last branch. An unclosed region
    // leaves nothing here, so the two cases need no telling apart.
    take(parser, shape.len.saturating_sub(taken));

    parser.complete(marker, CONDITIONAL_REGION);
}

/// The text one branch guards.
///
/// Bounded, and that matters: a ragged branch does not balance, so a run
/// inside one would otherwise go looking for the `end` that the *next* branch
/// writes and swallow the rest of the region.
///
/// The bound is on the rules, not on the shapes. A macro call is delimited
/// against the whole file rather than against this branch, so one whose
/// argument list ran past the `` `endif `` would take it -- the tokens are
/// still all emitted, in order, so the text round-trips and only the node
/// boundaries move. Clipping shapes here would mean the branch disagreeing
/// with the scan about where a call ends, which is worse; no call in the
/// corpus is written that way.
fn body<T: Tokens>(parser: &mut Parser<T>, len: u32, live: bool) {
    let end = parser.ahead(len);
    while !parser.at_end() && parser.position() < end {
        let at = parser.position();
        match live {
            true => super::any(parser, Some(end)),
            false => ragged(parser, Some(end)),
        }
        if parser.position() == at {
            break;
        }
    }
}

/// What a ragged branch may hold.
///
/// The preprocessor's own structure and declarations, both of which are
/// self-contained and all-or-nothing, and the fallback for everything else.
/// No rule that looks for a closing keyword runs here, because in a ragged
/// region the keyword that closes what this branch opened is in a different
/// branch.
fn ragged<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    if parser.at(TICK_IDENT) && any(parser) {
        return;
    }
    if declaration(parser).is_some() {
        return;
    }
    verbatim(parser, Context::Terminated, limit);
}

/// Takes `len` tokens as they were lexed.
fn take<T: Tokens>(parser: &mut Parser<T>, len: u32) {
    for _ in 0..len {
        parser.bump();
    }
}
