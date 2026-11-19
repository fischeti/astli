//! A SystemVerilog parser that builds a lossless syntax tree.
//!
//! The tree holds every byte of the input, whitespace and comments included,
//! so `root.text()` is the source again. Parsing never fails: what the grammar
//! does not cover yet, or cannot make sense of, becomes a `VERBATIM` node over
//! the tokens as written, and a [`Diagnostic`] reports anything malformed.
//!
//! # Parsing one file
//!
//! [`SyntaxTree`] is the door for a tool that reads one file at a time.
//!
//! ```
//! use astli_parse::SyntaxTree;
//!
//! let text = "module top;\n  logic q;\nendmodule\n";
//! let tree = SyntaxTree::parse("top.sv", text.to_string());
//! // Or `SyntaxTree::read("top.sv")?`, which reads the file itself.
//!
//! assert_eq!(tree.root().text(), text);
//! assert!(tree.diagnostics().is_empty());
//! ```
//!
//! # Walking the tree
//!
//! The tree is a [`rowan`] tree: [`SyntaxNode`]s, each with a [`SyntaxKind`],
//! over [`SyntaxToken`]s. Walk it untyped, by kind:
//!
//! ```
//! use astli_parse::SyntaxTree;
//! use astli_syntax::SyntaxKind::*;
//!
//!
//! let tree = SyntaxTree::parse("top.sv", "module a; endmodule\nmodule b; endmodule\n".into());
//! let modules = tree.root().descendants().filter(|node| node.kind() == MODULE_DECL);
//! assert_eq!(modules.count(), 2);
//! ```
//!
//! Or through the typed views in [`astli_syntax::ast`], which name each
//! node's parts. A view is a checked node and nothing more: cast any node
//! with [`AstNode::cast`](astli_syntax::ast::AstNode::cast), and expect an
//! accessor to return `None` where the source leaves a part out or the parser
//! fell back to `VERBATIM`.
//!
//! ```
//! use astli_parse::SyntaxTree;
//! use astli_syntax::ast::{self, AstNode};
//!
//! let text = "module top (input a);\n  assign y = a;\nendmodule\n";
//! let tree = SyntaxTree::parse("top.sv", text.to_string());
//! let file = ast::SourceFile::cast(tree.root().clone()).unwrap();
//!
//! for item in file.items() {
//!     if let ast::Item::ModuleDecl(module) = item {
//!         assert_eq!(module.name().unwrap().text(), "top");
//!         assert_eq!(module.port_list().unwrap().ports().count(), 1);
//!         assert!(matches!(module.items().next(), Some(ast::Item::ContinuousAssign(_))));
//!     }
//! }
//! ```
//!
//! # Diagnostics
//!
//! A [`Diagnostic`]'s span is a byte range in the file.
//! [`SyntaxTree::line_col`] turns an offset into a line and column; to render
//! one with the source under it, hand [`SyntaxTree::origins`] to `astli-diag`.
//!
//! ```
//! use astli_parse::SyntaxTree;
//!
//! let tree = SyntaxTree::parse("top.sv", "module top;\n  logic q;\n".into());
//! let diagnostic = &tree.diagnostics()[0];
//! assert_eq!(diagnostic.code.as_str(), "unclosed-at-end-of-file");
//! assert_eq!(tree.line_col(diagnostic.at.start).to_string(), "1:1");
//!
//! // The whole file became one `VERBATIM` node, and is still all there.
//! assert_eq!(tree.root().text(), tree.source());
//! ```
//!
//! # Directives and macros
//!
//! The parser reads a file as written, which is *raw mode*: it does not follow
//! an `` `include ``, and it keeps a macro call as a `MACRO_CALL` node rather
//! than expanding it. An `` `ifdef `` becomes a `CONDITIONAL_REGION` holding
//! every branch, since which one is taken depends on a build it does not see.
//!
//! What raw mode cannot tell from the file alone is a macro's arity: whether
//! `` `M (x) `` passes `(x)` to `M` or follows it. A macro defined in the file
//! answers that for itself; for one defined elsewhere, [`parse`] takes a
//! [`MacroTable`] to learn it from. That is the second tier: an explicit
//! [`Session`], which is also what a custom [`Reader`](astli_text::Reader)
//! or spans compared across files need.
//!
//! ```
//! use astli_parse::parse;
//! use astli_preproc::{Build, Session};
//! use astli_syntax::SyntaxKind::*;
//!
//! let build = Build::new().define("WIDTH", "32");
//! let mut session = Session::new().building(build);
//! let file = session.add("top.sv", "module top;\n  logic [`WIDTH-1:0] q;\nendmodule\n".into());
//!
//! let expanded = session.expand(file);
//! let parsed = parse(&session, file, expanded.macros);
//!
//! let calls = parsed.root.descendants().filter(|node| node.kind() == MACRO_CALL);
//! assert_eq!(calls.count(), 1);
//! ```
//!
//! [`parse`] takes the session by shared reference and the tree does not
//! borrow it, so more files can be added while earlier trees stay alive.
//!
//! # Expanded mode
//!
//! [`parse_expanded`] reads what a compiler reads: macros expanded, includes
//! followed, the branch a build takes and no other. Its tree spans every file
//! the expansion read, so its text is the expansion's
//! [`render`](astli_preproc::render), a space or newline added wherever a
//! macro placed two tokens that would otherwise paste. An offset indexes that
//! text, and [`Parsed::span`] maps a token back to where it was written.
//!
//! ```
//! use astli_parse::parse_expanded;
//! use astli_preproc::{Build, Session, render};
//! use astli_syntax::SyntaxKind::*;
//!
//! let mut session = Session::new().building(Build::new().define("CORE", "alu"));
//! let file = session.add("top.sv", "module top;\n  `CORE u_core ();\nendmodule\n".into());
//!
//! let expanded = session.expand(file);
//! let parsed = parse_expanded(&session, &expanded.tokens);
//! assert_eq!(parsed.root.text(), render(session.origins(), &expanded.tokens).as_str());
//!
//! // The instantiation the macro wrote is in the tree, and its type's name
//! // was spelled on the command line.
//! let instance = parsed.root.descendants().find(|node| node.kind() == INSTANTIATION).unwrap();
//! let mut tokens = instance.descendants_with_tokens().filter_map(|element| element.into_token());
//! let name = tokens.find(|token| token.kind() == IDENT).unwrap();
//! assert_eq!(name.text(), "alu");
//! let spelled = session.origins().spelled(parsed.span(&name).unwrap());
//! assert_ne!(spelled.src_id, file);
//! ```
//!
//! What a compiler reads holds no conditional to split a construct and no
//! macro to stand for one, so a `VERBATIM` node in an expanded tree is an
//! error among its [`diagnostics`](Parsed::diagnostics), `not-parsed`, at
//! its first token. A raw tree's are warnings, on request from
//! [`Parsed::unparsed`].
//!
//! # How it works
//!
//! Rules read a token stream and append a flat list of events rather than
//! building nodes, so a speculative parse is undone by truncating the list.
//! The tree is built once, at the end, with the trivia the rules never saw
//! put back. A comment on its own line goes with what follows it, one that
//! ends a line with what precedes it.

