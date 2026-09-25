//! Full macro expansion and substitution engine.
//!
//! This module performs full preprocessor expansion:
//! - Follows `` `include `` directives recursively.
//! - Evaluates conditional regions against the active [`MacroTable`].
//! - Substitutes macro calls, expanding actual arguments and default values.
//! - Handles macro operators: stringification (`` `\" ``) and token pasting (``` `` ```).
//! - Places each token's [`Span`] through the expansion that produced it.

use std::rc::Rc;

use astli_text::{Diagnostic, Expansion, ExpansionId, Included, Origins, Reader, SourceId, Span};

use super::conditional::{self, Branch, Taken};
use super::diagnostics;
use super::directive::{Directive, DirectiveType, IncludePath, MacroDef, Operands};
use super::include::{Includes, MAX_DEPTH};
use super::macros::{self, Entry, MacroRef, MacroTable, key};
use super::session::Lexed;
use super::tokens::{Input, TokenId, TokenSpan};
use astli_syntax::{SyntaxKind, SyntaxKind::*, Token};

/// An expanded token: its kind, and its span as placed by the expansion
/// that produced it, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpandedToken {
    pub kind: SyntaxKind,
    pub span: Span,
}

/// Result of full preprocessor expansion.
#[derive(Debug, Clone)]
pub struct Expanded {
    /// Resulting stream of expanded tokens.
    pub tokens: Vec<ExpandedToken>,
    /// Macro table in effect at the conclusion of expansion.
    pub macros: MacroTable,
    /// Diagnostics emitted during expansion.
    pub diagnostics: Vec<Diagnostic>,
}

/// Binding of a formal parameter for a macro invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bound {
    /// Parameter bound to an actual argument provided at the call site.
    Actual(TokenSpan),
    /// Parameter using its default value from the macro definition.
    Default(TokenSpan),
    /// Unsupplied parameter with no default value (expands to nothing).
    Nothing,
}

/// Evaluation frame tracking parameter bindings and expansion origins during substitution.
#[derive(Debug, Clone, Copy)]
struct Frame<'f> {
    /// Active formal parameter bindings for the current macro body.
    args: &'f [(TokenId, Bound)],
    /// Origin expansion ID for tokens generated in this frame.
    from: Option<ExpansionId>,
    /// Calling frame enclosing the macro invocation.
    caller: Option<&'f Frame<'f>>,
}

impl Frame<'static> {
    /// Top-level frame for file evaluation.
    const FILE: Frame<'static> = Frame {
        args: &[],
        from: None,
        caller: None,
    };
}

/// Expands a complete file starting from `table`.
pub(super) fn file(
    origins: &mut Origins,
    lexed: &mut Lexed,
    includes: &Includes,
    reader: &dyn Reader,
    file: SourceId,
    table: MacroTable,
) -> Expanded {
    let mut expander = Expander::new(origins, lexed, includes, reader, table);
    let tokens = expander.lex(file);
    expander.out.reserve(tokens.len());
    expander.expand_range(TokenSpan::new(file, 0, tokens.len() as u32), &Frame::FILE);
    expander.finish()
}

/// Expands a token span within a file using a provided macro table.
pub(super) fn span(
    origins: &mut Origins,
    lexed: &mut Lexed,
    includes: &Includes,
    reader: &dyn Reader,
    span: TokenSpan,
    table: MacroTable,
) -> Expanded {
    let mut expander = Expander::new(origins, lexed, includes, reader, table);
    expander.lex(span.src_id);
    expander.expand_range(span, &Frame::FILE);
    expander.finish()
}

struct Expander<'a> {
    origins: &'a mut Origins,
    includes: &'a Includes,
    reader: &'a dyn Reader,
    lexed: &'a mut Lexed,
    table: MacroTable,
    out: Vec<ExpandedToken>,
    diags: Vec<Diagnostic>,
    active: Vec<TokenId>,
}

