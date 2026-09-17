//! What a rule sees, and what it is spared.

use svirig_syntax::parser::{Expanded, Position, Raw, Tokens};
use svirig_syntax::preproc::{ExpandedToken, Includes, Input, expand};
use svirig_syntax::{SyntaxKind, SyntaxKind::*, Token, tokenize};
use svirig_text::{Disk, FileId, Origins};

/// One file, kept alive so that both streams can be read out of it.
struct Source {
    origins: Origins,
    file: FileId,
    tokens: Vec<Token>,
}

impl Source {
    fn new(text: &str) -> Source {
        let mut origins = Origins::new();
        let file = origins.add_file("top.sv", text.to_string());
        let tokens = tokenize(origins.text(file));
        Source {
            origins,
            file,
            tokens,
        }
    }

    fn raw(&self) -> Raw<'_> {
        Raw::new(Input::new(
            self.file,
            self.origins.text(self.file),
            &self.tokens,
        ))
    }

    /// The same file after the preprocessor has had it.
    fn expanded(&self) -> (Origins, Vec<ExpandedToken>) {
        let mut origins = Origins::new();
        let file = origins.add_file("top.sv", self.origins.text(self.file).to_string());
        let tokens = expand(&mut origins, file, &Includes::new(), &Disk);
        (origins, tokens)
    }
}

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
    let mut raw = source.raw();
    assert_eq!(kinds(&mut raw), [MODULE_KW, IDENT, SEMICOLON]);
}

#[test]
fn lookahead_reaches_past_trivia() {
    let source = Source::new("assign  x\n  =  1 ;");
    let raw = source.raw();
    assert_eq!(raw.kind(0), ASSIGN_KW);
    assert_eq!(raw.kind(1), IDENT);
    assert_eq!(raw.kind(2), EQ);
    assert_eq!(raw.kind(3), INT_LITERAL);
    assert_eq!(raw.text(1), "x");
}

#[test]
fn looking_past_the_end_gives_eof_forever() {
    let source = Source::new("endmodule");
    let raw = source.raw();
    assert_eq!(raw.kind(0), ENDMODULE_KW);
    for ahead in 1..50 {
        assert_eq!(raw.kind(ahead), EOF, "{ahead} ahead");
    }
    assert_eq!(raw.text(9), "");
}

#[test]
fn bumping_past_the_end_stays_at_the_end() {
    let source = Source::new("x");
    let mut raw = source.raw();
    for _ in 0..10 {
        raw.bump();
    }
    assert!(raw.at_end());
    assert_eq!(raw.kind(0), EOF);
}

#[test]
fn seeking_puts_the_cursor_back() {
    let source = Source::new("module m ; endmodule");
    let mut raw = source.raw();
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
    let mut raw = source.raw();

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
    let mut raw = source.raw();
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

    let source = Source::new(text);
    let (origins, tokens) = source.expanded();
    let mut expanded = Expanded::new(&origins, &tokens);

    assert_eq!(kinds(&mut source.raw()), kinds(&mut expanded));
}

#[test]
fn the_expanded_stream_has_no_macro_calls_left() {
    let source = Source::new("`define W 8\nlogic [`W-1:0] x;\n");
    let (origins, tokens) = source.expanded();
    let mut expanded = Expanded::new(&origins, &tokens);

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
