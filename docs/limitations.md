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

**A caller that knows the build can now say so.** `Raw::seeded` and
`parse_seeded` take a table to start from, and `svirig parse` fills one by
expanding the file first and keeping only what that expansion ended with --
the tokens are thrown away, so the tree is still the file as written. Given
`-I`, `` `WITH (!expr) `` comes back as the name alone with the parentheses
left to the expression. What follows is what is still guessed at: a run with no
build behind it, which is every run that has only a path.

That is right 256 times and wrong 12 times over the corpus commits pinned in
[`preprocessor.md`](preprocessor.md#level-b--macro-invocations-are-grammar-atoms).
All 12 are `` `WITH (!expr) `` in one file: `` `define WITH iff `` is a nullary
stand-in for a keyword, and the parentheses are the expression's own. The
alternative rule, requiring the `(` to touch the name, is wrong twenty times
more often and wrong in the more expensive direction — a spurious argument list
still reproduces its own bytes, while a missed one leaves a parenthesised
expression in item position, where the parser can only fall back to verbatim.

Nothing here can be right in every case: the same bytes mean two different
things and only the definition separates them. A wrong guess is now a
`MACRO_ARG_LIST` node over a parenthesised expression that is nobody's
argument -- the wrong tree over the right bytes, which is exactly the cost the
rule was chosen to keep cheap.

**The expanded mode has largely escaped it.** Expansion walks a file against a
table built across every `` `include `` it has followed and holding only the
definitions the conditionals actually selected, so a macro is known by arity
where it is used and the fallback never runs. What is left there is a header
that was not found — no include path, or a name that only a build system knows
— where the definition is missing and the guess is all there is, exactly as in
raw mode.

**Revisit when** the formatter reaches the point of needing this, since it is
the one caller that will not have been handed a build on a command line. What
it wants is a `bender` manifest or a filelist read for it, which is the same
answer as for the expanded path and is already half-built: the driver resolves
one, and the seeding is a call.

**Where** `crates/svirig-preproc/src/macros.rs`,
`crates/svirig-preproc/src/expand.rs`, `crates/svirig-parse/src/source.rs`

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

Since the parser builds a `DIRECTIVE` node over the extent, the over-claim is
now visible in the tree: the code that followed would be *inside* the node.
Bytes still round-trip, and a formatter that emits a directive's line as
written would reproduce it, so what is lost is structure rather than text.

**Revisit when** something reads those operands, which is also when the shape
has to be parsed anyway.

**Where** `crates/svirig-preproc/src/directive.rs`

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

**Where** `crates/svirig-preproc/src/expand.rs`

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

**Where** `crates/svirig-preproc/src/expand.rs`

---

### The verbatim fallback guesses which keywords open a body

A run of tokens the parser cannot make sense of is kept balanced by a
delimiter stack, so that a `;` inside a `begin` … `end` does not end it. That
needs to know what opens a body, and five keywords only sometimes do:

| written | opens a body | does not |
| --- | --- | --- |
| `function` | `function f(); … endfunction` | `extern function f();`, `pure virtual function f();`, `import "DPI-C" function f();` |
| `class` | `class C; … endclass` | `typedef class C;` |
| `interface` | `interface i; … endinterface` | `virtual interface i vif;`, `interface class C; … endclass` |
| `property` | `property p; … endproperty` | `assert property (…);` |
| `sequence` | `sequence s; … endsequence` | `expect (s);` |

Each is decided by a test on the tokens around it -- whether `extern`, `pure`,
`import` or `typedef` has been seen since the last `;`, whether the next token
is an identifier or a `(`, whether the previous one was `virtual`. They are
heuristics. Deciding properly means knowing whether a declaration has a body,
which is the thing the parser was unable to work out in the first place.

**What a wrong guess costs is bounded, which is why they are acceptable.** The
stack holds what opened rather than a count, and a closer that does not match
the top of it ends the run instead of being swallowed. So a `function` pushed
wrongly does not eat the rest of the file: it eats until the `endclass` or
`endmodule` of whatever encloses it, which stops the run and resyncs. One
construct is formatted verbatim that need not have been.

**Revisit when** the grammar can parse the constructs themselves, at which
point the fallback runs on less and less, or when a corpus file is found whose
verbatim rate is much higher than its neighbours' -- which is what a wrong
guess looks like from the outside.

**Where** `crates/svirig-parse/src/verbatim.rs`

---

### A type is decided by shape, and never resolved

`foo bar;` is a declaration if and only if `foo` names a type, and nothing in
the token stream says whether it does. Deciding properly is name resolution --
following every `` `include `` and every `import` -- which a formatter does not
do ([D6](plan.md#4-decisions)).

So the parser does not decide it. It asks what shape the tokens are in
instead: a name, whatever parameters and packed dimensions qualify it, and then
a second name is a declaration, because the language is not written that way
anywhere else. An instantiation carries its port parentheses even when it
connects nothing, an expression statement is one name, and no item or
statement puts a bare name in front of another -- so the `(` after the second
name is the whole of what separates `my_module inst (…)` from `unknown_t x;`,
and the first name never has to be resolved at all.

What that costs is a diagnostic nobody is asking for yet. `nonexistent_t x;`
parses as a declaration of a type that does not exist, and the parser says
nothing; a compiler would have to resolve the name and reject it. It also
means the tree cannot say what a name *is* -- `foo bar (…)` is an
`INSTANTIATION` whether `foo` is a module, an interface, a program or a
primitive, which is the right answer for a formatter and not enough for
anything that has to elaborate.

The set of names a file gave to a type, gathered from its own `typedef`s
before the parse, is what used to answer this. It was deleted when the shape
rule subsumed it: every question it could settle, shape settles without
knowing anything, and it answered *no* for every type that came from a package
-- which is most of them.

**Revisit when** something downstream needs to know what a name means rather
than what shape it is in. That is name resolution, and it wants a compilation
unit rather than a file: the same `bender` integration the rest of these
entries wait on, and then `svirig-hir`.

**Where** `crates/svirig-parse/src/decl.rs`

---

### Six constructs are left to the fallback on purpose

Concurrent assertions (`assert`/`assume`/`cover property`, `sequence`),
`specify` sections, `covergroup` bodies, `clocking` blocks, `bind` statements
and the inside of a `constraint` have no rules. Each parses to a `VERBATIM`
run, and between them they are **205,062 of the 383,597 tokens** the fallback
still takes at the end of M3 -- 53% of what is left, and the reason the rest
of the number is as small as it is.

That is the bargain [D3](plan.md#the-verbatim-fallback) struck, taken
deliberately rather than by omission. All six are large -- a constraint body is
a language of its own, and `specify` has its own timing-check grammar -- and
all six are rare in RTL, which is what the formatter is for. A formatter that
reproduces them exactly as written is doing the right thing for them until
someone says otherwise, and doing it at no cost.

A `constraint` is the one that gets a shell anyway, and for a reason that has
nothing to do with its grammar: its body is braced rather than terminated by a
`;`, and the fallback reads a closing bracket as no boundary at all -- so
without a rule the run carried past the `}` and swallowed the member after it.
The shell bounds the damage; the body is still verbatim.

**Revisit when** someone formats verification code in anger, or when the
fallback rate stops falling for any other reason. Each is a self-contained
rule; none of them needs anything the parser does not already have.

**Where** `crates/svirig-parse/src/item.rs`

---

### A parenthesised header is taken whole when the expression rule stops early

`if`, `while`, `repeat`, `case`, `foreach`, `@( … )` and a `for`'s three
clauses all read a parenthesised header, and where the rule inside it stops
before the `)` -- a `foreach`'s index list, a sensitivity list with something
unusual in it -- whatever is left is taken as plain tokens so that the
`PAREN_EXPR` covers its own parentheses. The alternative is a node that ends in
the middle of a header, and a fallback starting inside one.

**This is the one place the metric could flatter itself**, because those tokens
are not inside a `VERBATIM` and are not understood either. So it is measured:
**21,161 of 7,009,174 tokens**, 0.3%, which is small enough that the rate means
what it says and large enough to be worth writing down. The same shape is used
by `decl.rs`'s `dimension`, and for the same reason.

**Revisit when** the number grows, which would mean a header shape nothing
reads rather than a scattering of odd ones. Re-measure by counting a node's
direct token children against the punctuation it is expected to carry itself.

**Where** `crates/svirig-parse/src/stmt.rs`

---

### A conditional branch is classified by eight delimiter pairs, not thirteen

A region whose every branch closes what it opens is read branch by branch in
the enclosing context; a ragged one is not. What counts as opening something is
brackets, `begin`, `case`, `fork`, `module` and `generate` -- and not
`function`, `class`, `interface`, `property` or `sequence`, which are exactly
the five the [fallback has to guess about](#the-verbatim-fallback-guesses-which-keywords-open-a-body)
and for exactly the same reason: each has a prototype form with no closer at
all, so counting them would call a branch ragged for writing `extern function
void f();`.

So a region that opens a `function` in one branch and closes it in another is
called self-delimiting and its branches are parsed. What that costs is bounded
by the same two things that bound every other wrong guess here: a branch body
is parsed against a `Position` bound, so nothing inside it can run past the
`` `endif ``, and a rule that does not find its own `end…` gives every token
back. Over the corpus, **1,571 of 1,660 regions** classify live.

