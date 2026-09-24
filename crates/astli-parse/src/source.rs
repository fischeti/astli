//! Token stream abstraction for the parser.
//!
//! The parser operates over a stream implementing [`Tokens`]. Two token stream
//! representations are provided:
//! - [`Raw`]: Reads source tokens directly without macro expansion, preserving all
//!   conditional branches and macro references for syntax formatting and lossless AST generation.
//! - [`Expanded`]: Reads macro-expanded and include-processed tokens for semantic analysis.
//!   Only tests read it until expanded mode has a tree builder.
//!
//! Both implementations filter out trivia (whitespace and comments) from the stream
//! presented to grammar rules. Trivia is reattached during tree assembly in [`build()`](crate::build()).

use std::ops::Range;

use rustc_hash::FxHashMap;

use astli_text::Span;

use astli_preproc::{
    DirectiveType, Input, Item, MacroTable, Operands, Region, TokenSpan, regions, scan_seeded,
};
use astli_syntax::{SyntaxKind, SyntaxKind::*};

#[cfg(test)]
use astli_preproc::ExpandedToken;

/// Cursor position within the grammar token stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Position(u32);

/// Geometry of a compiler directive within the token stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirectiveShape {
    pub ty: DirectiveType,
    /// Total grammar tokens covered by the directive, including its introducer.
    pub len: u32,
    /// Token range of a `` `define `` macro replacement body, if present and non-empty.
    pub body: Option<Range<u32>>,
}

/// Geometry of a conditional compilation region (`ifdef` … `endif`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegionShape {
    /// Whether all branches in the region open and close matching delimiters locally.
    pub live: bool,
    /// Total grammar tokens covered by the region, including the closing `` `endif ``.
    pub len: u32,
    /// Constituent conditional branches in source order.
    pub branches: Vec<BranchShape>,
}

/// Geometry of a single branch within a conditional compilation region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BranchShape {
    /// Token count of the introducing directive and its operands.
    pub directive: u32,
    /// Token count of the guarded branch body.
    pub body: u32,
}

/// Stream of non-trivia tokens consumed by parser rules.
pub(crate) trait Tokens {
    /// Returns the kind of the token `ahead` positions from the cursor.
    fn kind(&self, ahead: usize) -> SyntaxKind;

    /// Advances the cursor by one token.
    fn bump(&mut self);

