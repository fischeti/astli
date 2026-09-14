//! Substitution: turning a reference and a definition into tokens.
//!
//! This is the expanded mode. [`scan`](super::scan) has already found every
//! reference and split its arguments, and [`MacroTable`] already knows what
//! each name means, so what is left is the substitution itself -- and the
//! provenance that makes the result diagnosable.
//!
//! # What comes out
//!
//! An [`ExpandedToken`], which is a kind and a [`TokenOrigin`] rather than a
//! kind and a byte range. A token on this path can be spelled in a body, in an
//! argument at the call site, or in a buffer no file contains, so the byte
//! range on its own is not enough to say where it is; see
//! [D9](../../../../docs/plan.md) and `svirig-text`.
//!
//! # Rescanning by recursion, not by re-lexing
//!
//! A body is contiguous text in a file, and so is an argument. So a reference
//! nested in either is delimited *in place*, against the same token slice and
//! the same table, and expanded by recursing into the range that holds it.
//! Nothing has to be re-lexed and no intermediate token stream exists.
//!
//! The cost is one case this cannot see: a call whose name comes from one piece
//! of text and whose argument list comes from another, as in `` `define A(x)
//! x(1) `` invoked as `` `A(`FOO) ``, where `` `FOO ``'s arguments would have to
//! be taken from the body. Handling it needs a heterogeneous rescan over tokens
//! from several files at once, which is a large machine for a construct the
//! corpus does not contain. Recorded in `docs/limitations.md`.
//!
//! # The two operators make text that is in no file
//!
//! ``` `` ``` fuses the tokens either side of it and `` `" `` turns a stretch
//! of body into a string literal. Neither result is spelled anywhere: the
//! bytes have to be built, and a token pointing at them needs somewhere to
//! point. That is what `Origins::add_synthesised` is for, and it is why both
//! are expansions even where no `` `define `` is involved.
//!
//! A paste is resolved against the tokens *already emitted*, not against the
//! body text, because either side may itself be a formal or a nested call:
//! `` `define REG(n) reg_``n``_q `` pastes what the argument expanded to.
//!
//! # Scope and placement are different questions
//!
//! Substituting a formal splices in text written at the *call site*, so the
//! names in it mean what they mean there -- an identifier that happens to
//! match a formal of the macro being expanded is not that formal. But the
//! tokens are *placed* by this expansion, which is what a message about them
//! has to say. [`Frame`] therefore carries the two separately: a formal
//! binding is looked up through the caller's frame, while `from` stays this
//! expansion's.

use std::ops::Range;

use svirig_text::{Expansion, ExpansionId, FileId, Origins, Span, TokenOrigin};

use super::directive::{Directive, DirectiveType, MacroDef};
use super::macros::{self, Entry, MacroRef, MacroTable, key};
use super::{Item, scan};
use crate::{SyntaxKind, SyntaxKind::*, Token};

/// A token on the expanded path: a kind, and where its bytes are.
///
/// The raw path keeps the plain [`Token`], which is a kind and a range in the
/// one file it came from. That is no longer enough once a macro expands, so
/// this carries a [`TokenOrigin`] instead -- 12 bytes more per token, on the
/// path that needs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpandedToken {
    pub kind: SyntaxKind,
    pub origin: TokenOrigin,
}

/// What one formal stands for in one call.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Bound {
    /// The tokens the call passed, which live at the call site.
    Actual(Range<u32>),
    /// The formal's default, which lives in the definition. Taken only when the
    /// call omitted the argument entirely: an argument that is present and
    /// empty is an empty argument, not an absent one (22.5.1).
    Default(Range<u32>),
    /// No argument and no default. 22.5.1 makes that an error; expanding the
    /// formal to nothing keeps the rest of the body.
    Nothing,
}

