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

use std::path::Path;
use std::rc::Rc;

use rustc_hash::FxHashMap;
use svirig_text::{Expansion, ExpansionId, FileId, Origins, Span, TokenOrigin};

use super::conditional::{self, Branch, Taken};
use super::directive::{Directive, DirectiveType, IncludePath, MacroDef, Operands};
use super::include::{Includes, MAX_DEPTH};
use super::macros::{self, Entry, MacroRef, MacroTable, key};
use super::tokens::{Input, TokenId, TokenSpan};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bound {
    /// The tokens the call passed, which live at the call site.
    Actual(TokenSpan),
    /// The formal's default, which lives in the definition. Taken only when the
    /// call omitted the argument entirely: an argument that is present and
    /// empty is an empty argument, not an absent one (22.5.1).
    Default(TokenSpan),
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
    args: &'f [(TokenId, Bound)],
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

/// Expands every macro reference in `file`, following the `` `include ``s it
/// reaches through `includes` and evaluating the conditionals it meets.
pub fn expand(origins: &mut Origins, file: FileId, includes: &Includes) -> Vec<ExpandedToken> {
    let mut expander = Expander::new(origins, includes, MacroTable::new());
    let tokens = expander.lex(file);
    expander.out.reserve(tokens.len());
    expander.expand_range(TokenSpan::new(file, 0, tokens.len() as u32), &Frame::FILE);
    expander.out
}

/// Expands one stretch of a file, against the definitions already in `table`.
///
/// [`expand`] is this over a whole file with an empty table. The other case is
/// asking what a *piece* of source means -- one branch of a conditional, say --
/// where the piece is not the file and the definitions it needs were made
/// somewhere the piece does not contain.
pub fn expand_span(
    origins: &mut Origins,
    span: TokenSpan,
    table: MacroTable,
    includes: &Includes,
) -> Vec<ExpandedToken> {
    let mut expander = Expander::new(origins, includes, table);
    expander.lex(span.file);
    expander.expand_range(span, &Frame::FILE);
    expander.out
}

struct Expander<'a> {
    origins: &'a mut Origins,
    includes: &'a Includes<'a>,
    /// Each file's tokens, shared rather than borrowed: a slice taken out of
    /// this map could not be held across a write to `origins`, and every
    /// expansion writes to it.
    lexed: FxHashMap<FileId, Rc<[Token]>>,
    /// The table as it stands at the point being expanded, rebuilt as the
    /// directives go past rather than taken whole from the scan: a reference
    /// may only use a definition that precedes it.
    table: MacroTable,
    out: Vec<ExpandedToken>,
    /// The definitions currently being expanded, by the name token of each.
    ///
    /// A name already here is a macro that has reached itself, directly or
    /// through others.
    active: Vec<TokenId>,
}