    /// Returns the span of the token `ahead` positions from the cursor.
    fn span(&self, ahead: usize) -> Option<Span>;

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
pub(crate) struct Raw<'a> {
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
    #[cfg(test)]
    pub(crate) fn new(input: Input<'a>) -> Raw<'a> {
        Raw::seeded(input, MacroTable::new())
    }

    /// Returns the source text of the token `ahead` positions from the cursor.
    #[cfg(test)]
    fn text(&self, ahead: usize) -> &str {
        self.raw(ahead).map_or("", |raw| self.input.text(raw))
    }

    /// Creates a raw token stream seeded with predefined macros.
    pub(crate) fn seeded(input: Input<'a>, seed: MacroTable) -> Raw<'a> {
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

    fn span(&self, ahead: usize) -> Option<Span> {
        let file = self.input.src_id;
        match self.raw(ahead) {
            Some(raw) => {
                let token = self.input.token(raw);
                Some(Span::new(file, token.start, token.end))
            }
            None => {
                let last = self.input.len().checked_sub(1)?;
                let token = self.input.token(last);
                Some(Span::point(file, token.end))
            }
        }
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
#[cfg(test)]
pub(crate) struct Expanded<'a> {
    tokens: &'a [ExpandedToken],
    grammar: Vec<u32>,
    at: u32,
}

#[cfg(test)]
impl<'a> Expanded<'a> {
    /// Creates an expanded token stream from expanded tokens.
    pub(crate) fn new(tokens: &'a [ExpandedToken]) -> Expanded<'a> {
        Expanded {
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

#[cfg(test)]
impl Tokens for Expanded<'_> {
    fn kind(&self, ahead: usize) -> SyntaxKind {
        self.token(ahead).map_or(EOF, |token| token.kind)
    }

    fn span(&self, ahead: usize) -> Option<Span> {
        self.token(ahead)
            .or_else(|| self.tokens.last())
            .map(|token| token.span)
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

#[cfg(test)]
mod tests {
    //! What a rule sees, and what it is spared.

    use super::*;
    use crate::testing::Source;
    use astli_preproc::ExpandedToken;

    /// Every kind a rule would see, from the cursor to the end.
    fn kinds(tokens: &mut impl Tokens) -> Vec<SyntaxKind> {
        let mut out = Vec::new();
        while !tokens.at_end() {
            out.push(tokens.kind(0));
            tokens.bump();
        }
        out
    }

    #[test]
    fn trivia_is_not_in_the_grammar_view() {
        let source = Source::new("module /* why */ m ; // done\n");
        let mut raw = Raw::new(source.input());
        assert_eq!(kinds(&mut raw), [MODULE_KW, IDENT, SEMICOLON]);
    }

    #[test]
    fn lookahead_reaches_past_trivia() {
        let source = Source::new("assign  x\n  =  1 ;");
        let raw = Raw::new(source.input());
        assert_eq!(raw.kind(0), ASSIGN_KW);
        assert_eq!(raw.kind(1), IDENT);
        assert_eq!(raw.kind(2), EQ);
        assert_eq!(raw.kind(3), INT_LITERAL);
        assert_eq!(raw.text(1), "x");
    }

    #[test]
    fn looking_past_the_end_gives_eof_forever() {
        let source = Source::new("endmodule");
        let raw = Raw::new(source.input());
        assert_eq!(raw.kind(0), ENDMODULE_KW);
        for ahead in 1..50 {
            assert_eq!(raw.kind(ahead), EOF, "{ahead} ahead");
        }
        assert_eq!(raw.text(9), "");
    }

    #[test]
    fn bumping_past_the_end_stays_at_the_end() {
        let source = Source::new("x");
        let mut raw = Raw::new(source.input());
        for _ in 0..10 {
            raw.bump();
        }
        assert!(raw.at_end());
        assert_eq!(raw.kind(0), EOF);
    }

    #[test]
    fn seeking_puts_the_cursor_back() {
        let source = Source::new("module m ; endmodule");
        let mut raw = Raw::new(source.input());
        raw.bump();

        let mark: Position = raw.at();
        let seen = kinds(&mut raw);
        assert_eq!(seen, [IDENT, SEMICOLON, ENDMODULE_KW]);

        raw.seek(mark);
        assert_eq!(kinds(&mut raw), seen);
    }

    #[test]
    fn a_macro_reference_is_one_thing_with_a_length() {
        let source = Source::new("`define W(x) x\nassign y = `W(1 + 2) ;");
        let mut raw = Raw::new(source.input());

        // Walk to the reference, which is the only position that answers.
        let mut lengths = Vec::new();
        while !raw.at_end() {
            lengths.push(raw.macro_call());
            raw.bump();
        }

        // `W ( 1 + 2 ) -- six tokens, the definition's own `W is not a call.
        assert_eq!(lengths.iter().flatten().copied().collect::<Vec<_>>(), [6]);
    }

    #[test]
    fn a_reference_without_arguments_is_one_token() {
        let source = Source::new("`define W 8\nlogic [`W-1:0] x;");
        let mut raw = Raw::new(source.input());
        while raw.macro_call().is_none() && !raw.at_end() {
            raw.bump();
        }
        assert_eq!(raw.text(0), "`W");
        assert_eq!(raw.macro_call(), Some(1));
    }

    #[test]
    fn the_two_streams_read_a_plain_file_identically() {
        // The abstraction's whole claim: with nothing for the preprocessor to do,
        // a rule cannot tell which stream it has. Anything that made the expanded
        // path drop or add a token the grammar can see would show up here.
        let text = "module m #( parameter int W = 8 ) ( input logic clk ) ;\n\
                      // a comment\n\
                      always_ff @( posedge clk ) q <= 8'hFF ;\n\
                    endmodule\n";

        let mut source = Source::new(text);
        // Raw first: the expanded reading borrows the store the raw one is read
        // out of.
        let raw = kinds(&mut Raw::new(source.input()));
        let tokens = source.session.expand(source.file).tokens;
        let mut expanded = Expanded::new(&tokens);

        assert_eq!(raw, kinds(&mut expanded));
    }

    #[test]
    fn the_expanded_stream_has_no_macro_calls_left() {
        let mut source = Source::new("`define W 8\nlogic [`W-1:0] x;\n");
        let tokens = source.session.expand(source.file).tokens;
        let mut expanded = Expanded::new(&tokens);

        let mut seen = Vec::new();
        while !expanded.at_end() {
            assert_eq!(expanded.macro_call(), None);
            seen.push(expanded.kind(0));
            expanded.bump();
        }
        // The reference became what it stands for.
        assert!(seen.contains(&INT_LITERAL));
        assert!(!seen.contains(&TICK_IDENT));
    }

    #[test]
    fn the_expanded_stream_has_none_of_this_to_shape() {
        // The reference is gone, the directive has run, and the branch taken is
        // simply the text, so a rule shaping these in raw mode does nothing here.
        let mut source = Source::new("`define W 8\n`ifdef W\nlogic [`W-1:0] x;\n`endif\n");
        let tokens: Vec<ExpandedToken> = source.session.expand(source.file).tokens;

        let mut expanded = Expanded::new(&tokens);
        while !expanded.at_end() {
            assert_eq!(expanded.macro_call(), None);
            assert_eq!(expanded.directive(), None);
            assert_eq!(expanded.region(), None);
            expanded.bump();
        }
    }
}