/// The text being substituted into, and what the names in it mean.
///
/// Copy, and threaded by reference rather than pushed on a stack of its own,
/// because a formal's actual argument has to be expanded in the *caller's*
/// frame and so needs to reach it.
#[derive(Debug, Clone, Copy)]
struct Frame<'f> {
    /// Each formal's name token in the definition, and what it stands for.
    /// Empty outside a macro body.
    args: &'f [(u32, Bound)],
    /// The expansion that placed this text, and so the one a token emitted
    /// from it points back at.
    from: Option<ExpansionId>,
    /// The frame the call that opened this one was written in. An actual
    /// argument is expanded in it, because the argument's tokens are the
    /// caller's text.
    caller: Option<&'f Frame<'f>>,
}

impl Frame<'static> {
    /// A file's own top level: no formals in scope, and nothing placed it.
    const FILE: Frame<'static> = Frame {
        args: &[],
        from: None,
        caller: None,
    };
}

/// Expands every macro reference in `file`, whose tokens are `tokens`.
///
/// `` `include `` is not followed and conditionals are not evaluated yet, so
/// every branch's text is expanded and the definitions from all of them are in
/// the table at once. See `docs/next.md`.
pub fn expand(origins: &mut Origins, file: FileId, tokens: &[Token]) -> Vec<ExpandedToken> {
    let items = scan(origins.text(file), tokens).items;
    let mut expander = Expander {
        tokens,
        file,
        origins,
        table: MacroTable::new(),
        out: Vec::with_capacity(tokens.len()),
        active: Vec::new(),
    };
    expander.run(&items);
    expander.out
}

struct Expander<'a> {
    tokens: &'a [Token],
    file: FileId,
    origins: &'a mut Origins,
    /// The table as it stands at the point being expanded, rebuilt as the
    /// directives go past rather than taken whole from the scan: a reference
    /// may only use a definition that precedes it.
    table: MacroTable,
    out: Vec<ExpandedToken>,
    /// The definitions currently being expanded, by the name token of each.
    ///
    /// A name already here is a macro that has reached itself, directly or
    /// through others. Identifying a definition by its own token is enough
    /// while there is one file; following an `` `include `` will make it want a
    /// file alongside, as the table's indices will.
    active: Vec<u32>,
}