**Revisit when** a region turns up that hands a `function` or a `class` across
a branch, which would show as a construct formatted verbatim for no visible
reason. Closing it means the same shape test the fallback uses, which is a
heuristic wherever it is written.

**Where** `crates/svirig-parse/src/source.rs`

---

### An assignment is not an expression

A.8.3 admits `( operator_assignment )` as a primary, so `(a = b)` and
`(x += 1)` are legal expressions whose value is what was assigned. The
expression rule does not have `=` or any of its compound forms in its
precedence table, so it stops at the `=` and hands the rest back.

The reason is what including it would cost everywhere else. `=` sits at the
bottom of Table 11-2, so an expression rule that knows it swallows the
right-hand side of every assignment *statement* as well -- and then the
statement rule, which is the thing that actually wants to see the `=`, has to
be written against an expression parser that has already taken it. Excluding
it costs one parenthesised form; including it complicates every caller.

The corpus has zero of them. What the grep finds instead is 360 matches that
are something else: `for (i = 0; …)` initialisers, which belong to the
statement rule and get their `=` there; parameter defaults in port lists;
`(FLUSH => …)` case items; and the `|=>` assertion operator, which is one
token.

**Revisit when** a real input parenthesises an assignment, which would show up
as a verbatim run around an otherwise ordinary expression.