mod build;
mod checks;
mod constraint;
mod coverage;
mod decl;
mod delimiters;
mod diagnostics;
mod event;
mod expr;
mod item;
mod preprocessor;
mod property;
mod source;
mod specify;
mod stmt;
mod tree;
mod verbatim;

#[cfg(test)]
mod testing;

use source::{DirectiveShape, Expanded, Position, Raw, RegionShape, Tokens, pieces};
pub use tree::SyntaxTree;

use build::{Holds, build};
use checks::checked;
use event::{Completed, Event, Events, Marker};
use item::{elements, item};
use stmt::statement;

use astli_preproc::{ExpandedToken, MacroTable, Session};
use astli_syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode, SyntaxToken};
use astli_text::{Diagnostic, SourceId, Span};

/// How many nodes may be open at once before what opens the next is left as
/// written.
///
/// Rules recurse once per level of nesting, so this is what bounds the
/// parser's stack. Far above what anyone writes: nesting in real code stays
/// in the tens.
pub(crate) const MAX_NESTING: u32 = 256;

/// Parser state tracking token consumption, emitted events, and grammatical scope.
pub(crate) struct Parser<T> {
    tokens: T,
    events: Events,
    scope: Scope,
    /// Whether a pattern is being parsed, where `.name` binds a variable and
    /// `.*` matches anything.
    pattern: bool,
    /// Where the stream ends for the rules reading it, innermost last: the
    /// end of a conditional branch, or of a brace block, each read alone.
    fences: Vec<Position>,
}