impl Expander<'_> {
    fn run(&mut self, items: &[Item]) {
        let mut at = 0;
        for item in items {
            let range = item.tokens();
            self.expand_range(at..range.start, &Frame::FILE);
            match item {
                Item::Directive(directive) => self.directive(directive, &Frame::FILE),
                Item::Macro(reference) => self.reference(reference, &Frame::FILE),
            }
            at = range.end;
        }
        self.expand_range(at..self.tokens.len() as u32, &Frame::FILE);
    }

    /// Walks `range` as substitution text, expanding what it finds.
    ///
    /// Used for a file's own top level too, where the frame binds nothing and
    /// every token is simply emitted. One path rather than two is worth the
    /// empty lookups: it is the same question either way.
    fn expand_range(&mut self, range: Range<u32>, frame: &Frame) {
        let mut at = range.start;
        while at < range.end {
            at = self.step(at, range.end, frame);
        }
    }

    /// Handles the token at `at` and returns the next index. `limit` is the end
    /// of the text being walked, which nothing read here may reach past.
    fn step(&mut self, at: u32, limit: u32, frame: &Frame) -> u32 {
        let kind = self.tokens[at as usize].kind;

        // A comment in a body is not part of the substituted text (22.5.1).
        // Outside one there is no expansion to belong to, and it is ordinary
        // trivia the parser will place.
        if kind.is_trivia() && kind != WHITESPACE && frame.from.is_some() {
            return at + 1;
        }

        match kind {
            // The `\` goes and the newline stays, so the substituted text has
            // the line break the definition was written with. The exception --
            // a continuation inside a string literal, where both characters
            // survive -- needs nothing here: a string is one token, so its
            // backslash never reaches this kind.
            LINE_CONTINUATION if frame.from.is_some() => {
                let token = self.tokens[at as usize];
                self.push(
                    WHITESPACE,
                    Span::new(self.file, token.start + 1, token.end),
                    frame.from,
                );
                at + 1
            }
            // Both operators build text, so both need an expansion to hang
            // the buffer on. Outside a body there is none, and neither means
            // anything there: they are ordinary tokens the parser will reject.
            MACRO_QUOTE if frame.from.is_some() => self.stringify(at, limit, frame),
            MACRO_PASTE if frame.from.is_some() => self.paste(at, limit, frame),
            DIRECTIVE => self.directive_or_reference(at, limit, frame),
            _ => match self.bound(at, frame) {
                Some(bound) => {
                    self.substitute(bound, frame);
                    at + 1
                }
                None => {
                    self.emit(at, frame);
                    at + 1
                }
            },
        }
    }

    /// A `` `name `` inside substitution text, which may be either.
    ///
    /// A directive in a macro body is processed where the macro is used (22.2),
    /// which is here.
    fn directive_or_reference(&mut self, at: u32, limit: u32, frame: &Frame) -> u32 {
        let source = self.origins.text(self.file);
        match DirectiveType::lookup(self.tokens[at as usize].text(source)) {
            Some(name) => {
                let directive = super::directive::parse(name, source, self.tokens, at);
                let end = directive.tokens.end.min(limit).max(at + 1);
                self.directive(&directive, frame);
                end
            }
            None => {
                let reference = macros::parse(source, self.tokens, at, limit, &self.table);
                let end = reference.tokens.end;
                self.reference(&reference, frame);
                end
            }
        }
    }

    fn directive(&mut self, directive: &Directive, frame: &Frame) {
        use DirectiveType::*;

        self.table
            .apply(self.origins.text(self.file), self.tokens, directive);

        match directive.ty {
            // The two directives that are macros: they stand for a value where
            // they appear rather than instructing the preprocessor.
            FileName | LineNumber => self.builtin(directive, frame),
            // Every other directive is consumed. Nothing reaches the expanded
            // stream, which is a program and not the text that produced it.
            _ => {}
        }
    }

    /// `` `__FILE__ `` and `` `__LINE__ ``, whose text is in no file.
    ///
    /// Both answer for the *outermost* call site when they sit in a macro body,
    /// which is what the origin map already computes: a `` `__LINE__ `` in a
    /// body reports the line the macro was used on, not the line it was written
    /// on.
    fn builtin(&mut self, directive: &Directive, frame: &Frame) {
        let span = self.extent(directive.tokens.clone());
        let reported = self.origins.reported_at(TokenOrigin {
            spelled: span,
            from: frame.from,
        });

        let (kind, text) = match directive.ty {
            DirectiveType::FileName => {
                let path = self.origins.path(reported.file);
                // A span in a synthesised buffer has no path. Nothing can
                // produce one here yet, and an empty name beats a panic.
                let name = path.map(|path| path.display().to_string());
                (STRING_LITERAL, format!("\"{}\"", name.unwrap_or_default()))
            }
            _ => {
                let line = self.origins.line_col(reported.file, reported.start).line;
                (INT_LITERAL, line.to_string())
            }
        };

        let id = self.origins.expand(Expansion {
            name: span,
            call: span,
            // No `` `define `` supplied this; the implementation did.
            def: None,
            parent: frame.from,
        });
        let len = text.len() as u32;
        let file = self.origins.add_synthesised(text, id);
        self.push(kind, Span::new(file, 0, len), Some(id));
    }

    /// Expands one macro reference.
    fn reference(&mut self, reference: &MacroRef, frame: &Frame) {
        let source = self.origins.text(self.file);
        let Some(Entry { def, .. }) = self
            .table
            .get(self.tokens[reference.name as usize].text(source))
        else {
            // Undefined at the point of use, which 22.5.1 makes an error. The
            // reference's own tokens are the honest stand-in until there is a
            // diagnostics layer to say so.
            return self.emit_verbatim(reference, frame);
        };
        // Cloned so that the body can be walked while `origins` is written to.
        // One small clone per expansion, against threading the two borrows
        // through every step below.
        let def = def.clone();

        if self.active.contains(&def.name) {
            // A macro that has reached itself. Substituting again cannot
            // terminate, so the reference stands as written.
            return self.emit_verbatim(reference, frame);
        }

        let Some(bindings) = self.bind(&def, reference) else {
            return self.emit_verbatim(reference, frame);
        };

        let id = self.origins.expand(Expansion {
            name: self.span(reference.name),
            call: self.extent(reference.tokens.clone()),
            def: Some(self.extent(def.tokens.clone())),
            // The call itself may have been placed by an expansion, which is
            // what makes a macro expanding to a macro read back as a chain.
            parent: frame.from,
        });

        self.active.push(def.name);
        self.expand_range(
            def.body.clone(),
            &Frame {
                args: &bindings,
                from: Some(id),
                caller: Some(frame),
            },
        );
        self.active.pop();

        // A definition that takes no arguments, reached through a reference
        // that was given some: the arity was ambiguous and the scan guessed
        // that the parentheses were a list. They are not, so they are ordinary
        // text following the expansion.
        if def.formals.is_none() && reference.args.is_some() {
            self.expand_range(reference.name + 1..reference.tokens.end, frame);
        }
    }

    /// Pairs each formal with what the call gives it, or `None` if the call and
    /// the definition disagree about whether there is an argument list at all.
    fn bind(&self, def: &MacroDef, reference: &MacroRef) -> Option<Vec<(u32, Bound)>> {
        let formals = match (&def.formals, &reference.args) {
            (None, _) => return Some(Vec::new()),
            // The macro takes an argument list and the call has none, which
            // 22.5.1 makes an error. Defaults do not rescue it: the list is
            // what makes it a call.
            (Some(_), None) => return None,
            (Some(formals), Some(_)) => formals,
        };
        let empty = Vec::new();
        let actuals = reference.args.as_ref().unwrap_or(&empty);

        // `` `A() `` splits into one empty argument, because the list is split
        // on commas and nothing else. A macro with no formals has to read that
        // as no arguments.
        let given = match actuals.as_slice() {
            [only] if only.is_empty() && formals.is_empty() => &[][..],
            actuals => actuals,
        };

        Some(
            formals
                .iter()
                .enumerate()
                .map(|(at, formal)| {
                    let bound = match given.get(at) {
                        Some(actual) => Bound::Actual(actual.clone()),
                        // Too few arguments. 22.5.1 lets a default stand in,
                        // and makes it an error when there is none.
                        None => match &formal.default {
                            Some(default) => Bound::Default(default.clone()),
                            None => Bound::Nothing,
                        },
                    };
                    (formal.name, bound)
                })
                .collect(),
        )
        // Arguments beyond the formals are dropped. They are an error by
        // 22.5.1, and there is no formal to splice them into.
    }

    /// What the token at `at` is bound to, if it names a formal of the macro
    /// being expanded.
    ///
    /// Matched on text rather than on kind: a formal may be written as an
    /// escaped identifier, and `` `define A(input) `` names one after a
    /// keyword, which the lexer has already reclassified.
    fn bound<'f>(&self, at: u32, frame: &'f Frame) -> Option<&'f Bound> {
        if frame.args.is_empty() {
            return None;
        }
        let name = key(self.text(at));
        frame
            .args
            .iter()
            .find(|(formal, _)| key(self.text(*formal)) == name)
            .map(|(_, bound)| bound)
    }

    /// Splices in what a formal stands for.
    fn substitute(&mut self, bound: &Bound, frame: &Frame) {
        match bound {
            // The argument's tokens are the caller's text, so the names in it
            // are the caller's -- but this expansion is what placed them.
            Bound::Actual(actual) => {
                let outer = frame.caller.copied().unwrap_or(Frame::FILE);
                self.expand_range(
                    actual.clone(),
                    &Frame {
                        from: frame.from,
                        ..outer
                    },
                );
            }
            // A default is written in the definition, so it reads as body text.
            Bound::Default(default) => self.expand_range(default.clone(), frame),
            Bound::Nothing => {}
        }
    }

    /// `` `" ... `" `` -- the text between the quotes, expanded, as one string
    /// literal (22.5.1).
    fn stringify(&mut self, at: u32, limit: u32, frame: &Frame) -> u32 {
        let id = frame.from.expect("only reached inside an expansion");
        let close = (at + 1..limit)
            .find(|&at| self.tokens[at as usize].kind == MACRO_QUOTE)
            .unwrap_or(limit);

        let (_, inner) = self.aside(|expander| expander.expand_range(at + 1..close, frame));
        let text = quoted(self.origins, &inner);

        let len = text.len() as u32;
        let file = self.origins.add_synthesised(text, id);
        self.push(STRING_LITERAL, Span::new(file, 0, len), Some(id));
        // Past the closing quote, or to the end of the text if it never came.
        (close + 1).min(limit)
    }

    /// ``` `` ``` -- delete the whitespace either side and fuse the tokens that
    /// meet.
    ///
    /// The left operand comes off the output rather than out of the body, so
    /// that what a formal or a nested call expanded to is what gets fused.
    fn paste(&mut self, at: u32, limit: u32, frame: &Frame) -> u32 {
        let id = frame.from.expect("only reached inside an expansion");

        while self
            .out
            .last()
            .is_some_and(|token| token.kind == WHITESPACE)
        {
            self.out.pop();
        }
        let mut next = at + 1;
        while next < limit && self.tokens[next as usize].kind == WHITESPACE {
            next += 1;
        }

        // An operator with nothing on one side of it has nothing to fuse, which
        // 22.5.1 makes an error. Dropping it is what leaves the other side
        // intact.
        let Some(left) = self.out.pop() else {
            return next;
        };
        if next >= limit {
            self.out.push(left);
            return next;
        }

        let (end, mut right) = self.aside(|expander| expander.step(next, limit, frame));
        while right.first().is_some_and(|token| token.kind == WHITESPACE) {
            right.remove(0);
        }
        let Some(first) = right.first().copied() else {
            self.out.push(left);
            return end;
        };

        let fused = format!(
            "{}{}",
            self.origins.slice(left.origin.spelled),
            self.origins.slice(first.origin.spelled)
        );
        let file = self.origins.add_synthesised(fused, id);
        // Re-lexed, because fusing is the point: `reg_` and `q` are two
        // identifiers apart and one identifier together. Where the bytes do not
        // make a single token they make however many they make.
        for token in crate::tokenize(self.origins.text(file)) {
            if token.kind != EOF {
                self.push(
                    token.kind,
                    Span::new(file, token.start, token.end),
                    Some(id),
                );
            }
        }
        self.out.extend(right.drain(1..));
        end
    }

    /// Expands into a buffer of its own, leaving the output stream untouched.
    ///
    /// Both operators need their operands as tokens before they can be turned
    /// into bytes, which means expanding text that does not go straight to the
    /// output.
    fn aside<T>(&mut self, walk: impl FnOnce(&mut Self) -> T) -> (T, Vec<ExpandedToken>) {
        let saved = std::mem::take(&mut self.out);
        let value = walk(self);
        (value, std::mem::replace(&mut self.out, saved))
    }

    /// Emits a reference's own tokens, for the cases where there is nothing to
    /// substitute.
    fn emit_verbatim(&mut self, reference: &MacroRef, frame: &Frame) {
        for at in reference.tokens.clone() {
            self.emit(at, frame);
        }
    }

    fn emit(&mut self, at: u32, frame: &Frame) {
        let token = self.tokens[at as usize];
        self.push(
            token.kind,
            Span::new(self.file, token.start, token.end),
            frame.from,
        );
    }

    fn push(&mut self, kind: SyntaxKind, spelled: Span, from: Option<ExpansionId>) {
        self.out.push(ExpandedToken {
            kind,
            origin: TokenOrigin { spelled, from },
        });
    }

    fn text(&self, at: u32) -> &str {
        self.tokens[at as usize].text(self.origins.text(self.file))
    }

    fn span(&self, at: u32) -> Span {
        let token = self.tokens[at as usize];
        Span::new(self.file, token.start, token.end)
    }

    /// The bytes a token range covers, whatever lies between the tokens
    /// included.
    fn extent(&self, range: Range<u32>) -> Span {
        let start = self.tokens[range.start as usize].start;
        let end = self.tokens[range.end.max(range.start + 1) as usize - 1].end;
        Span::new(self.file, start, end)
    }
}

