//! Token stream abstraction for the parser.
//!
//! The parser operates over a stream implementing [`Tokens`]. Two token stream
//! representations are provided:
//! - [`Raw`]: Reads source tokens directly without macro expansion, preserving all
//!   conditional branches and macro references for syntax formatting and lossless AST generation.
//! - [`Expanded`]: Reads macro-expanded and include-processed tokens for semantic analysis.
//!
//! Both implementations filter out trivia (whitespace and comments) from the stream
//! presented to grammar rules. Trivia is reattached during tree assembly in [`build()`](crate::build()).

use std::ops::Range;

use rustc_hash::FxHashMap;

use svirig_text::{Origins, Span, TokenOrigin};

use svirig_preproc::{
    DirectiveType, ExpandedToken, Input, Item, MacroTable, Operands, Region, TokenSpan, regions,
    scan_seeded,
};
use svirig_syntax::{SyntaxKind, SyntaxKind::*};

/// Cursor position within the grammar token stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position(u32);

/// Geometry of a compiler directive within the token stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectiveShape {
    pub ty: DirectiveType,
    /// Total grammar tokens covered by the directive, including its introducer.
    pub len: u32,
    /// Token range of a `` `define `` macro replacement body, if present and non-empty.
    pub body: Option<Range<u32>>,
}

/// Geometry of a conditional compilation region (`ifdef` … `endif`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionShape {
    /// Whether all branches in the region open and close matching delimiters locally.
    pub live: bool,
    /// Total grammar tokens covered by the region, including the closing `` `endif ``.
    pub len: u32,
    /// Constituent conditional branches in source order.
    pub branches: Vec<BranchShape>,
}

/// Geometry of a single branch within a conditional compilation region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BranchShape {
    /// Token count of the introducing directive and its operands.
    pub directive: u32,
    /// Token count of the guarded branch body.
    pub body: u32,
}

/// Stream of non-trivia tokens consumed by parser rules.
pub trait Tokens {
    /// Returns the kind of the token `ahead` positions from the cursor.
    fn kind(&self, ahead: usize) -> SyntaxKind;

    /// Returns the source text of the token `ahead` positions from the cursor.
    fn text(&self, ahead: usize) -> &str;

    /// Advances the cursor by one token.
    fn bump(&mut self);

    /// Returns the source origin of the token `ahead` positions from the cursor.
    fn origin(&self, ahead: usize) -> Option<TokenOrigin>;

    /// Returns the current cursor position.
    fn at(&self) -> Position;

    /// Returns the position `ahead` tokens from the cursor, clamped to the stream end.
    fn ahead(&self, ahead: u32) -> Position;

    /// Returns `true` if the token `ahead` touches the preceding token without intervening trivia.
    fn adjacent(&self, ahead: usize) -> bool;

    /// Seeks the cursor to a previously recorded position.
    fn seek(&mut self, to: Position);

    /// Returns the token count covered by a macro call at the cursor, if present.
    fn macro_call(&self) -> Option<u32>;

    /// Returns the directive shape at the cursor, if present.
    fn directive(&self) -> Option<DirectiveShape>;

    /// Returns the conditional region shape at the cursor, if present.
    fn region(&self) -> Option<RegionShape>;

    /// Returns `true` if the cursor has reached the end of the stream.
    fn at_end(&self) -> bool {
        self.kind(0) == EOF
    }
}

/// Maps a raw token index to its grammar position, if non-trivia.
fn position(grammar: &[u32], at: u32) -> Option<u32> {
    grammar.binary_search(&at).ok().map(|at| at as u32)
}

/// Checks whether grammar token `at` directly abuts its predecessor in the raw token stream.
fn touching(grammar: &[u32], at: usize) -> bool {
    match at.checked_sub(1).and_then(|before| grammar.get(before)) {
        Some(&before) => grammar.get(at).is_some_and(|&raw| raw == before + 1),
        None => false,
    }
}

/// Counts the number of grammar tokens contained within a raw token index range.
fn count(grammar: &[u32], range: Range<u32>) -> u32 {
    let from = grammar.partition_point(|&raw| raw < range.start);
    let to = grammar.partition_point(|&raw| raw < range.end);
    (to - from) as u32
}

/// Extracts indices of all non-trivia tokens.
fn grammar_tokens(kinds: impl Iterator<Item = SyntaxKind>) -> Vec<u32> {
    kinds
        .enumerate()
        .filter(|(_, kind)| !kind.is_trivia())
        .map(|(at, _)| at as u32)
        .collect()
}

/// Number of tracked delimiter pairs for regional balance checks.
const PAIRS: usize = 8;