/// Which rule takes an entry of the body or list being parsed, through [`any`]
/// or [`item::element`], with what an entry may say of the entries after it.
///
/// A conditional region holds what the body around it does, so a flag that
/// region's entries need is here; context for a rule below an entry is a
/// parameter of that rule instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    /// Outer module, interface, package, or description items.
    Item,
    /// Procedural statements within a block, loop, or subroutine.
    Statement,
    /// Constraints within braces, and the arms of an `if`, a `foreach` or an
    /// implication among them.
    Constraint,
    /// The entries of a port list, and the branches of a conditional among
    /// them; so for the two below. `formals` for the ports of a property, a
    /// sequence, a checker or a `let`, which may be a sequence or a property
    /// and default to one.
    Ports { formals: bool },
    /// The entries of a modport; `methods` once one imports or exports a
    /// subroutine, so that the entries after it without a keyword do too.
    Modport { methods: bool },
    /// The entries of a parameter port list; `types` after a type parameter,
    /// so that the entries after it with neither a keyword nor a type are
    /// type parameters too.
    Parameters { types: bool },
    /// The arguments of a call, or an instance's connections or overrides;
    /// `values` for overrides, a parameter's values, which may be
    /// `min:typ:max`.
    Arguments { values: bool },
    /// The cover points, crosses, bins and options of a `covergroup`.
    Coverage,
    /// The paths, timing checks and `specparam`s of a `specify` block.
    Specify,
    /// The rows of a UDP's `table`.
    Table,
    /// The signals, skews and declarations of a `clocking` block.
    Clocking,
    /// The rules and local parameters of a `config`.
    Config,
    /// The productions of a `randsequence`.
    Productions,
    /// The arms of a `case`, which hold items in a generate `case`, and
    /// patterns in a `case … matches`.
    CaseItems { generate: bool, patterns: bool },
}

/// Checkpoint of parser state across the event buffer, the token stream and
/// the scope, which an entry of a list may change for the entries after it.
///
/// A rollback leaves the fences alone, which is right because
/// [`Parser::fenced`] raises one for exactly the call it wraps and nothing
/// inside that call rolls back to a snapshot taken outside it: `fences`
/// checks that.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Snapshot {
    events: event::Snapshot,
    tokens: Position,
    scope: Scope,
    fences: usize,
}

impl<T: Tokens> Parser<T> {
    /// Creates a new parser reading from `tokens` with initial [`Scope::Item`].
    pub fn new(tokens: T) -> Parser<T> {
        Parser {
            tokens,
            events: Events::new(),
            scope: Scope::Item,
            pattern: false,
            fences: Vec::new(),
        }
    }

    /// Sets the current grammatical scope and returns the previous scope.
    pub fn set_scope(&mut self, scope: Scope) -> Scope {
        std::mem::replace(&mut self.scope, scope)
    }

    /// Whether the node being parsed is nested [`MAX_NESTING`] deep, too deep
    /// to open another.
    pub fn too_deep(&self) -> bool {
        self.events.open() >= MAX_NESTING
    }

    /// Parses with `parse` as if the stream ended at `end`, or at the fence
    /// around this one if that comes first: every rule and every lookahead
    /// sees `EOF` there, so what lies past it cannot change how what lies
    /// before it is read.
    pub fn fenced(&mut self, end: Position, parse: impl FnOnce(&mut Self)) {
        let end = self.fence().map_or(end, |outer| end.min(outer));
        self.fences.push(end);
        parse(self);
        self.fences.pop();
    }

