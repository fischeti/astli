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

### The `line` directive does not move the line numbers we report

1800-2023 22.7 lets `` `line `` reset the line number and file name reported
from that point on, so that generated code can point at whatever produced it.
`Origins::line_col` counts newlines from the top of the buffer and ignores the
directive entirely.

Generated SystemVerilog is common in this corpus -- `iDMA` and the `opentitan`
autogen trees are both templated -- but none of it emits `` `line ``, which
appears zero times across all nine repositories. The operands are recognised and
kept as tokens, so nothing is lost but the effect. `` `__LINE__ `` and
`` `__FILE__ `` do occur, 7 and 5 times, and they expand against these same
numbers.

**Revisit when** a real input contains one, or when the diagnostics layer exists
and would otherwise point a user at the wrong line of a generated file. Closing
it means a sorted list of directives per buffer and a lookup in `line_col`,
which is the shape the state of the art uses too.

**Where** `crates/svirig-text/src/origins.rs`

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

**The expanded mode has largely escaped it.** Expansion walks a file against a
table built across every `` `include `` it has followed and holding only the
definitions the conditionals actually selected, so a macro is known by arity
where it is used and the fallback never runs. What is left there is a header
that was not found — no include path, or a name that only a build system knows
— where the definition is missing and the guess is all there is, exactly as in
raw mode.

**Revisit when** a filelist or `bender` integration hands the formatter an
include path. That closes the rest of the expanded path outright, and would
also let raw mode know an arity without following the includes into the tree.

**Where** `crates/svirig-syntax/src/preproc/macros.rs`,
`crates/svirig-syntax/src/preproc/expand.rs`

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

---

### A macro call cannot be assembled out of two pieces of text

Expansion rescans by recursion: a body and an argument are each contiguous text
in a file, so a reference nested in either is delimited in place and expanded by
recursing into the range that holds it. Nothing is re-lexed and no intermediate
stream exists, which is what keeps every token's spelling a real location.

The boundary that buys is a call whose name comes from one piece of text and
whose argument list comes from another. `` `define A(x) x(1) `` invoked as
`` `A(`FOO) `` puts `` `FOO `` in the argument and `(1)` in the body, and a
preprocessor that rescanned a flat stream would hand the one to the other. We
expand `` `FOO `` as the nullary reference it looks like where it is written,
and leave `(1)` as body text.

Closing it means a rescan over a heterogeneous token stream — tokens from
several files and synthesised buffers at once — which is a different and much
larger machine. The corpus contains no call built this way.

**Revisit when** one turns up, or when `` `include `` resolution makes a header
somewhere rely on it.

**Where** `crates/svirig-syntax/src/preproc/expand.rs`

---

### The expanded path reports no errors, only recoveries

1800-2023 22.5.1 makes several things errors that expansion here simply
recovers from, because there is no diagnostics layer for it to report to:

| What | What happens instead |
| --- | --- |
| A reference to a name with no definition | The reference's own tokens stand |
| A call missing the argument list its macro requires | The reference's own tokens stand |
| More arguments than the macro has formals | The extras are dropped |
| A formal with neither an argument nor a default | It expands to nothing |
| A macro that reaches itself | The reference stands as written |
| `` `" `` that is never closed | The text to the end of the body is quoted |
| ``` `` ``` with nothing on one side | The operator is dropped |
| An `` `include `` whose name resolves to nothing | The directive expands to nothing |
| An `` `include `` that would re-enter a file already open | The same |
| An `` `include `` chain past 200 levels | The same |
| A conditional with no name to test | The branch is never taken |
| A conditional region with no `` `endif `` | It runs to the end of its text |
| An `` `endif `` or `` `else `` with nothing above it | Consumed, like any other directive |

Each recovery is chosen to keep the tokens around it rather than to guess at
intent, so nothing is silently *wrong* — but nothing says so either, and an
undefined macro is exactly the mistake a user most wants told about.

Note that the first two are errors only on *this* path. In raw mode a reference
with no definition in its own file is the common case, not a mistake.

