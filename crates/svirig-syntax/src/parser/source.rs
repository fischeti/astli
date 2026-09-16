//! Where a rule gets its tokens, and the one thing the grammar is generic
//! over.
//!
//! The preprocessor produces two streams from the same machinery: the **raw**
//! one, which keeps macro calls as written and every branch of a conditional,
//! and the **expanded** one, which has substituted the macros and followed the
//! includes. A formatter has to read the first and a compiler the second, and
//! the whole point of [`Tokens`] is that the grammar between them is one
//! grammar. A rule asks what kind is at the cursor; it does not ask which
//! stream it is reading.
//!
//! # Trivia is not here
//!
//! A rule never sees whitespace or a comment. Both streams keep them --
//! nothing is lost -- but a position here counts only tokens the grammar cares
//! about, so no rule has to remember to skip. Putting them back is the tree
//! builder's job, and it walks the original tokens to do it.
//!
//! That is also why a [`Position`] is an index into the *grammar* tokens
//! rather than a byte offset or a raw token index: it is the coordinate both
//! implementations can offer, and the only one a rule should ever hold.
//!
//! # `EOF` is a token
//!
//! Looking past the end gives [`EOF`] rather than `None`, as
//! many times as it is asked. A rule that has run off the end therefore
//! behaves like one that reached the end, which is the behaviour every rule
//! wants and none would remember to write.
//!
//! # What the preprocessor already knows, in grammar positions
//!
//! Three questions a rule asks that no amount of looking at token kinds can
//! answer -- [`Tokens::macro_call`], [`Tokens::directive`] and
//! [`Tokens::region`]. A `` `name `` is a directive or
//! a macro reference depending on a table, an argument list depends on the
//! macro's arity, and a conditional region is five directives that only mean
//! anything together. All of it is worked out once, when the stream is built,
//! and reported here as **counts of grammar tokens** so that a rule can bump
//! its way through a shape without ever leaving the coordinate it lives in.
//!
//! **Only the raw stream ever answers.** An expansion leaves no reference
//! behind and a directive has already been executed, so a rule that shapes
//! these correctly in raw mode does nothing at all in expanded mode, without
//! asking why.

use std::ops::Range;

use rustc_hash::FxHashMap;

use svirig_text::Origins;

use crate::preproc::{
    DirectiveType, ExpandedToken, Input, Item, Operands, Region, TokenSpan, regions, scan,
};
use crate::{SyntaxKind, SyntaxKind::*};

/// How far through the tokens a parse is.
///
/// Opaque, and only ever compared or handed back: what it counts is one
/// implementation's business. Taking one and later [seeking](Tokens::seek) to
/// it is the token half of a rollback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position(u32);

/// A compiler directive at the cursor, measured in grammar tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectiveShape {
    pub ty: DirectiveType,
    /// How many tokens the directive covers, introducer included.
    pub len: u32,
    /// A `` `define ``'s substitution text, offset from the cursor. `None`
    /// for every other directive, and for a definition whose body is empty.
    pub body: Option<Range<u32>>,
}

/// One `` `ifdef `` … `` `endif `` at the cursor, measured in grammar tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionShape {
    /// Whether every branch of the region opens and closes whatever it opens,
    /// so that each is text the grammar may read in the enclosing context.
    ///
    /// A region where one branch hands a delimiter to another -- an
    /// `` `ifdef `` opening a `begin` that the `` `else `` closes -- is
    /// **ragged**, and no branch of it is a construct. There the parser runs
    /// only what is self-contained; see
    /// [`preprocessor`](super::preprocessor).
    ///
    /// Counted over raw tokens, because a rule that could see the answer for
    /// itself would already have had to read the branch to get it.
    pub live: bool,
    /// How many tokens the region covers, the `` `endif `` included where
    /// there is one.
    pub len: u32,
    /// In source order, starting with the `` `ifdef `` or `` `ifndef ``.
    /// Never empty, and **every branch written is here** -- raw mode keeps
    /// them all, because the formatter cannot evaluate the condition.
    pub branches: Vec<BranchShape>,
}

/// One branch of a [`RegionShape`], measured in grammar tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BranchShape {
    /// The branch's own introducing directive, its operand included.
    pub directive: u32,
    /// The text the branch guards, which runs to the next directive at this
    /// level.
    pub body: u32,
}

/// A stream of tokens with the trivia already stepped over.
pub trait Tokens {
    /// The kind `ahead` tokens from the cursor; `0` is the cursor itself.
    fn kind(&self, ahead: usize) -> SyntaxKind;

    /// The text of the token `ahead` of the cursor, as it is written in
    /// whatever file it came from.
    fn text(&self, ahead: usize) -> &str;