The statement rule is the other half of this, and it confirms the trade. It
parses a left-hand side as an *lvalue* -- a primary and its postfixes, which is
what A.8.5 admits there -- rather than as an expression, because `<=` is the
nonblocking assignment and the relational operator at once, and an expression
rule would take the whole line as a comparison.

**Where** `crates/svirig-parse/src/expr.rs`,
`crates/svirig-parse/src/stmt.rs`

---

### An `` `include `` cycle is caught by path, not by identity

Following a name that would re-enter a file already open above it does nothing,
which is what stops a cycle. "The same file" means the same path with `.` and
`..` resolved textually, so `dir/../defs.svh` and `defs.svh` are one file and a
symlink, a hard link, or a second mount of the same tree are two.

Canonicalising instead means asking the filesystem, and reading is deliberately
behind the `Reader` trait so that the preprocessor can be tested without one
and an editor can answer out of its unsaved buffers. A path that is real enough
to canonicalise is an assumption neither of those can make.

The depth limit of 200 is the backstop, and the only thing that catches a cycle
the path check cannot see. 1800-2023 22.4 requires at least 15 levels, so the
limit is two orders of magnitude above anything legitimate.

**Revisit when** a real tree loops through a symlink, or when something needs
the identity of a file for another reason — at which point the trait grows a
second method and this closes with it.

**Where** `crates/svirig-text/src/origins.rs`, and the search that feeds it in
`crates/svirig-preproc/src/include.rs`

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

**Where** `crates/svirig-preproc/src/directive.rs`

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

**Where** `crates/svirig-preproc/src/conditional.rs`

---

### A build reaches raw mode through the driver, not through the session