/// Returns the pair index and nesting delta for `kind`, if tracked.
fn pair(kind: SyntaxKind, previous: SyntaxKind) -> Option<(usize, i32)> {
    Some(match kind {
        L_PAREN => (0, 1),
        R_PAREN => (0, -1),
        L_BRACK => (1, 1),
        R_BRACK => (1, -1),
        L_BRACE | APOSTROPHE_L_BRACE => (2, 1),
        R_BRACE => (2, -1),
        BEGIN_KW => (3, 1),
        END_KW => (3, -1),
        CASE_KW | CASEX_KW | CASEZ_KW | RANDCASE_KW => (4, 1),
        ENDCASE_KW => (4, -1),
        FORK_KW if !matches!(previous, DISABLE_KW | WAIT_KW) => (5, 1),
        JOIN_KW | JOIN_ANY_KW | JOIN_NONE_KW => (5, -1),
        MODULE_KW | MACROMODULE_KW => (6, 1),
        ENDMODULE_KW => (6, -1),
        GENERATE_KW => (7, 1),
        ENDGENERATE_KW => (7, -1),
        _ => return None,
    })
}

/// Checks whether all tracked delimiter pairs balance within `body`.
fn balanced(input: &Input, directives: &[TokenSpan], body: TokenSpan) -> bool {
    let mut net = [0i32; PAIRS];
    delta(input, directives, body, &mut net);
    net.iter().all(|&count| count == 0)
}

/// Calculates net delimiter counts across a token span.
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

/// Raw source token stream preserving unexpanded macros and all conditional branches.
pub struct Raw<'a> {
    input: Input<'a>,
    grammar: Vec<u32>,
    at: u32,
    shapes: Shapes,
}

/// Precomputed geometry of directives, macro calls, and conditional regions.
#[derive(Debug, Default)]
struct Shapes {
    calls: FxHashMap<u32, u32>,
    directives: FxHashMap<u32, DirectiveShape>,
    regions: FxHashMap<u32, RegionShape>,
}

impl Shapes {
    fn of(input: &Input, grammar: &[u32], seed: MacroTable) -> Shapes {
        let mut shapes = Shapes::default();

        let found = scan_seeded(input, seed);
        for item in &found.items {
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
                            body: body.filter(|body| body.start < body.end),
                        },
                    );
                }
            }
        }

        let directives: Vec<TokenSpan> = found
            .directives()
            .map(|directive| directive.tokens)
            .collect();
        shapes.nest(input, grammar, &directives, input.span(0..input.len()));
        shapes
    }

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
    /// Creates a raw token stream with default preprocessor macro definitions.
    pub fn new(input: Input<'a>) -> Raw<'a> {
        Raw::seeded(input, MacroTable::new())
    }

    /// Creates a raw token stream seeded with predefined macros.
    pub fn seeded(input: Input<'a>, seed: MacroTable) -> Raw<'a> {
        let grammar = grammar_tokens(input.tokens.iter().map(|token| token.kind));
        let shapes = Shapes::of(&input, &grammar, seed);

        Raw {
            input,
            grammar,
            at: 0,
            shapes,
        }
    }

    fn raw(&self, ahead: usize) -> Option<u32> {
        self.grammar.get(self.at as usize + ahead).copied()
    }
}

impl Tokens for Raw<'_> {
    fn kind(&self, ahead: usize) -> SyntaxKind {
        self.raw(ahead).map_or(EOF, |raw| self.input.kind(raw))
    }

    fn origin(&self, ahead: usize) -> Option<TokenOrigin> {
        let file = self.input.file;
        match self.raw(ahead) {
            Some(raw) => {
                let token = self.input.token(raw);
                Some(TokenOrigin::written(Span::new(
                    file,
                    token.start,
                    token.end,
                )))
            }
            None => {
                let last = self.input.len().checked_sub(1)?;
                let token = self.input.token(last);
                Some(TokenOrigin::written(Span::point(file, token.end)))
            }
        }
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

/// Token stream representing fully expanded source tokens.
pub struct Expanded<'a> {
    origins: &'a Origins,
    tokens: &'a [ExpandedToken],
    grammar: Vec<u32>,
    at: u32,
}

impl<'a> Expanded<'a> {
    /// Creates an expanded token stream from expanded tokens and their source origin map.
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

    fn origin(&self, ahead: usize) -> Option<TokenOrigin> {
        self.token(ahead)
            .or_else(|| self.tokens.last())
            .map(|token| token.origin)
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
        None
    }

    fn directive(&self) -> Option<DirectiveShape> {
        None
    }

    fn region(&self) -> Option<RegionShape> {
        None
    }
}