    /// Advances one token. At the end it stays there.
    fn bump(&mut self);

    /// Where the cursor is.
    fn at(&self) -> Position;

    /// The position `ahead` tokens from the cursor, clamped to the end.
    ///
    /// How a rule that has been told a shape's length turns it into a bound it
    /// can compare against, without ever doing arithmetic on a [`Position`].
    fn ahead(&self, ahead: u32) -> Position;

    /// Whether the token `ahead` of the cursor touches the one before it, with
    /// no whitespace or comment between them.
    ///
    /// The one thing a rule may ask about the trivia it otherwise cannot see,
    /// and it is asked for the one reason that survives: a *lexical* unit that
    /// came out as several tokens. `'h 4a43_f880` is a base and then two
    /// tokens, because `4a43_f880` is an integer followed by an identifier --
    /// and what says they are one number rather than two operands is that
    /// nothing separates them.
    fn adjacent(&self, ahead: usize) -> bool;

    /// Puts the cursor back where it was.
    fn seek(&mut self, to: Position);

    /// How many tokens the macro reference at the cursor covers, name and
    /// argument list together, or `None` if the cursor is not on one.
    ///
    /// Only the raw stream ever answers: on the expanded path a macro has
    /// already become the tokens it stands for, so there is no reference left
    /// to see. Which is the point -- a rule that handles a call correctly in
    /// raw mode does nothing at all in expanded mode, without asking why.
    fn macro_call(&self) -> Option<u32>;

    /// The directive at the cursor, or `None` where there is none.
    ///
    /// A conditional's introducer answers here as well, because it *is* a
    /// directive; a rule that wants the whole region has to ask
    /// [`region`](Tokens::region) first. Same `None` in expanded mode, and
    /// for the same reason as [`macro_call`](Tokens::macro_call): the
    /// directive has already been executed.
    fn directive(&self) -> Option<DirectiveShape>;

    /// The conditional region opening at the cursor, or `None`.
    ///
    /// Answers for a nested region as readily as for an outermost one, since
    /// a nested one is reached by parsing the branch that holds it.
    fn region(&self) -> Option<RegionShape>;

    /// Whether the cursor is at the end.
    fn at_end(&self) -> bool {
        self.kind(0) == EOF
    }
}

/// The grammar position of raw token `at`, if a rule can see it at all.
fn position(grammar: &[u32], at: u32) -> Option<u32> {
    grammar.binary_search(&at).ok().map(|at| at as u32)
}

/// Whether the grammar token at `at` follows its predecessor with nothing
/// dropped between them.
///
/// Both streams index into the tokens they were built from, so two grammar
/// tokens are adjacent exactly when the indices they came from are.
fn touching(grammar: &[u32], at: usize) -> bool {
    match at.checked_sub(1).and_then(|before| grammar.get(before)) {
        Some(&before) => grammar.get(at).is_some_and(|&raw| raw == before + 1),
        None => false,
    }
}

/// How many grammar tokens lie in a range of raw ones.
///
/// Counted rather than looked up at both ends, because an exclusive end -- and
/// the start of a trimmed operand span -- may sit on trivia.
fn count(grammar: &[u32], range: Range<u32>) -> u32 {
    let from = grammar.partition_point(|&raw| raw < range.start);
    let to = grammar.partition_point(|&raw| raw < range.end);
    (to - from) as u32
}

/// Indices of the tokens a grammar rule can see, in order.
fn grammar_tokens(kinds: impl Iterator<Item = SyntaxKind>) -> Vec<u32> {
    kinds
        .enumerate()
        .filter(|(_, kind)| !kind.is_trivia())
        .map(|(at, _)| at as u32)
        .collect()
}

/// The delimiter pairs a branch has to close for itself.
///
/// Brackets, `begin`, `case`, `fork`, `module` and `generate`: the ones whose
/// opener always opens something. `function`, `class`, `interface`, `property`
/// and `sequence` are left out because each has a prototype form with no
/// closer at all, and counting those would call a branch ragged for writing
/// `extern function void f();` -- the same five the fallback has to guess
/// about, and the same reason.
const PAIRS: usize = 8;