/// Renders an expanded stream back to text.
///
/// For comparing against another preprocessor, and for an eventual `-E`. It is
/// not a formatter and not a round-trip: a token a macro placed brings no
/// whitespace with it, so the only separation written here is the separation
/// the text cannot do without.
pub fn render(origins: &Origins, tokens: &[ExpandedToken]) -> String {
    let mut out = String::new();
    for (gap, token) in pieces(origins, tokens) {
        out.push_str(gap);
        out.push_str(origins.slice(token.origin.spelled));
    }
    out
}

/// A stream as the contents of one string literal, for `` `" ``.
fn quoted(origins: &Origins, tokens: &[ExpandedToken]) -> String {
    let mut out = String::from("\"");
    for (gap, token) in pieces(origins, tokens) {
        // A string literal holds no raw newline, and a gap only ever exists to
        // keep two tokens apart.
        out.push_str(if gap.is_empty() { "" } else { " " });
        match token.kind {
            // `` `\`" `` is how a body writes a quote that survives into the
            // string rather than ending it.
            MACRO_ESCAPED_QUOTE => out.push_str("\\\""),
            _ => escape(origins.slice(token.origin.spelled), &mut out),
        }
    }
    out.push('"');
    out
}

fn escape(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '"' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => {}
            _ => out.push(ch),
        }
    }
}