A build passes `+define+SYNTHESIS` or `-DFPV_ON`, and nearly every conditional
in the corpus is written against names that arrive that way. Both modes can now
be told. Expanded mode takes a seeded table through `Session::expand_span`, and
raw mode takes one through `Raw::seeded`, which changes no token it emits and
only tells it which `` `name `` takes an argument list.

What is awkward is the route. `Session` carries an `Includes` and no
definitions, so the driver builds the table itself — `-D` lexed into a
`<command-line>` buffer, and for an include path a whole expansion run for the
table it ends with and nothing else. That works and is what `svirig parse`
does, but it means every caller wanting an arity pays for an expansion and
writes the same three steps. `docs/api.md` has the `Build` on the session that
replaces both halves, at which point seeding is a property of the session
rather than a thing each caller assembles.

`svirig lex` still says an include path and a set of definitions are unused,
and that one is permanent: what the bytes are is not a question a definition
answers.

It costs the oracle as well as the tool. The reference declines 3630 corpus
files for want of definitions and include paths it has not been told about, and
reaching them means telling *both* sides what a build passes — which our side
now is, and the comparison's is not.

**Revisit when** a second caller wants a seeded raw parse, or when widening the
differential past those 3630 files is worth the run.

**Where** `crates/svirig-preproc/src/session.rs`, `crates/svirig/src/session.rs`,
`crates/svirig/src/cmd/parse.rs`

---

### A filelist carries four things, and real ones carry more

`-f` and `-F` read source paths, `+incdir+`, `+define+` and a nested filelist,
with `//` and `/* */` comments and `$VAR` substitution. That is what a
generated filelist — `bender script flist`, or a flow's own — actually
contains.

What it does not read is the library half of the format: `-y` for a library
directory, `-v` for a library file, `+libext+` for the extensions to try. Those
name modules to be found *by name* rather than files to be read, which is
elaboration's question and not something anything here can answer yet. A path
containing whitespace has no spelling either; the format has no quoting rule
that the tools agree on.

Each of those is **rejected by name**, with the filelist and the line, rather
than skipped. A filelist that half works otherwise produces a build quietly
missing half its inputs, and the failure surfaces as a parse error somewhere
else entirely.

**Revisit when** something can resolve a module name to a file, which is the
only way `-y` means anything, or when a real filelist arrives that needs one of
them enough to justify recording it and ignoring it.

**Where** `crates/svirig/src/filelist.rs`

---

### Mixing `-D` and `+define+` on one command line ignores their order

`preprocess` takes both spellings of each build option: `-I`/`-D` as a compiler
spells them, `+incdir+a+b`/`+define+A=1+B` as a simulator does and as a
filelist may carry, so a command line pasted out of one works.

They resolve in three layers, each the last word over the one before: what a
filelist carried, then the plus-separated flags, then `-I` and `-D`. The first
boundary is deliberate — the command line is the override. The second is a
**rule standing in for an answer nobody can give**: the argument parser reports
each option's values without saying where in argv they fell, so
`-D A=1 +define+A=2` cannot be told from `+define+A=2 -D A=1`. Picking per
invocation is impossible, so the dash form is always the later one and the two
orders mean the same thing.

It only bites when one command line spells the *same name* both ways, which is
a thing to do by accident rather than on purpose. Include directories are
unaffected in practice: the layering fixes their search order, and a directory
named twice is searched twice to no effect.

A word starting with `+` that matches neither sigil is rejected rather than
read as a file, for the reason `unknown_flags = "error"` is set on the dash
side. A file genuinely named that way is still reachable as `./+name`.

**Revisit when** the parser can report the argv position an option's values
came from, which is the only thing that would let one command line's two
spellings interleave.

**Where** `crates/svirig/src/sources.rs`, `crates/svirig/src/cli.rs`

---

### A parallel run holds a wave of files in memory

Files are read in parallel but printed in the order they were named, so a
file's output waits for the ones before it. Holding the whole run would mean
holding every tree dump in the filelist at once, so a wave is read, written,
and the next started: what is held is bounded by the wave.

How long the wave can be is a question about memory, and it is also what
decides the speedup, since every wave ends on its slowest file. So it is sized
from what the last wave held against a 64 MB budget, between one file per
thread and sixty-four. A quiet run holds nothing and stays at the ceiling; a
tree dump of the corpus settles lower and peaks at 116 MB resident against the
21 MB the same dump takes sequentially.

The budget is a number picked to be generous rather than measured, and there
is no accounting for the trees themselves — a file's session and tree are
alive while it is being rendered, which is a per-thread cost the wave does not
see. Both are fine while the unit is a file of a few hundred kilobytes.

The alternative, streaming each file the moment it finishes, gives up the
order. A run over a filelist is compared against the last one, and output that
reorders itself under load cannot be diffed.

**Revisit when** a run has to hold something that is not a file's printed
output — a formatter's rewritten buffers would be the first — or when the
budget is reached by a corpus rather than by a dump nobody reads.

**Where** `crates/svirig/src/cmd/mod.rs`