    /// Where the innermost fence stands, if any does.
    fn fence(&self) -> Option<Position> {
        self.fences.last().copied()
    }

    /// Returns the syntax kind of the token `ahead` positions from the cursor,
    /// `EOF` at a fence or past it.
    pub fn kind(&self, ahead: usize) -> SyntaxKind {
        if (self.fence()).is_some_and(|fence| self.tokens.ahead(ahead as u32) >= fence) {
            return EOF;
        }
        self.tokens.kind(ahead)
    }

    /// Returns the text of the token `ahead` positions from the cursor, empty
    /// at a fence or past it.
    pub fn text(&self, ahead: usize) -> &str {
        match self.kind(ahead) {
            EOF => "",
            _ => self.tokens.text(ahead),
        }
    }

    /// Returns `true` if the token at the cursor matches `kind`.
    pub fn at(&self, kind: SyntaxKind) -> bool {
        self.kind(0) == kind
    }

    /// Returns `true` if the parser has reached the end of the token stream,
    /// or a fence.
    pub fn at_end(&self) -> bool {
        self.kind(0) == EOF
    }

    /// Returns `true` if the parser has reached the end of the file, which a
    /// fence only stands in for: what is open there is unclosed.
    pub fn at_eof(&self) -> bool {
        self.tokens.at_end()
    }

    /// Returns `true` if an encrypted envelope stands between where `since`
    /// was taken and the end of the stream: decrypted, it may close what is
    /// open there, as a vendor's often closes its module.
    pub fn enveloped(&self, since: &Snapshot) -> bool {
        self.enveloped_from(since.tokens)
    }

    /// Returns `true` if an encrypted envelope stands between `from` and the
    /// end of the stream.
    pub fn enveloped_from(&self, from: Position) -> bool {
        self.tokens.enveloped(from, self.fence())
    }

    /// Returns the current token stream position.
    pub fn position(&self) -> Position {
        self.tokens.at()
    }

    /// Returns the position `ahead` tokens from the cursor, clamped to the
    /// end of the stream or the fence.
    pub fn ahead(&self, ahead: u32) -> Position {
        let at = self.tokens.ahead(ahead);
        self.fence().map_or(at, |fence| at.min(fence))
    }

    /// Returns `true` if the token `ahead` positions from the cursor touches its predecessor directly.
    pub fn adjacent(&self, ahead: usize) -> bool {
        self.tokens.adjacent(ahead)
    }

    /// Finds the index just past the group the bracket `ahead` opens, or
    /// past that token if it is no opening bracket. A group that never
    /// closes runs to the end of the stream.
    pub fn past_group(&self, ahead: usize) -> usize {
        let opener = self.kind(ahead);
        if delimiters::bracket_depth(opener) != 1 {
            return if opener == EOF { ahead } else { ahead + 1 };
        }
        let pair = delimiters::pair(opener).map(|(pair, _)| pair);
        let mut depth = 0;
        let mut at = ahead;
        loop {
            let kind = self.kind(at);
            if kind == EOF {
                return at;
            }
            if delimiters::pair(kind).map(|(of, _)| of) == pair {
                depth += delimiters::bracket_depth(kind);
            }
            at += 1;
            if depth == 0 {
                return at;
            }
        }
    }

    /// Returns the token count covered by a macro call at the cursor, if one is present.
    pub fn macro_call(&self) -> Option<u32> {
        self.tokens.macro_call().filter(|_| !self.at_end())
    }

    /// Returns the directive shape at the cursor, if one is present.
    pub fn directive(&self) -> Option<DirectiveShape> {
        self.tokens.directive().filter(|_| !self.at_end())
    }

    /// Returns the conditional region shape at the cursor, if one begins here.
    pub fn region(&self) -> Option<RegionShape> {
        self.tokens.region().filter(|_| !self.at_end())
    }

    /// Consumes the token at the cursor.
    pub fn bump(&mut self) {
        if !self.at_end() {
            self.events.token(self.kind(0));
            self.tokens.bump();
        }
    }

    /// Starts a new syntax node and returns an uncompleted marker.
    pub fn start(&mut self) -> Marker {
        self.events.start()
    }