**Revisit when** the diagnostics layer exists. `Origins::trace` and
`Origins::reported_at` already carry what a message needs; what each of these
becomes is a diagnostic, not a change to the recovery.

**Where** `crates/svirig-syntax/src/preproc/expand.rs`

---

### An `` `include `` cycle is caught by path, not by identity

Following a name that would re-enter a file already open above it does nothing,
which is what stops a cycle. "The same file" means the same path with `.` and
`..` resolved textually, so `dir/../defs.svh` and `defs.svh` are one file and a
symlink, a hard link, or a second mount of the same tree are two.

Canonicalising instead means asking the filesystem, and reading is deliberately
behind a trait so that the preprocessor can be tested without one and an editor
can answer out of its unsaved buffers. A path that is real enough to
canonicalise is an assumption neither of those can make.

The depth limit of 200 is the backstop, and the only thing that catches a cycle
the path check cannot see. 1800-2023 22.4 requires at least 15 levels, so the
limit is two orders of magnitude above anything legitimate.

**Revisit when** a real tree loops through a symlink, or when something needs
the identity of a file for another reason — at which point the trait grows a
second method and this closes with it.

**Where** `crates/svirig-syntax/src/preproc/include.rs`

---

### An `` `include `` name that expands is read as one token

22.4 allows only a quoted or an angled literal. Anything else is kept for
expansion, which is how a macro stands in for the name and how a formal does
inside a macro body — the corpus has `` `define include_file(f) `include `"f`" ``.

That operand is taken as **one token**, except for a stringification, which
runs to its closing quote. So `` `include `PATH(a, b) `` is read as `` `PATH ``
and its argument list is left behind. Delimiting the list needs the macro
table, which is not available where directives are parsed, and no include in
the corpus is written that way.

Reading to the end of the line instead is what the previous rule did, and it is
worse: a macro body that puts an `` `include `` between an `` `ifdef `` and an
`` `endif `` — the ordinary way to write a conditional include — would take the
`` `endif `` for part of the file name.

**Revisit when** an include names a macro that takes arguments.

**Where** `crates/svirig-syntax/src/preproc/directive.rs`

---

### A conditional region does not cross a file boundary

An `` `ifdef `` in one file and an `` `endif `` in a file it includes do not
pair. Each region is read within the one stretch of text it opens in: an
unclosed region runs to the end of that text, and an `` `endif `` with nothing
above it is consumed like any other directive.

Pairing them is not obviously even coherent. The `` `include `` that would join
the two sits *inside* the region, so whether it is followed at all is the
question the region was supposed to answer — and that answer would have to be
known before the `` `endif `` could be found. Any reading here is a choice; this
one keeps a region inside text a reader can see it in.

The same boundary applies to a macro body, and there it is not a limitation but
the design: a body is substitution text, so the region in
`` `define GUARD(x) `ifdef E x `endif `` is the *body's*, evaluated wherever the
macro is used.

Zero corpus files have an unpaired conditional directive of either kind.

**Revisit when** real input pairs one across an include, or when the
diagnostics layer exists and should say something about the unpaired ones
rather than swallowing them.

**Where** `crates/svirig-syntax/src/preproc/conditional.rs`

---

### Nothing is predefined, and nothing can be

A build passes `+define+SYNTHESIS` or `-DFPV_ON`, and nearly every conditional
in the corpus is written against names that arrive that way. The preprocessor
starts with an empty table, so every such name reads as undefined and the
`` `else `` branch is the one taken.

That is the right default — it is what a file means on its own — but it is not
a *choice* until something can express the other one. The gap is an argument,
not a design: `expand` takes the include path already, and a table seeded with
command-line definitions is the same shape.

It costs the oracle as well as the tool. The reference declines 3630 corpus
files for want of definitions and include paths it has not been told about, and
reaching them means telling *both* sides what a build actually passes.

**Revisit when** the driver lands, which is what knows a filelist or a `bender`
manifest. That closes this and widens the differential in one move.

**Where** `crates/svirig-syntax/src/preproc/expand.rs`