/// Each token of a stream, with whatever separation has to precede it.
///
/// Shared so that the plain rendering and the stringified one cannot drift
/// apart on the question of when two tokens need keeping apart.
fn pieces<'a>(
    origins: &'a Origins,
    tokens: &'a [ExpandedToken],
) -> impl Iterator<Item = (&'static str, &'a ExpandedToken)> {
    let mut previous: Option<Span> = None;

    tokens
        .iter()
        .filter(|token| token.kind != EOF)
        .map(move |token| {
            let span = token.origin.spelled;
            let gap = match previous {
                // Adjacent in the same buffer means the source already
                // separated them however it wanted to, and nothing may be
                // added.
                Some(last) if last.file == span.file && last.end == span.start => "",
                Some(last) => separator(origins.slice(last), origins.slice(span)),
                None => "",
            };
            previous = Some(span);
            (gap, token)
        })
}

/// What has to go between two tokens that were not written next to each other.
///
/// Substitution puts tokens side by side that the source never did, and two of
/// them run together may lex as a third thing -- `1` and `2` as `12`, `+` and
/// `+` as `++`. So the question is not one of style: write the least that
/// makes the pair lex back as the pair.
fn separator(left: &str, right: &str) -> &'static str {
    // Whitespace on either side is separation already. Two runs of it do lex
    // as one, but merging them is what separating means rather than something
    // to prevent.
    let separated = left.ends_with(char::is_whitespace)
        || right.starts_with(char::is_whitespace)
        || !pastes(left, right);

    if separated {
        ""
    } else if !pastes(&format!("{left} "), right) {
        " "
    } else {
        // A `//` comment eats whatever follows it on the line, so a space
        // cannot separate one from what comes next.
        "\n"
    }
}

/// Whether `right` written directly after `left` changes what `left` lexes as.
fn pastes(left: &str, right: &str) -> bool {
    let joined = format!("{left}{right}");
    // `tokenize` always yields at least an `EOF`, so the first token exists.
    crate::tokenize(&joined)[0].end as usize != left.len()
}
