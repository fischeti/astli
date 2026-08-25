# Limitations

Deliberate gaps: things the standard requires that we knowingly do not do.
Each entry says what would justify closing it, so that a decision taken once
with good reasons does not quietly become a decision nobody remembers taking.

This is not a bug list and not a to-do list. Work that is merely *not done yet*
belongs in [`grammar-coverage.md`](grammar-coverage.md); work that is broken
belongs in the issue tracker. What lives here is the third thing — conformance
we chose to trade away.

**Prefer observable to documented.** Where a limitation can announce itself at
runtime — a warning, a diagnostic, a refusal — do that as well as writing it
down here. A limitation nobody trips over is one nobody remembers.

---

### Only the IEEE 1800-2023 keyword set

`` `begin_keywords `` / `` `end_keywords `` select which revision's reserved
words apply, and under `` `begin_keywords "1364-1995" `` names like `logic`,
`bit`, and `class` are ordinary identifiers. We implement the 1800-2023 set
only and ignore the directive's effect. The directive itself still lexes and
round-trips.

Nine keyword tables is a lot of transcription for a directive that appears zero
times in the corpus and, as far as I know, zero times in practice.

**Revisit when** a `` `begin_keywords `` shows up in real input, or when
someone reports a file where a reserved word is being rejected as a name. The
lookup already takes a `KeywordVersion`, so closing this is adding a table, not
restructuring anything.

**Where** `crates/svirig-syntax/src/keyword.rs`

---

### Token kinds are not checked against an external oracle

The lexer is verified by targeted tests and by a round-trip over the corpus.
Round-trip proves every byte lands in exactly one token; it does not prove the
*kind* is right. A token classified wrongly but sliced correctly passes
everything we run today.

The alternative was a differential test against another implementation's lexer,
which would prove kinds properly. It was judged not worth the detour at this
stage: the parser will start rejecting miss-classified tokens soon enough, and
that is a real oracle arriving for free.

**Revisit when** the parser lands and either (a) it turns out to be a weak
oracle in practice, or (b) a miss-kinded token survives into formatter output.

**Where** `crates/svirig-syntax/tests/lexer.rs`

---

### No lexer modes

Two constructs are lexed as ordinary SystemVerilog when the standard says they
have their own lexical rules:

- UDP `table` / `endtable` bodies, whose entries are symbol sequences, not
  expressions.
- `` `pragma protect `` encrypted envelopes, whose payload is not source at all.

Both lex into whatever the ordinary rules make of them. Bytes are preserved, so
the round-trip holds and the formatter cannot corrupt them by accident, but the
token kinds inside are meaningless.

The corpus has zero UDP tables and one file with a protect envelope, so both
are speculative, and each would cost a second token enum to serve.

`` `define `` bodies are the third construct with their own lexical rules, and
they are lexed rather than held as text — nearly all of a body lexes the same
either way, and expansion substitutes into the tokens. The one rule that really
does differ is applied. What is left of that gap belongs to
[triple-quoted strings](#triple-quoted-string-literals-are-not-lexed).

**Revisit when** a real input contains a UDP table or a protect envelope.

**Where** `crates/svirig-syntax/src/kind.rs`

---

### A macro reference's arguments are guessed when its arity is unknown

Raw mode never follows an `` `include `` (decision D6), so 95% of macro
references in the corpus have no definition in the file that uses them. With no
definition to consult, a `(` anywhere on the rest of the line is read as an
argument list.

That is right 256 times and wrong 12 times over the corpus commits pinned in
[`preprocessor.md`](preprocessor.md#level-b--macro-invocations-are-grammar-atoms).
All 12 are `` `WITH (!expr) `` in one file: `` `define WITH iff `` is a nullary
stand-in for a keyword, and the parentheses are the expression's own. The
alternative rule, requiring the `(` to touch the name, is wrong twenty times
more often and wrong in the more expensive direction — a spurious argument list
still reproduces its own bytes, while a missed one leaves a parenthesised
expression in item position, where the parser can only fall back to verbatim.

Nothing here can be right in every case: the same bytes mean two different
things and only the definition separates them.

**Revisit when** the *expanded* mode exists, where the includes have been
followed and the arity is never in doubt — the guess belongs to raw mode alone
and must not leak into it. Also worth revisiting if a filelist or `bender`
integration ever hands the formatter an include path it could use for arity
without following the includes into the tree.

**Where** `crates/svirig-syntax/src/preproc/macros.rs`

---

### A directive with a defined end is given the whole line

1800-2023 22.2 lets ordinary code follow a directive on the same line, once
that directive's syntax has reached its end. `` `timescale ``, `` `line ``,
`` `default_nettype ``, `` `unconnected_drive ``, `` `nounconnected_drive `` and
`` `begin_keywords `` are reported with the rest of their line as operands
regardless, so `` `default_nettype none logic x; `` would have `logic x;`
counted as part of the directive.

The alternative is a small shape parser for each of the six, and none of them
has a reader: their operands are kept as tokens precisely because nothing
consumes them yet. Between them they occur 16 times in the corpus, never with
code following on the line.

**Revisit when** something reads those operands, which is also when the shape
has to be parsed anyway.

**Where** `crates/svirig-syntax/src/preproc/directive.rs`

---

### Triple-quoted string literals are not lexed

IEEE 1800-2023 added `"""…"""` strings, which may contain unescaped quotes and
newlines. Only the ordinary form is recognised, so a triple-quoted string lexes
as an empty string followed by whatever its contents look like.

Bytes survive — the round-trip still holds — but the kinds inside are wrong,
and a formatter could reflow something it should not touch.

There is a second consequence inside a `` `define ``. A newline ends the macro
text unless it sits in a block comment or a triple-quoted string (22.5.1). The
lexer honours the block comment, because one is a single token; it cannot
honour the string it does not recognise, so a triple-quoted string opened in a
macro body would end the definition on its first newline.

**Revisit when** one turns up, which requires a toolchain that has adopted
1800-2023. Zero occurrences in the corpus.

**Where** `crates/svirig-syntax/src/kind.rs`

---

### An escaped identifier must be followed by whitespace

`\foo` at the very end of a file, with no trailing whitespace or newline, does
not lex as an identifier. The standard says an escaped identifier is terminated
by whitespace, and the terminator is kept inside the token so that a formatter
cannot delete it.

A file ending that way is malformed anyway, so this is a limitation in the
sense that the error message will be poor rather than that valid input is
rejected.

**Revisit when** the diagnostics layer exists and can say something better than
"unexpected byte".

**Where** `crates/svirig-syntax/src/kind.rs`