    /// Completes an open marker as `kind` and returns a handle to the completed node.
    pub fn complete(&mut self, marker: Marker, kind: SyntaxKind) -> Completed {
        marker.complete(&mut self.events, kind)
    }

    /// Discards an open marker, retaining any tokens emitted inside it.
    pub fn abandon(&mut self, marker: Marker) {
        marker.abandon(&mut self.events)
    }

    /// Opens a new marker that will wrap an already completed node as its parent.
    pub fn precede(&mut self, node: Completed) -> Marker {
        node.precede(&mut self.events)
    }

    /// Captures a snapshot of parser position and event buffer state.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            events: self.events.snapshot(),
            tokens: self.tokens.at(),
            scope: self.scope,
            fences: self.fences.len(),
        }
    }

    /// Rolls back the parser and token stream to the given snapshot.
    pub fn rollback(&mut self, snapshot: Snapshot) {
        debug_assert_eq!(
            snapshot.fences,
            self.fences.len(),
            "rolled back across a fence"
        );
        self.events.rollback(snapshot.events);
        self.tokens.seek(snapshot.tokens);
        self.scope = snapshot.scope;
    }

    /// Returns the span of the token currently at the cursor.
    pub fn span(&self) -> Option<Span> {
        self.tokens.span(0)
    }

    /// Finishes parsing and returns the resolved event list and diagnostics.
    pub fn finish(mut self) -> Finished {
        let diagnostics = self.events.take_diagnostics();
        Finished {
            events: self.events.resolve(),
            diagnostics,
        }
    }
}

/// Parses a single item or statement at the cursor according to the active scope.
///
/// Takes nothing at the end of the stream: a branch holding only `if (a)`
/// leaves the statement its body, and the `` `endif `` after it closes the
/// branch.
pub(crate) fn any<T: Tokens>(parser: &mut Parser<T>) {
    if parser.at_end() {
        return;
    }
    if verbatim::too_deep(parser) {
        return;
    }
    match parser.scope {
        Scope::Item => item(parser),
        Scope::Statement => statement(parser),
        Scope::Constraint => constraint::item(parser),
        Scope::Ports { .. }
        | Scope::Modport { .. }
        | Scope::Parameters { .. }
        | Scope::Arguments { .. } => item::element(parser),
        Scope::CaseItems { generate, patterns } => stmt::case_item(parser, generate, patterns),
        Scope::Coverage => coverage::item(parser),
        Scope::Specify => specify::item(parser),
        Scope::Table => item::udp_row(parser),
        Scope::Clocking => property::clocking_item(parser),
        Scope::Config => item::config_item(parser),
        Scope::Productions => stmt::production(parser),
    }
}

/// Parses the entries of a body in `scope`, each through [`any`], up to one
/// of `closers` or the end of the stream, and stops early at an entry no
/// rule consumes.
///
/// A body that ends at a position rather than a token passes no closers,
/// inside a [fence](Parser::fenced) at that position.
pub(crate) fn entries<T: Tokens>(parser: &mut Parser<T>, scope: Scope, closers: &[SyntaxKind]) {
    let outer = parser.set_scope(scope);
    while !parser.at_end() && !closers.contains(&parser.kind(0)) {
        let at = parser.position();
        any(parser);
        if parser.position() == at {
            break;
        }
    }
    parser.set_scope(outer);
}