/// Which pair `kind` belongs to, and which way it counts.
fn pair(kind: SyntaxKind, previous: SyntaxKind) -> Option<(usize, i32)> {
    Some(match kind {
        L_PAREN => (0, 1),
        R_PAREN => (0, -1),
        L_BRACK => (1, 1),
        R_BRACK => (1, -1),
        // `'{` opens an assignment pattern and a plain `}` closes it.
        L_BRACE | APOSTROPHE_L_BRACE => (2, 1),
        R_BRACE => (2, -1),
        BEGIN_KW => (3, 1),
        END_KW => (3, -1),
        CASE_KW | CASEX_KW | CASEZ_KW | RANDCASE_KW => (4, 1),
        ENDCASE_KW => (4, -1),
        // `disable fork` and `wait fork` name a block rather than opening one.
        FORK_KW if !matches!(previous, DISABLE_KW | WAIT_KW) => (5, 1),
        JOIN_KW | JOIN_ANY_KW | JOIN_NONE_KW => (5, -1),
        MODULE_KW | MACROMODULE_KW => (6, 1),
        ENDMODULE_KW => (6, -1),
        GENERATE_KW => (7, 1),
        ENDGENERATE_KW => (7, -1),
        _ => return None,
    })
}

/// Whether a branch closes everything it opens.
fn balanced(input: &Input, directives: &[TokenSpan], body: TokenSpan) -> bool {
    let mut net = [0i32; PAIRS];
    delta(input, directives, body, &mut net);
    net.iter().all(|&count| count == 0)
}

/// Adds up what a stretch of text opens and closes.
///
/// A directive's own tokens contribute nothing -- its operands are its, and a
/// `` `define `` body is substitution text rather than code. A **nested**
/// region contributes its first branch only: adding every branch would count
/// code that never coexists, and one branch is what any given build sees.
fn delta(input: &Input, directives: &[TokenSpan], body: TokenSpan, net: &mut [i32; PAIRS]) {
    let nested = regions(input, body);
    let mut next = 0;
    let mut cursor = body.start;
    let mut previous = EOF;

    while cursor < body.end {
        if let Some(region) = nested
            .get(next)
            .filter(|region| region.tokens.start == cursor)
        {
            delta(input, directives, region.branches[0].body, net);
            cursor = region.tokens.end.max(cursor + 1);
            next += 1;
            continue;
        }

        if let Ok(at) = directives.binary_search_by_key(&cursor, |span| span.start) {
            cursor = directives[at].end.max(cursor + 1);
            continue;
        }

        let kind = input.kind(cursor);
        if !kind.is_trivia() && kind != LINE_CONTINUATION {
            if let Some((at, by)) = pair(kind, previous) {
                net[at] += by;
            }
            previous = kind;
        }
        cursor += 1;
    }
}

/// The stream as written: macros unexpanded, includes not followed, every
/// branch of every conditional present.
///
/// What the formatter reads. See `docs/preprocessor.md`.
pub struct Raw<'a> {
    input: Input<'a>,
    /// The raw index of each token a rule can see.
    grammar: Vec<u32>,
    at: u32,
    shapes: Shapes,
}

/// Everything the preprocessor found, keyed by the grammar position it starts
/// at.
///
/// Built once, when the stream is, because every one of these answers costs a
/// scan and a rule asks at nearly every token. What a rule reads back is in
/// the coordinate it lives in, so it can bump its way through a shape without
/// converting anything.
#[derive(Debug, Default)]
struct Shapes {
    /// How many tokens a macro reference covers, name and argument list
    /// together.
    calls: FxHashMap<u32, u32>,
    directives: FxHashMap<u32, DirectiveShape>,
    regions: FxHashMap<u32, RegionShape>,
}

impl Shapes {
    fn of(input: &Input, grammar: &[u32]) -> Shapes {
        let mut shapes = Shapes::default();

        for item in scan(input).items {
            let span = item.tokens();
            let Some(at) = position(grammar, span.start) else {
                continue;
            };
            match item {
                Item::Macro(_) => {
                    shapes
                        .calls
                        .insert(at, count(grammar, span.start..span.end));
                }
                Item::Directive(directive) => {
                    let body = match &directive.operands {
                        Operands::Define(def) => Some(
                            count(grammar, span.start..def.body.start)
                                ..count(grammar, span.start..def.body.end),
                        ),
                        _ => None,
                    };
                    shapes.directives.insert(
                        at,
                        DirectiveShape {
                            ty: directive.ty,
                            len: count(grammar, span.start..span.end),
                            // An empty body is no body: a rule would have
                            // nothing to put in the node.
                            body: body.filter(|body| body.start < body.end),
                        },
                    );
                }
            }
        }

        // Sorted by construction: `scan` walks the file forwards.
        let directives: Vec<TokenSpan> =
            scan(input).directives().map(|found| found.tokens).collect();
        shapes.nest(input, grammar, &directives, input.span(0..input.len()));
        shapes
    }

