//! Random input, held to the two properties that must hold for *all* input.
//!
//! **It comes back out byte for byte, and nothing panics.** Everything else
//! the parser does is a judgement about what the text means, and a judgement
//! can be wrong without the tool being broken -- that is what
//! [the fallback](../src/parser/verbatim.rs) is for. These two are not
//! judgements. A formatter that loses a byte is a formatter nobody may run,
//! and a parser that panics on the fourth file of a repository is one nobody
//! will run twice.
//!
//! # Why a seeded generator and not `cargo-fuzz`
//!
//! libFuzzer finds more, given a week and a corpus of its own. What is wanted
//! here is a property that runs on every `cargo nextest run`, on the machine
//! of whoever broke it, and reports the *same* failure twice -- so the input
//! is generated from a counter and the seed is in the panic message. A real
//! fuzz target is worth adding when there is CI to run it in; it would use
//! these same generators.
//!
//! # What is generated
//!
//! Random bytes find the lexer's edges and nothing else: almost none of it
//! reaches a grammar rule. So most of what is generated is a **random sequence
//! of real tokens** -- keywords that open and close bodies, punctuation,
//! macro references -- which is what puts a rule somewhere it was never
//! written to be. The third generator splices corpus files together, which is
//! the only one that produces input that is nearly valid, and nearly valid is
//! where a rule that reads one token too far shows up.

use rowan::NodeOrToken;
use svirig_parse::parse;
use svirig_preproc::Session;
use svirig_syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode};

mod corpus;

/// xorshift64*, so that a seed names an input and a failure repeats.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        // Zero is the one state this generator cannot leave.
        Rng(seed | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn pick<'a, T>(&mut self, from: &'a [T]) -> &'a T {
        &from[self.below(from.len())]
    }
}

/// Tokens chosen for what they make a rule do, not for how often they are
/// written.
///
/// Every keyword that opens a body is here with the one that closes it, and
/// several that close a body nothing opened -- which is the case the fallback's
/// delimiter stack exists for.
const SPELLINGS: &[&str] = &[
    // The shells, and their closers.
    "module",
    "endmodule",
    "interface",
    "endinterface",
    "package",
    "endpackage",
    "program",
    "endprogram",
    "class",
    "endclass",
    "function",
    "endfunction",
    "task",
    "endtask",
    "generate",
    "endgenerate",
    "case",
    "endcase",
    "begin",
    "end",
    "fork",
    "join",
    "join_any",
    "constraint",
    "covergroup",
    "endgroup",
    "clocking",
    "endclocking",
    "specify",
    "endspecify",
    // What goes in one.
    "assign",
    "always_ff",
    "always_comb",
    "initial",
    "final",
    "if",
    "else",
    "for",
    "foreach",
    "while",
    "do",
    "repeat",
    "forever",
    "return",
    "break",
    "continue",
    "disable",
    "wait",
    "unique",
    "priority",
    "default",
    "modport",
    "import",
    "export",
    "extends",
    "implements",
    // Declarations, and the words that qualify one.
    "typedef",
    "parameter",
    "localparam",
    "enum",
    "struct",
    "union",
    "packed",
    "logic",
    "wire",
    "int",
    "bit",
    "string",
    "void",
    "type",
    "var",
    "const",
    "genvar",
    "input",
    "output",
    "inout",
    "ref",
    "virtual",
    "extern",
    "pure",
    "static",
    "automatic",
    "local",
    "protected",
    "rand",
    "signed",
    "unsigned",
    "posedge",
    "negedge",
    "or",
    "iff",
    "inside",
    "new",
    "this",
    "super",
    "null",
    // Punctuation.
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    "'{",
    ";",
    ",",
    ".",
    "::",
    ":",
    "#",
    "##",
    "@",
    "=",
    "<=",
    "==",
    "+",
    "-",
    "*",
    "/",
    "?",
    "'",
    "->",
    "->>",
    "++",
    "+:",
    "&",
    "|",
    "^",
    "!",
    "~",
    "<",
    ">",
    "*)",
    "(*",
    // Names, numbers, and the preprocessor.
    "a",
    "b",
    "x_i",
    "foo_t",
    "pkg",
    "u_foo",
    "\\esc ",
    "$display",
    "1",
    "42",
    "8'hFF",
    "'h",
    "1.5",
    "10ns",
    "\"s\"",
    "`define",
    "`ifdef",
    "`elsif",
    "`else",
    "`endif",
    "`include",
    "`FOO",
    "`BAR(a, b)",
];

/// What may stand between two tokens, including nothing at all.
const GAPS: &[&str] = &[
    " ",
    "\n",
    "  ",
    "\t",
    "",
    " // c\n",
    " /* c */ ",
    "\r\n",
    " \\\n",
];

/// A random sequence of real tokens, written out as text.
fn tokens(rng: &mut Rng, len: usize) -> String {
    let mut out = String::new();
    for _ in 0..len {
        out.push_str(rng.pick(SPELLINGS));
        out.push_str(rng.pick(GAPS));
    }
    out
}

/// Random bytes, for the lexer's edges rather than the grammar's.
fn bytes(rng: &mut Rng, len: usize) -> String {
    const ALPHABET: &[char] = &[
        'a', 'z', '_', '0', '9', '$', '\\', '`', '\'', '"', '/', '*', '(', ')', '{', '}', '[', ']',
        ';', ':', '.', '#', '@', '=', '<', '>', '+', '-', '?', '|', '&', '^', '~', '!', '%', ' ',
        '\n', '\t', '\r', 'é', '→', '\u{0}',
    ];
    (0..len).map(|_| *rng.pick(ALPHABET)).collect()
}