impl<'a> Expander<'a> {
    fn new(
        origins: &'a mut Origins,
        lexed: &'a mut Lexed,
        includes: &'a Includes,
        reader: &'a dyn Reader,
        table: MacroTable,
    ) -> Expander<'a> {
        Expander {
            origins,
            includes,
            reader,
            lexed,
            table,
            out: Vec::new(),
            diags: Vec::new(),
            active: Vec::new(),
        }
    }

    fn finish(self) -> Expanded {
        Expanded {
            tokens: self.out,
            macros: self.table,
            diagnostics: self.diags,
        }
    }

    /// The bytes of `span`, as placed by `frame`'s expansion.
    fn placed(&mut self, span: TokenSpan, tokens: &[Token], frame: &Frame) -> Span {
        self.origins.through(span.bytes(tokens), frame.from)
    }

    fn report(&mut self, diagnostic: Diagnostic) {
        self.diags.push(diagnostic);
    }

    fn lex(&mut self, file: SourceId) -> Rc<[Token]> {
        if let Some(tokens) = self.lexed.get(&file) {
            return Rc::clone(tokens);
        }
        let tokens: Rc<[Token]> = astli_syntax::tokenize(self.origins.text(file)).into();
        self.lexed.insert(file, Rc::clone(&tokens));
        tokens
    }

    fn tokens(&self, file: SourceId) -> Rc<[Token]> {
        Rc::clone(
            self.lexed
                .get(&file)
                .expect("file must be lexed before token access"),
        )
    }

    fn text_at(&self, at: TokenId) -> &str {
        let token = self.lexed[&at.src_id][at.index as usize];
        token.text(self.origins.text(at.src_id))
    }

    fn expand_range(&mut self, span: TokenSpan, frame: &Frame) {
        let tokens = self.tokens(span.src_id);
        let mut at = span.start;
        while at < span.end {
            at = self.step(&tokens, span.with(at..span.end), frame);
        }
    }

    fn step(&mut self, tokens: &[Token], rest: TokenSpan, frame: &Frame) -> u32 {
        let at = rest.start;
        let kind = tokens[at as usize].kind;

        // Skip comments inside macro bodies during substitution (IEEE 1800-2023 §22.5.1).
        if kind.is_trivia() && kind != WHITESPACE && frame.from.is_some() {
            return at + 1;
        }

        match kind {
            LINE_CONTINUATION if frame.from.is_some() => {
                let token = tokens[at as usize];
                self.push(
                    WHITESPACE,
                    Span::new(rest.src_id, token.start + 1, token.end),
                    frame.from,
                );
                at + 1
            }
            MACRO_QUOTE if frame.from.is_some() => self.stringify(tokens, rest, frame),
            MACRO_PASTE if frame.from.is_some() => self.paste(tokens, rest, frame),
            TICK_IDENT => self.directive_or_reference(tokens, rest, frame),
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

    fn directive_or_reference(&mut self, tokens: &[Token], rest: TokenSpan, frame: &Frame) -> u32 {
        let at = rest.start;
        let input = Input::new(rest.src_id, self.origins.text(rest.src_id), tokens);
        match DirectiveType::lookup(input.text(at)) {
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

    fn conditional(&mut self, tokens: &[Token], rest: TokenSpan, frame: &Frame) -> u32 {
        let region = conditional::region(
            &Input::new(rest.src_id, self.origins.text(rest.src_id), tokens),
            rest.start,
            rest.end,
        );
        for branch in &region.branches {
            if branch.taken == Taken::Never {
                let at = self.placed(branch.directive, tokens, frame);
                self.report(diagnostics::conditional_without_name(at));
            }
        }
        if !region.closed {
            let opener = region.branches[0].directive;
            let at = self.placed(opener, tokens, frame);
            self.report(diagnostics::unclosed_conditional(at));
        }

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

        let file = directive.tokens.src_id;
        let tokens = self.tokens(file);
        self.table.apply(
            &Input::new(file, self.origins.text(file), &tokens),
            directive,
        );

        match (directive.ty, &directive.operands) {
            (FileName | LineNumber, _) => self.builtin(&tokens, directive, frame),
            (Include, Operands::Include(path)) => self.include(directive, path, frame),
            (Elsif | Else | Endif, _) => {
                let written = self.text_at(directive.tokens.at(directive.tokens.start));
                let written = written.to_string();
                let at = self.placed(directive.tokens, &tokens, frame);
                self.report(diagnostics::stray_conditional(&written, at));
            }
            _ => {}
        }
        self.trailing(&tokens, directive, frame);
    }

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

    fn include(&mut self, directive: &Directive, path: &IncludePath, frame: &Frame) {
        let tokens = self.tokens(directive.tokens.src_id);
        let at = self.placed(directive.tokens, &tokens, frame);
        let site = self.origins.reported_at(at);

        let (name, angle) = match path {
            IncludePath::Quoted(at) => (unquote(self.text_at(*at)).to_string(), false),
            IncludePath::Angle(span) => (
                self.origins.slice(span.bytes(&tokens)).trim().to_string(),
                true,
            ),
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
        if name.is_empty() {
            return self.report(diagnostics::include_without_name(at));
        }
        if self.origins.include_depth(site.src_id) >= MAX_DEPTH {
            return self.report(diagnostics::include_too_deep(&name, MAX_DEPTH, at));
        }

        let candidates = self
            .includes
            .search(&name, self.origins.path(site.src_id), angle);
        let included = match self.origins.load_included(self.reader, &candidates, site) {
            Included::Opened(file) => file,
            Included::NotFound => return self.report(diagnostics::include_not_found(&name, at)),
            Included::Cycle => return self.report(diagnostics::include_cycle(&name, at)),
        };
        let tokens = self.lex(included);
        self.expand_range(
            TokenSpan::new(included, 0, tokens.len() as u32),
            &Frame::FILE,
        );
    }

    fn builtin(&mut self, tokens: &[Token], directive: &Directive, frame: &Frame) {
        let span = self.placed(directive.tokens, tokens, frame);
        let reported = self.origins.reported_at(span);

        let (kind, text) = match directive.ty {
            DirectiveType::FileName => {
                let path = self.origins.path(reported.src_id);
                let name = path.map(|path| path.display().to_string());
                (STRING_LITERAL, format!("\"{}\"", name.unwrap_or_default()))
            }
            _ => {
                let line = self.origins.line_col(reported.src_id, reported.start).line;
                (INT_LITERAL, line.to_string())
            }
        };

        let id = self.origins.expand(Expansion {
            name: span,
            call: span,
            def: None,
        });
        let len = text.len() as u32;
        let file = self.origins.add_synthesised(text);
        self.push(kind, Span::new(file, 0, len), Some(id));
    }

    fn reference(&mut self, reference: &MacroRef, frame: &Frame) {
        let tokens = self.tokens(reference.tokens.src_id);
        let Some(Entry { def, .. }) = self.table.get(self.text_at(reference.name)) else {
            let name = self.text_at(reference.name).to_string();
            let at = self.placed(reference.tokens, &tokens, frame);
            self.report(diagnostics::undefined_macro(&name, at));
            return self.emit_verbatim(&tokens, reference, frame);
        };
        let def = def.clone();

        if self.active.contains(&def.name) {
            let name = self.text_at(reference.name).to_string();
            let at = self.placed(reference.tokens, &tokens, frame);
            self.report(diagnostics::recursive_macro(&name, at));
            return self.emit_verbatim(&tokens, reference, frame);
        }

        let Some(bindings) = self.bind(&def, reference, &tokens, frame) else {
            return self.emit_verbatim(&tokens, reference, frame);
        };

        let defined_in = self.tokens(def.tokens.src_id);
        let name = self.placed(reference.name.span(), &tokens, frame);
        let call = self.placed(reference.tokens, &tokens, frame);
        let id = self.origins.expand(Expansion {
            name,
            call,
            def: Some(def.tokens.bytes(&defined_in)),
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

        if def.formals.is_none() && reference.args.is_some() {
            self.expand_range(
                reference
                    .tokens
                    .with(reference.name.index + 1..reference.tokens.end),
                frame,
            );
        }
    }

    fn bind(
        &mut self,
        def: &MacroDef,
        reference: &MacroRef,
        tokens: &[Token],
        frame: &Frame,
    ) -> Option<Vec<(TokenId, Bound)>> {
        let formals = match (&def.formals, &reference.args) {
            (None, _) => return Some(Vec::new()),
            (Some(_), None) => {
                let name = self.text_at(reference.name).to_string();
                let at = self.placed(reference.tokens, tokens, frame);
                self.report(diagnostics::missing_argument_list(&name, at));
                return None;
            }
            (Some(formals), Some(_)) => formals,
        };
        let empty = Vec::new();
        let actuals = reference.args.as_ref().unwrap_or(&empty);

        let given = match actuals.as_slice() {
            [only] if only.is_empty() && formals.is_empty() => &[][..],
            actuals => actuals,
        };

        if given.len() > formals.len() {
            let name = self.text_at(reference.name).to_string();
            let at = self.placed(reference.tokens, tokens, frame);
            self.report(diagnostics::too_many_arguments(
                &name,
                formals.len(),
                given.len(),
                at,
            ));
        }

        let mut bindings = Vec::with_capacity(formals.len());
        for (index, formal) in formals.iter().enumerate() {
            let bound = match given.get(index) {
                Some(actual) => Bound::Actual(*actual),
                None => match formal.default {
                    Some(default) => Bound::Default(default),
                    None => {
                        let name = self.text_at(reference.name).to_string();
                        let missing = self.text_at(formal.name).to_string();
                        let at = self.placed(reference.tokens, tokens, frame);
                        self.report(diagnostics::missing_argument(&name, &missing, at));
                        Bound::Nothing
                    }
                },
            };
            bindings.push((formal.name, bound));
        }
        Some(bindings)
    }

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

    fn substitute(&mut self, bound: &Bound, frame: &Frame) {
        match *bound {
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
            Bound::Default(default) => self.expand_range(default, frame),
            Bound::Nothing => {}
        }
    }

    fn stringify(&mut self, tokens: &[Token], rest: TokenSpan, frame: &Frame) -> u32 {
        let id = frame.from.expect("only reached inside an expansion");
        let at = rest.start;
        let close = (at + 1..rest.end).find(|&at| tokens[at as usize].kind == MACRO_QUOTE);
        if close.is_none() {
            let origin = self.placed(rest.with(at..rest.end), tokens, frame);
            self.report(diagnostics::unclosed_stringification(origin));
        }
        let close = close.unwrap_or(rest.end);

        let (_, inner) =
            self.aside(|expander| expander.expand_range(rest.with(at + 1..close), frame));
        let text = quoted(self.origins, &inner);

        let len = text.len() as u32;
        let file = self.origins.add_synthesised(text);
        self.push(STRING_LITERAL, Span::new(file, 0, len), Some(id));
        (close + 1).min(rest.end)
    }

    fn paste(&mut self, tokens: &[Token], rest: TokenSpan, frame: &Frame) -> u32 {
        let id = frame.from.expect("only reached inside an expansion");
        let at = rest.start;
        let next = at + 1;

        let spaced = |at: u32| matches!(tokens[at as usize].kind, WHITESPACE | LINE_CONTINUATION);
        if next >= rest.end {
            let origin = self.placed(rest.with(at..rest.end), tokens, frame);
            self.report(diagnostics::paste_without_operand(origin));
            return next;
        }
        if (at > 0 && spaced(at - 1)) || spaced(next) {
            return next;
        }

        let Some(left) = self.out.pop() else {
            let origin = self.placed(rest.with(at..next), tokens, frame);
            self.report(diagnostics::paste_without_operand(origin));
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
            self.origins.slice(left.span),
            self.origins.slice(first.span)
        );
        let file = self.origins.add_synthesised(fused);
        for token in astli_syntax::tokenize(self.origins.text(file)) {
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

    fn aside<T>(&mut self, walk: impl FnOnce(&mut Self) -> T) -> (T, Vec<ExpandedToken>) {
        let saved = std::mem::take(&mut self.out);
        let value = walk(self);
        (value, std::mem::replace(&mut self.out, saved))
    }

    fn emit_verbatim(&mut self, tokens: &[Token], reference: &MacroRef, frame: &Frame) {
        for at in reference.tokens.iter() {
            self.emit(tokens, at, frame);
        }
    }

    fn emit(&mut self, tokens: &[Token], at: TokenId, frame: &Frame) {
        let token = tokens[at.index as usize];
        self.push(
            token.kind,
            Span::new(at.src_id, token.start, token.end),
            frame.from,
        );
    }

    fn push(&mut self, kind: SyntaxKind, spelled: Span, from: Option<ExpansionId>) {
        let span = self.origins.through(spelled, from);
        self.out.push(ExpandedToken { kind, span });
    }
}

fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(text)
}

/// Renders an expanded token stream back to text with minimal necessary whitespace separation.
pub fn render(origins: &Origins, tokens: &[ExpandedToken]) -> String {
    let mut out = String::new();
    for (gap, token) in spaced(origins, tokens) {
        out.push_str(gap);
        out.push_str(origins.slice(token.span));
    }
    out
}

fn quoted(origins: &Origins, tokens: &[ExpandedToken]) -> String {
    let mut out = String::from("\"");
    for (gap, token) in spaced(origins, tokens) {
        out.push_str(if gap.is_empty() { "" } else { " " });
        match token.kind {
            MACRO_ESCAPED_QUOTE => out.push_str("\\\""),
            _ => escape(origins.slice(token.span), &mut out),
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

/// Each token but `EOF`, with what [`render`] writes before it: nothing, or
/// the space or newline that keeps it from pasting onto the token before.
///
/// Two tokens adjacent in the stream need not have been adjacent where
/// written, since a macro may have placed them, and `a` then `b` would
/// otherwise read back as `ab`.
pub fn spaced<'a>(
    origins: &'a Origins,
    tokens: &'a [ExpandedToken],
) -> impl Iterator<Item = (&'static str, &'a ExpandedToken)> {
    let mut previous: Option<Span> = None;

    tokens
        .iter()
        .filter(|token| token.kind != EOF)
        .map(move |token| {
            // Adjacent where written, whichever expansions placed them.
            let span = origins.spelled(token.span);
            let gap = match previous {
                Some(last) if last.src_id == span.src_id && last.end == span.start => "",
                Some(last) => separator(origins.slice(last), origins.slice(span)),
                None => "",
            };
            previous = Some(span);
            (gap, token)
        })
}

fn separator(left: &str, right: &str) -> &'static str {
    let separated = left.ends_with(char::is_whitespace)
        || right.starts_with(char::is_whitespace)
        || !pastes(left, right);

    if separated {
        ""
    } else if !pastes(left, &format!(" {right}")) {
        " "
    } else {
        "\n"
    }
}

fn pastes(left: &str, right: &str) -> bool {
    let joined = format!("{left}{right}");
    astli_syntax::tokenize(&joined)[0].end as usize != left.len()
}