    /// Records every region directly inside `span`, then every region inside
    /// each of their branches.
    ///
    /// `regions` reports only the outermost, which is what lets one function
    /// answer for a file and for a branch alike -- and a nested region is
    /// reached by parsing the branch that holds it, so its introducer has to
    /// answer too.
    fn nest(&mut self, input: &Input, grammar: &[u32], directives: &[TokenSpan], span: TokenSpan) {
        for region in regions(input, span) {
            if let Some(at) = position(grammar, region.tokens.start) {
                self.regions
                    .insert(at, shape(input, grammar, directives, &region));
            }
            for branch in &region.branches {
                self.nest(input, grammar, directives, branch.body);
            }
        }
    }
}

fn shape(input: &Input, grammar: &[u32], directives: &[TokenSpan], region: &Region) -> RegionShape {
    RegionShape {
        live: region
            .branches
            .iter()
            .all(|branch| balanced(input, directives, branch.body)),
        len: count(grammar, region.tokens.start..region.tokens.end),
        branches: region
            .branches
            .iter()
            .map(|branch| BranchShape {
                directive: count(grammar, branch.directive.start..branch.directive.end),
                body: count(grammar, branch.body.start..branch.body.end),
            })
            .collect(),
    }
}

impl<'a> Raw<'a> {
    pub fn new(input: Input<'a>) -> Raw<'a> {
        let grammar = grammar_tokens(input.tokens.iter().map(|token| token.kind));
        let shapes = Shapes::of(&input, &grammar);

        Raw {
            input,
            grammar,
            at: 0,
            shapes,
        }
    }

    /// The raw index of the token `ahead` of the cursor, if there is one.
    fn raw(&self, ahead: usize) -> Option<u32> {
        self.grammar.get(self.at as usize + ahead).copied()
    }
}

impl Tokens for Raw<'_> {
    fn kind(&self, ahead: usize) -> SyntaxKind {
        self.raw(ahead).map_or(EOF, |raw| self.input.kind(raw))
    }

    fn text(&self, ahead: usize) -> &str {
        self.raw(ahead).map_or("", |raw| self.input.text(raw))
    }

    fn bump(&mut self) {
        if (self.at as usize) < self.grammar.len() {
            self.at += 1;
        }
    }

    fn at(&self) -> Position {
        Position(self.at)
    }

    fn ahead(&self, ahead: u32) -> Position {
        Position((self.at + ahead).min(self.grammar.len() as u32))
    }

    fn adjacent(&self, ahead: usize) -> bool {
        touching(&self.grammar, self.at as usize + ahead)
    }

    fn seek(&mut self, to: Position) {
        self.at = to.0;
    }

    fn macro_call(&self) -> Option<u32> {
        self.shapes.calls.get(&self.at).copied()
    }

    fn directive(&self) -> Option<DirectiveShape> {
        self.shapes.directives.get(&self.at).cloned()
    }

    fn region(&self) -> Option<RegionShape> {
        self.shapes.regions.get(&self.at).cloned()
    }
}

/// The stream a compiler would read: macros substituted, includes followed,
/// the branch the definitions select and no other.
pub struct Expanded<'a> {
    origins: &'a Origins,
    tokens: &'a [ExpandedToken],
    grammar: Vec<u32>,
    at: u32,
}

impl<'a> Expanded<'a> {
    pub fn new(origins: &'a Origins, tokens: &'a [ExpandedToken]) -> Expanded<'a> {
        Expanded {
            origins,
            grammar: grammar_tokens(tokens.iter().map(|token| token.kind)),
            tokens,
            at: 0,
        }
    }

    fn token(&self, ahead: usize) -> Option<&ExpandedToken> {
        let at = *self.grammar.get(self.at as usize + ahead)?;
        self.tokens.get(at as usize)
    }
}

impl Tokens for Expanded<'_> {
    fn kind(&self, ahead: usize) -> SyntaxKind {
        self.token(ahead).map_or(EOF, |token| token.kind)
    }

    fn text(&self, ahead: usize) -> &str {
        self.token(ahead)
            .map_or("", |token| self.origins.slice(token.origin.spelled))
    }

    fn bump(&mut self) {
        if (self.at as usize) < self.grammar.len() {
            self.at += 1;
        }
    }

    fn at(&self) -> Position {
        Position(self.at)
    }

    fn ahead(&self, ahead: u32) -> Position {
        Position((self.at + ahead).min(self.grammar.len() as u32))
    }

    fn adjacent(&self, ahead: usize) -> bool {
        touching(&self.grammar, self.at as usize + ahead)
    }

    fn seek(&mut self, to: Position) {
        self.at = to.0;
    }

    /// Always `None`: an expansion leaves no reference behind.
    fn macro_call(&self) -> Option<u32> {
        None
    }

    /// Always `None`: a directive on this path has already been executed.
    fn directive(&self) -> Option<DirectiveShape> {
        None
    }

    /// Always `None`: the branch that was taken is simply the text.
    fn region(&self) -> Option<RegionShape> {
        None
    }
}