/// Chunks of real files, cut at random points and put back in another order.
///
/// The one generator that makes input which is *nearly* valid, which is where
/// a rule that reads one token too far shows up.
fn splice(rng: &mut Rng, sources: &[String]) -> String {
    let mut out = String::new();
    for _ in 0..1 + rng.below(4) {
        let source = rng.pick(sources);
        if source.is_empty() {
            continue;
        }
        let mut from = rng.below(source.len());
        let mut to = from + rng.below(600);
        to = to.min(source.len());
        while from > 0 && !source.is_char_boundary(from) {
            from -= 1;
        }
        while to > from && !source.is_char_boundary(to) {
            to -= 1;
        }
        out.push_str(&source[from..to]);
        out.push('\n');
    }
    out
}

/// How many inputs a generator makes here.
///
/// A twentieth as many in debug, because `cargo nextest run -P quick` is the
/// tight loop and a second spent here is a second spent on every edit. The
/// full run is a release run and gets the whole number, which is where a
/// generator of this kind earns anything at all.
fn cases(many: u64) -> u64 {
    match cfg!(debug_assertions) {
        true => (many / 20).max(50),
        false => many,
    }
}

/// The two properties, plus the one structural claim that holds for any input
/// at all.
fn check(text: &str, what: &str) {
    let mut session = Session::new();
    let file = session.add("fuzz.sv", text.to_string());
    let tree = parse(session.input(file));

    assert_eq!(
        tree.text().to_string(),
        text,
        "the tree is not the input, from {what}"
    );
    assert_eq!(tree.kind(), SOURCE_FILE, "no root, from {what}");

    // A rule that closed a node over text it never read would show here and
    // nowhere else: the bytes would still round-trip.
    if let Some(bad) = malformed(&tree) {
        panic!("{bad}, from {what}");
    }
}

/// The first shell whose own tokens are not the keyword it claims and the
/// `end…` that matches, if there is one.
fn malformed(node: &SyntaxNode) -> Option<String> {
    fn bounds(kind: SyntaxKind) -> Option<(&'static [SyntaxKind], SyntaxKind)> {
        Some(match kind {
            MODULE_DECL => (&[MODULE_KW, MACROMODULE_KW][..], ENDMODULE_KW),
            INTERFACE_DECL => (&[INTERFACE_KW], ENDINTERFACE_KW),
            PROGRAM_DECL => (&[PROGRAM_KW], ENDPROGRAM_KW),
            PACKAGE_DECL => (&[PACKAGE_KW], ENDPACKAGE_KW),
            CLASS_DECL => (&[CLASS_KW, VIRTUAL_KW, INTERFACE_KW], ENDCLASS_KW),
            GENERATE_REGION => (&[GENERATE_KW], ENDGENERATE_KW),
            CASE_STMT => (
                &[
                    CASE_KW,
                    CASEX_KW,
                    CASEZ_KW,
                    UNIQUE_KW,
                    UNIQUE0_KW,
                    PRIORITY_KW,
                ],
                ENDCASE_KW,
            ),
            _ => return None,
        })
    }

    if let Some((open, close)) = bounds(node.kind()) {
        let mut own: Vec<SyntaxKind> = node
            .children_with_tokens()
            .filter_map(NodeOrToken::into_token)
            .map(|token| token.kind())
            .filter(|kind| !kind.is_trivia())
            .collect();
        // `endmodule : m` -- the label is the node's too.
        if own.len() >= 3 && own[own.len() - 2] == COLON {
            own.truncate(own.len() - 2);
        }
        if !own.first().is_some_and(|kind| open.contains(kind)) || own.last() != Some(&close) {
            return Some(format!("{:?} is {own:?}", node.kind()));
        }
    }

    node.children().find_map(|child| malformed(&child))
}

#[test]
fn random_token_sequences_round_trip() {
    for seed in 0..cases(20_000) {
        let mut rng = Rng::new(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        let len = 1 + rng.below(80);
        let text = tokens(&mut rng, len);
        check(&text, &format!("seed {seed}"));
    }
}

#[test]
fn long_random_token_sequences_round_trip() {
    // Long enough that a shell opens, fills and closes by accident, which is
    // what the short ones almost never do.
    for seed in 0..cases(600) {
        let mut rng = Rng::new(seed.wrapping_mul(0xbf58_476d_1ce4_e5b9) ^ 0x5eed);
        let len = 2000 + rng.below(2000);
        let text = tokens(&mut rng, len);
        check(&text, &format!("long seed {seed}"));
    }
}

#[test]
fn random_bytes_round_trip() {
    for seed in 0..cases(15_000) {
        let mut rng = Rng::new(seed.wrapping_mul(0x94d0_49bb_1331_11eb) ^ 0xb17e);
        let len = 1 + rng.below(200);
        let text = bytes(&mut rng, len);
        check(&text, &format!("byte seed {seed}"));
    }
}

#[test]
fn the_empty_file_and_the_shortest_ones() {
    check("", "empty");
    for spelling in SPELLINGS {
        check(spelling, spelling);
        check(&format!("{spelling}{spelling}"), spelling);
    }
}

/// The nearly-valid case: real text, cut where nobody would cut it.
#[test]
fn corpus_spliced_files_round_trip() {
    let Some(files) = corpus::files() else {
        return;
    };

    // Enough files for variety, few enough that the test stays a test.
    let sources: Vec<String> = files
        .iter()
        .step_by(37)
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .collect();
    assert!(!sources.is_empty(), "the corpus read as nothing");

    for seed in 0..cases(10_000) {
        let mut rng = Rng::new(seed.wrapping_mul(0xd6e8_feb8_6659_fd93) ^ 0x5f1c_e000);
        let text = splice(&mut rng, &sources);
        check(&text, &format!("splice seed {seed}"));
    }
}
