//! The inputs the fuzz tests make, shared by the parser's and the
//! formatter's so that both are held to the same text.
//!
//! Random bytes find the lexer's edges and nothing else: almost none of it
//! reaches a grammar rule. So most of what is generated is a **random sequence
//! of real tokens** -- keywords that open and close bodies, punctuation,
//! macro references -- which is what puts a rule somewhere it was never
//! written to be. Corpus files spliced together are the only input that is
//! nearly valid, and nearly valid is where a rule that reads one token too far
//! shows up.
//!
//! Each input is made from a counter, and comes with what names it: a failure
//! repeats, and says which seed to repeat it with.

// Each test binary compiles its own copy of this module, so a helper only one
// of them needs looks unused to the others.
#![allow(dead_code)]

use super::corpus;

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

/// Every construct that nests, nested far deeper than anyone writes: the
/// input the random generators never produce, and the one that finds a rule
/// recursing once per level.
///
/// Each is the text around it, then what opens a level, what sits in the
/// innermost one, and what closes a level.
const NESTINGS: &[(&str, &str, &str, &str, &str)] = &[
    ("module m; assign a = ", "(", "b", ")", "; endmodule"),
    ("module m; assign a = ", "{", "b", "}", "; endmodule"),
    ("module m; assign a = ", "'{", "b", "}", "; endmodule"),
    ("module m; assign a = ", "f(", "b", ")", "; endmodule"),
    ("module m; assign a = ", "a[", "b", "]", "; endmodule"),
    ("module m; assign a = ", "-", "b", "", "; endmodule"),
    ("module m; assign a = ", "c ? b : ", "b", "", "; endmodule"),
    ("module m; assign a = ", "b + ", "b", "", "; endmodule"),
    ("module m; assign a = ", "", "b", ".c", "; endmodule"),
    ("module m; assign a = ", "", "b", "[0]", "; endmodule"),
    ("module m; assign a = ", "", "f", "()", "; endmodule"),
    (
        "module m; initial ",
        "begin ",
        "b = 1;",
        " end",
        " endmodule",
    ),
    (
        "module m; initial ",
        "fork ",
        "b = 1;",
        " join",
        " endmodule",
    ),
    ("module m; initial ", "if (a) ", "b = 1;", "", " endmodule"),
    (
        "module m; initial ",
        "if (a) b = 1; else ",
        "b = 1;",
        "",
        " endmodule",
    ),
    (
        "module m; initial ",
        "for (;;) ",
        "b = 1;",
        "",
        " endmodule",
    ),
    (
        "module m; initial ",
        "case (a) 1: ",
        "b = 1;",
        " endcase",
        " endmodule",
    ),
    ("module m; initial ", "@(a) ", "b = 1;", "", " endmodule"),
    ("", "module m; ", "", " endmodule", ""),
    ("module m; ", "if (a) ", "assign a = b;", "", " endmodule"),
    ("module m; ", "generate ", "", " endgenerate", " endmodule"),
    ("", "class c; ", "", " endclass", ""),
    (
        "module m; typedef ",
        "struct { ",
        "logic",
        " a; }",
        " t; endmodule",
    ),
    (
        "module m; assert property (",
        "not ",
        "a",
        "",
        "); endmodule",
    ),
    ("module m; assert property (", "(", "a", ")", "); endmodule"),
    (
        "module m; assert property (",
        "a |-> ",
        "a",
        "",
        "); endmodule",
    ),
    (
        "module m; assert property (",
        "a ##1 ",
        "a",
        "",
        "); endmodule",
    ),
    (
        "class c; constraint k { ",
        "if (a) ",
        "b;",
        "",
        " } endclass",
    ),
    ("class c; constraint k { ", "{ ", "b;", " }", " } endclass"),
    ("", "`ifdef A\n", "", "`endif\n", ""),
];

/// Short random token sequences.
pub fn token_sequences() -> impl Iterator<Item = (String, String)> {
    (0..cases(20_000)).map(|seed| {
        let mut rng = Rng::new(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        let len = 1 + rng.below(80);
        (format!("seed {seed}"), tokens(&mut rng, len))
    })
}

/// Token sequences long enough that a shell opens, fills and closes by
/// accident, which is what the short ones almost never do.
pub fn long_token_sequences() -> impl Iterator<Item = (String, String)> {
    (0..cases(600)).map(|seed| {
        let mut rng = Rng::new(seed.wrapping_mul(0xbf58_476d_1ce4_e5b9) ^ 0x5eed);
        let len = 2000 + rng.below(2000);
        (format!("long seed {seed}"), tokens(&mut rng, len))
    })
}

/// Random bytes.
pub fn random_bytes() -> impl Iterator<Item = (String, String)> {
    (0..cases(15_000)).map(|seed| {
        let mut rng = Rng::new(seed.wrapping_mul(0x94d0_49bb_1331_11eb) ^ 0xb17e);
        let len = 1 + rng.below(200);
        (format!("byte seed {seed}"), bytes(&mut rng, len))
    })
}

/// The empty file, and each token alone and twice over.
pub fn shortest() -> impl Iterator<Item = (String, String)> {
    let once = SPELLINGS.iter().map(|s| (s.to_string(), s.to_string()));
    let twice = SPELLINGS.iter().map(|s| (s.to_string(), s.repeat(2)));
    std::iter::once(("empty".to_string(), String::new())).chain(once.chain(twice))
}

/// Each of [`NESTINGS`], `deep` levels deep.
pub fn deep_nestings(deep: usize) -> impl Iterator<Item = (String, String)> {
    NESTINGS
        .iter()
        .map(move |&(before, open, inner, close, after)| {
            let text = [
                before,
                &open.repeat(deep),
                inner,
                &close.repeat(deep),
                after,
            ]
            .concat();
            (format!("{open:?} nested {deep} deep"), text)
        })
}

/// Corpus files cut and spliced, or `None` without a corpus.
pub fn corpus_splices() -> Option<impl Iterator<Item = (String, String)>> {
    let files = corpus::files()?;

    // Enough files for variety, few enough that the test stays a test.
    let sources: Vec<String> = files
        .iter()
        .step_by(37)
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .collect();
    assert!(!sources.is_empty(), "the corpus read as nothing");

    Some((0..cases(10_000)).map(move |seed| {
        let mut rng = Rng::new(seed.wrapping_mul(0xd6e8_feb8_6659_fd93) ^ 0x5f1c_e000);
        (format!("splice seed {seed}"), splice(&mut rng, &sources))
    }))
}