/// Parser output containing resolved events and diagnostics prior to tree building.
#[derive(Debug)]
pub(crate) struct Finished {
    pub(crate) events: Vec<Event>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

/// What [`parse`] or [`parse_expanded`] makes of one file.
#[derive(Debug, Clone)]
pub struct Parsed {
    /// The `SOURCE_FILE` node. Its text is the file's in raw mode, and
    /// [`render`](astli_preproc::render)'s in expanded mode.
    pub root: SyntaxNode,
    /// What the grammar found malformed, in the order it found it.
    pub diagnostics: Vec<Diagnostic>,
    placement: Placement,
    holds: Holds,
}

/// Where the tree's tokens were written.
#[derive(Debug, Clone)]
enum Placement {
    /// Every token is in this file, at its offset in the tree.
    File(SourceId),
    /// Each token's offset in the tree and its span, sorted by offset.
    Expanded(Vec<(u32, Span)>),
}

impl Parsed {
    /// Where `token`, a token of this tree, was written.
    ///
    /// In raw mode that is its range in the file. In expanded mode it is the
    /// span as placed by the expansion that produced it, which
    /// [`Origins`](astli_text::Origins) resolves to where it was spelled or
    /// where to report it, and `None` for a separator the tree added between
    /// two tokens that would otherwise paste.
    pub fn span(&self, token: &SyntaxToken) -> Option<Span> {
        let range = token.text_range();
        match &self.placement {
            Placement::File(file) => {
                Some(Span::new(*file, range.start().into(), range.end().into()))
            }
            Placement::Expanded(spans) => {
                let at = u32::from(range.start());
                let found = spans.binary_search_by_key(&at, |&(offset, _)| offset);
                found.ok().map(|found| spans[found].1)
            }
        }
    }

    /// A `not-parsed` warning for each outermost `VERBATIM` node of a raw
    /// tree, which is what the grammar did not cover or could not make sense
    /// of.
    ///
    /// A node that [`diagnostics`](Parsed::diagnostics) already explain, such
    /// as the rest of a file with a `module` left open, gets none. These are
    /// not among a raw tree's diagnostics because not every tool wants them:
    /// the tree holds every byte either way, and a raw file is every
    /// configuration at once. An expanded tree's are, as errors, so this
    /// finds none in one.
    pub fn unparsed(&self) -> Vec<Diagnostic> {
        (self.runs().into_iter())
            .map(|(first, last)| diagnostics::not_parsed(first, last))
            .collect()
    }
}

/// Parses `file` in raw mode, taking the arity of a macro the file does not
/// define from `seed`.
///
/// Pass an empty [`MacroTable`] when there is no build to learn from, or the
/// `macros` of [`Session::expand`] when there is. See the
/// [crate docs](crate#directives-and-macros).
pub fn parse(session: &Session, file: SourceId, seed: MacroTable) -> Parsed {
    let input = session.input(file);
    let mut finished = run(Raw::seeded(input, seed));
    let (green, deep, holds) = build(&finished.events, &input);
    finished.diagnostics.extend(deep);
    checked(Parsed {
        root: SyntaxNode::new_root(green),
        diagnostics: finished.diagnostics,
        placement: Placement::File(file),
        holds,
    })
}

/// Parses the tokens of [`Session::expand`] in expanded mode.
///
/// The tree has no directive, macro call or conditional left, and its text is
/// not any one file's, so [`Parsed::span`] says where each token came from.
/// See the [crate docs](crate#expanded-mode).
pub fn parse_expanded(session: &Session, tokens: &[ExpandedToken]) -> Parsed {
    let pieces = pieces(session.origins(), tokens);
    let mut finished = run(Expanded::new(&pieces));
    let (green, deep, holds) = build(&finished.events, pieces.as_slice());
    finished.diagnostics.extend(deep);

    let mut spans = Vec::with_capacity(pieces.len());
    let mut offset = 0u32;
    for piece in &pieces {
        if let Some(span) = piece.span {
            spans.push((offset, span));
        }
        offset += piece.text.len() as u32;
    }

    let mut parsed = checked(Parsed {
        root: SyntaxNode::new_root(green),
        diagnostics: finished.diagnostics,
        placement: Placement::Expanded(spans),
        holds,
    });
    // What a compiler reads, the grammar may reject only where a compiler
    // would.
    let runs = parsed.runs().into_iter();
    let unparsed = runs.map(|(first, last)| diagnostics::not_parsed_expanded(first, last));
    parsed.diagnostics.extend(unparsed.collect::<Vec<_>>());
    parsed
}

/// Parses a whole stream as a sequence of items.
fn run<T: Tokens>(tokens: T) -> Finished {
    let mut parser = Parser::new(tokens);
    let root = parser.start();
    while !parser.at_end() {
        item(&mut parser);
    }
    parser.complete(root, SOURCE_FILE);
    parser.finish()
}
