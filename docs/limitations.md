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
stage: the parser will start rejecting mis-classified tokens soon enough, and
that is a real oracle arriving for free.

**Revisit when** the parser lands and either (a) it turns out to be a weak
oracle in practice, or (b) a mis-kinded token survives into formatter output.

**Where** `crates/svirig-syntax/tests/lexer.rs`

---

### No lexer modes

Several constructs are lexed as ordinary SystemVerilog when the standard says
they have their own lexical rules:

- UDP `table` / `endtable` bodies, whose entries are symbol sequences, not
  expressions.
- `` `pragma protect `` encrypted envelopes, whose payload is not source at all.
- `` `define `` bodies, which are substitution *text*.

All three currently lex into whatever the ordinary rules make of them. Bytes
are preserved, so the round-trip holds and the formatter cannot corrupt them by
accident, but the token kinds inside are meaningless.

The corpus has zero UDP tables and one file with a protect envelope, so the
first two are speculative. `` `define `` bodies are not — 134 files — but they
are the preprocessor's problem rather than the lexer's.

**Revisit when** the preprocessor lands (`` `define `` bodies, unavoidably), or
when a real input contains a UDP table or a protect envelope.

**Where** `crates/svirig-syntax/src/kind.rs`

---

### Triple-quoted string literals are not lexed

IEEE 1800-2023 added `"""…"""` strings, which may contain unescaped quotes and
newlines. Only the ordinary form is recognised, so a triple-quoted string lexes
as an empty string followed by whatever its contents look like.

Bytes survive — the round-trip still holds — but the kinds inside are wrong,
and a formatter could reflow something it should not touch.

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