impl<'a> Expander<'a> {
    fn new(
        origins: &'a mut Origins,
        includes: &'a Includes<'a>,
        table: MacroTable,
    ) -> Expander<'a> {
        Expander {
            origins,
            includes,
            lexed: FxHashMap::default(),
            table,
            out: Vec::new(),
            active: Vec::new(),
        }
    }

    /// Lexes a file and keeps its tokens, so that anything addressing them
    /// later can be read against them.
    fn lex(&mut self, file: FileId) -> Rc<[Token]> {
        let tokens: Rc<[Token]> = crate::tokenize(self.origins.text(file)).into();
        self.lexed.insert(file, Rc::clone(&tokens));
        tokens
    }

    fn tokens(&self, file: FileId) -> Rc<[Token]> {
        Rc::clone(
            self.lexed
                .get(&file)
                .expect("a file is lexed before anything addresses it"),
        )
    }

    /// The text of one token, whichever file it is in.
    fn text_at(&self, id: TokenId) -> &str {
        let token = self.lexed[&id.file][id.index as usize];
        token.text(self.origins.text(id.file))
    }

    /// Walks `span` as substitution text, expanding what it finds.
    ///
    /// Used for a file's own top level too, where the frame binds nothing and
    /// every token is simply emitted. One path rather than two is worth the
    /// empty lookups: it is the same question either way -- and it is what
    /// lets a file read through an `` `include `` see the definitions the file
    /// that included it had made, which a per-file scan cannot.
    fn expand_range(&mut self, span: TokenSpan, frame: &Frame) {
        let tokens = self.tokens(span.file);
        let mut at = span.start;
        while at < span.end {
            at = self.step(&tokens, span.with(at..span.end), frame);
        }
    }

    /// Handles the token at `rest.start` and returns the next index.
    ///
    /// `rest` is what is left of the text being walked; nothing read here may
    /// reach past its end.
    fn step(&mut self, tokens: &[Token], rest: TokenSpan, frame: &Frame) -> u32 {
        let at = rest.start;
        let kind = tokens[at as usize].kind;

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
                let token = tokens[at as usize];
                self.push(
                    WHITESPACE,
                    Span::new(rest.file, token.start + 1, token.end),
                    frame.from,
                );
                at + 1
            }
            // Both operators build text, so both need an expansion to hang
            // the buffer on. Outside a body there is none, and neither means
            // anything there: they are ordinary tokens the parser will reject.
            MACRO_QUOTE if frame.from.is_some() => self.stringify(tokens, rest, frame),
            MACRO_PASTE if frame.from.is_some() => self.paste(tokens, rest, frame),
            DIRECTIVE => self.directive_or_reference(tokens, rest, frame),
            _ => match self.bound(rest.at(at), frame) {
                Some(bound) => {
                    self.substitute(bound, frame);
                    at + 1
                }
                None => {
                    self.emit(tokens, rest.at(at), frame);
                    at + 1
                }
            },
        }
    }

    /// A `` `name `` inside substitution text, which may be either.
    ///
    /// A directive in a macro body is processed where the macro is used (22.2),
    /// which is here.
    fn directive_or_reference(&mut self, tokens: &[Token], rest: TokenSpan, frame: &Frame) -> u32 {
        let at = rest.start;
        let input = Input::new(rest.file, self.origins.text(rest.file), tokens);
        match DirectiveType::lookup(input.text(at)) {
            // A conditional is a region rather than a directive: what follows
            // it belongs to it, and which branch is taken decides what is read
            // at all.
            Some(DirectiveType::Ifdef | DirectiveType::Ifndef) => {
                self.conditional(tokens, rest, frame)
            }
            Some(name) => {
                let directive = super::directive::parse(name, &input, at);
                let end = directive.tokens.end.min(rest.end).max(at + 1);
                self.directive(&directive, frame);
                end
            }
            None => {
                let reference = macros::parse(&input, at, rest.end, &self.table);
                let end = reference.tokens.end;
                self.reference(&reference, frame);
                end
            }
        }
    }

    /// Evaluates the conditional region opening at `rest.start` and expands
    /// the one branch it takes.
    ///
    /// The branches not taken are not text: their `` `define ``s never reach
    /// the table and their `` `include ``s are never followed, which is what
    /// makes an include guard a guard.
    fn conditional(&mut self, tokens: &[Token], rest: TokenSpan, frame: &Frame) -> u32 {
        let region = conditional::region(
            &Input::new(rest.file, self.origins.text(rest.file), tokens),
            rest.start,
            rest.end,
        );
        let taken = region
            .branches
            .iter()
            .find(|branch| self.is_taken(branch))
            .map(|branch| branch.body);

        if let Some(body) = taken {
            self.expand_range(body, frame);
        }
        region.tokens.end
    }

    fn is_taken(&self, branch: &Branch) -> bool {
        match branch.taken {
            Taken::Defined(name) => self.table.get(self.text_at(name)).is_some(),
            Taken::Undefined(name) => self.table.get(self.text_at(name)).is_none(),
            Taken::Otherwise => true,
            Taken::Never => false,
        }
    }

    fn directive(&mut self, directive: &Directive, frame: &Frame) {
        use DirectiveType::*;

        let file = directive.tokens.file;
        let tokens = self.tokens(file);
        self.table.apply(
            &Input::new(file, self.origins.text(file), &tokens),
            directive,
        );

        match (directive.ty, &directive.operands) {
            // The two directives that are macros: they stand for a value where
            // they appear rather than instructing the preprocessor.
            (FileName | LineNumber, _) => self.builtin(&tokens, directive, frame),
            (Include, Operands::Include(path)) => self.include(directive, path, frame),
            // Every other directive is consumed. Nothing reaches the expanded
            // stream, which is a program and not the text that produced it.
            _ => {}
        }
        self.trailing(&tokens, directive, frame);
    }

    /// Emits the trivia a directive leaves behind it on its line.
    ///
    /// A directive consumes its *operands*. A comment after them was written
    /// about whatever comes next, and deleting it deletes something the reader
    /// wrote -- so only the operands go. It matters for exactly the directives
    /// that run to the end of the line, `` `define `` and the unparsed ones,
    /// because only their extent reaches past their operands.
    ///
    /// Inside a macro body there is nothing to leave behind: a comment there is
    /// not part of the substituted text (22.5.1), wherever in the body it sits.
    fn trailing(&mut self, tokens: &[Token], directive: &Directive, frame: &Frame) {
        if frame.from.is_some() {
            return;
        }
        let operands = super::directive::trim(tokens, directive.tokens.range());
        for at in operands.end..directive.tokens.end {
            if tokens[at as usize].kind.is_trivia() {
                self.emit(tokens, directive.tokens.at(at), frame);
            }
        }
    }

    /// Reads the file an `` `include `` names and walks it in place.
    ///
    /// The included text is a *file*, not substitution text: its comments are
    /// its own and its tokens are written where they are used, so it is walked
    /// at the top level however deeply nested the include was. What placed it
    /// is recorded on the file rather than on each token, which is what
    /// [`Origins::include_trace`] reads back.
    ///
    /// Every way of failing is silent. An unresolved name, a cycle and a
    /// runaway depth all leave the include expanding to nothing, which is what
    /// a directive does; each is a diagnostic waiting for a layer to report
    /// to, and they are tabulated in `docs/limitations.md`.
    fn include(&mut self, directive: &Directive, path: &IncludePath, frame: &Frame) {
        let tokens = self.tokens(directive.tokens.file);
        // An `` `include `` in a macro body happens where the macro is used,
        // so that is the file a relative name is resolved against and the site
        // the included file records as its parent.
        let site = self.origins.reported_at(TokenOrigin {
            spelled: directive.tokens.bytes(&tokens),
            from: frame.from,
        });

        let (name, angle) = match path {
            IncludePath::Quoted(at) => (unquote(self.text_at(*at)).to_string(), false),
            IncludePath::Angle(span) => (
                self.origins.slice(span.bytes(&tokens)).trim().to_string(),
                true,
            ),
            // The name arrives by expansion, so it has to be expanded before
            // it can be read -- and it may come back in either spelling.
            IncludePath::Expanded(span) => {
                let (_, expanded) = self.aside(|expander| expander.expand_range(*span, frame));
                let text = render(self.origins, &expanded);
                let text = text.trim();
                match text
                    .strip_prefix('<')
                    .and_then(|rest| rest.strip_suffix('>'))
                {
                    Some(inner) => (inner.trim().to_string(), true),
                    None => (unquote(text).to_string(), false),
                }
            }
        };
        if name.is_empty() || self.depth(site.file) >= MAX_DEPTH {
            return;
        }

        let Some((resolved, text)) =
            self.includes
                .resolve(&name, self.origins.path(site.file), angle)
        else {
            return;
        };
        if self.reenters(&resolved, site.file) {
            return;
        }

        let included = self.origins.add_included(resolved, text, site);
        let tokens = self.lex(included);
        self.expand_range(
            TokenSpan::new(included, 0, tokens.len() as u32),
            &Frame::FILE,
        );
    }

    /// How many `` `include ``s deep a file is.
    fn depth(&self, file: FileId) -> usize {
        self.origins.include_trace(file).count()
    }

    /// Whether reading `path` from `file` would re-enter a file that is
    /// already open above it, which is a cycle and cannot terminate.
    fn reenters(&self, path: &Path, file: FileId) -> bool {
        std::iter::once(file)
            .chain(self.origins.include_trace(file).map(|site| site.file))
            .any(|open| self.origins.path(open) == Some(path))
    }

    /// `` `__FILE__ `` and `` `__LINE__ ``, whose text is in no file.
    ///
    /// Both answer for the *outermost* call site when they sit in a macro body,
    /// which is what the origin map already computes: a `` `__LINE__ `` in a
    /// body reports the line the macro was used on, not the line it was written
    /// on.
    fn builtin(&mut self, tokens: &[Token], directive: &Directive, frame: &Frame) {
        let span = directive.tokens.bytes(tokens);
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
        let tokens = self.tokens(reference.tokens.file);
        let Some(Entry { def, .. }) = self.table.get(self.text_at(reference.name)) else {
            // Undefined at the point of use, which 22.5.1 makes an error. The
            // reference's own tokens are the honest stand-in until there is a
            // diagnostics layer to say so.
            return self.emit_verbatim(&tokens, reference, frame);
        };
        // Cloned so that the body can be walked while `origins` is written to.
        // One small clone per expansion, against threading the two borrows
        // through every step below.
        let def = def.clone();

        if self.active.contains(&def.name) {
            // A macro that has reached itself. Substituting again cannot
            // terminate, so the reference stands as written.
            return self.emit_verbatim(&tokens, reference, frame);
        }

        let Some(bindings) = bind(&def, reference) else {
            return self.emit_verbatim(&tokens, reference, frame);
        };

        let defined_in = self.tokens(def.tokens.file);
        let id = self.origins.expand(Expansion {
            name: reference.name.bytes(&tokens),
            call: reference.tokens.bytes(&tokens),
            def: Some(def.tokens.bytes(&defined_in)),
            // The call itself may have been placed by an expansion, which is
            // what makes a macro expanding to a macro read back as a chain.
            parent: frame.from,
        });

        self.active.push(def.name);
        self.expand_range(
            def.body,
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
            self.expand_range(
                reference
                    .tokens
                    .with(reference.name.index + 1..reference.tokens.end),
                frame,
            );
        }
    }

    /// What the token at `at` is bound to, if it names a formal of the macro
    /// being expanded.
    ///
    /// Matched on text rather than on kind: a formal may be written as an
    /// escaped identifier, and `` `define A(input) `` names one after a
    /// keyword, which the lexer has already reclassified. The formal and the
    /// token need not be in the same file, which is why both are looked up
    /// rather than sliced out of one source.
    fn bound<'f>(&self, at: TokenId, frame: &'f Frame) -> Option<&'f Bound> {
        if frame.args.is_empty() {
            return None;
        }
        let name = key(self.text_at(at));
        frame
            .args
            .iter()
            .find(|(formal, _)| key(self.text_at(*formal)) == name)
            .map(|(_, bound)| bound)
    }

    /// Splices in what a formal stands for.
    fn substitute(&mut self, bound: &Bound, frame: &Frame) {
        match *bound {
            // The argument's tokens are the caller's text, so the names in it
            // are the caller's -- but this expansion is what placed them.
            Bound::Actual(actual) => {
                let outer = frame.caller.copied().unwrap_or(Frame::FILE);
                self.expand_range(
                    actual,
                    &Frame {
                        from: frame.from,
                        ..outer
                    },
                );
            }
            // A default is written in the definition, so it reads as body text.
            Bound::Default(default) => self.expand_range(default, frame),
            Bound::Nothing => {}
        }
    }

    /// `` `" ... `" `` -- the text between the quotes, expanded, as one string
    /// literal (22.5.1).
    fn stringify(&mut self, tokens: &[Token], rest: TokenSpan, frame: &Frame) -> u32 {
        let id = frame.from.expect("only reached inside an expansion");
        let at = rest.start;
        let close = (at + 1..rest.end)
            .find(|&at| tokens[at as usize].kind == MACRO_QUOTE)
            .unwrap_or(rest.end);

        let (_, inner) =
            self.aside(|expander| expander.expand_range(rest.with(at + 1..close), frame));
        let text = quoted(self.origins, &inner);

        let len = text.len() as u32;
        let file = self.origins.add_synthesised(text, id);
        self.push(STRING_LITERAL, Span::new(file, 0, len), Some(id));
        // Past the closing quote, or to the end of the text if it never came.
        (close + 1).min(rest.end)
    }

    /// ``` `` ``` -- the delimiter that lets a formal abut the text beside it.
    ///
    /// It is *deleted*, and that is all it does. The whitespace around it is
    /// the author's and stays, so what joins is what the deletion leaves
    /// adjacent: `` reg_``n``_q `` fuses and `` force ``name``_if `` does not.
    /// Reading it as an operator that eats its own whitespace -- which is what
    /// C's `##` does -- fuses `force` onto a signal name, and the corpus has
    /// that exact macro.
    ///
    /// The left operand comes off the output rather than out of the body, so
    /// that what a formal or a nested call expanded to is what gets fused.
    fn paste(&mut self, tokens: &[Token], rest: TokenSpan, frame: &Frame) -> u32 {
        let id = frame.from.expect("only reached inside an expansion");
        let at = rest.start;
        let next = at + 1;

        let spaced = |at: u32| matches!(tokens[at as usize].kind, WHITESPACE | LINE_CONTINUATION);
        if next >= rest.end || (at > 0 && spaced(at - 1)) || spaced(next) {
            return next;
        }

        // An operator with nothing on one side of it has nothing to fuse,
        // which 22.5.1 makes an error. Dropping it leaves the other side
        // intact.
        let Some(left) = self.out.pop() else {
            return next;
        };

        let (end, mut right) =
            self.aside(|expander| expander.step(tokens, rest.with(next..rest.end), frame));
        let Some(first) = right
            .first()
            .copied()
            .filter(|token| token.kind != WHITESPACE)
        else {
            self.out.push(left);
            self.out.append(&mut right);
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
    fn emit_verbatim(&mut self, tokens: &[Token], reference: &MacroRef, frame: &Frame) {
        for at in reference.tokens.iter() {
            self.emit(tokens, at, frame);
        }
    }

    fn emit(&mut self, tokens: &[Token], at: TokenId, frame: &Frame) {
        let token = tokens[at.index as usize];
        self.push(
            token.kind,
            Span::new(at.file, token.start, token.end),
            frame.from,
        );
    }

    fn push(&mut self, kind: SyntaxKind, spelled: Span, from: Option<ExpansionId>) {
        self.out.push(ExpandedToken {
            kind,
            origin: TokenOrigin { spelled, from },
        });
    }
}

/// A quoted include's file name, without the quotes the literal carries.
fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(text)
}

/// Pairs each formal with what the call gives it, or `None` if the call and the
/// definition disagree about whether there is an argument list at all.
///
/// Free of the expander because it reads no text: a formal and an actual are
/// both already spans, and which file each is in travels with it.
fn bind(def: &MacroDef, reference: &MacroRef) -> Option<Vec<(TokenId, Bound)>> {
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
                    Some(actual) => Bound::Actual(*actual),
                    // Too few arguments. 22.5.1 lets a default stand in,
                    // and makes it an error when there is none.
                    None => match formal.default {
                        Some(default) => Bound::Default(default),
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
