//! The macro table, and the references whose shape it decides.
//!
//! [`directive`](super::directive) splits a `` `name `` two ways and handles
//! only the first: a name in the closed directive set. This is the other half,
//! and the common one -- four `` ` `` tokens in five in real code are macro
//! references.
//!
//! # Why a reference needs a table
//!
//! `` `FOO (a + b) `` is a call with one argument if `FOO` was defined with
//! formals, and a bare reference followed by an unrelated parenthesised
//! expression if it was not. The two are different trees over the same bytes,
//! so a reference cannot be delimited without the definitions that precede it.
//!
//! **Both output modes therefore build a table.** Raw mode expands nothing, but
//! it still cannot shape a macro call without knowing the arity: *raw* means
//! unexpanded, not unprocessed.
//!
//! # Where arity is not knowable
//!
//! Two cases, and they behave alike. A macro defined in a header that raw mode
//! will never follow has no entry at all; and with every `` `ifdef `` branch
//! present at once, one name can arrive with two different formal lists. Both
//! come back as [`Arity::Unknown`], because a guess between two shapes is worse
//! than admitting the shape is unknown.
//!
//! Where it is unknown, a `(` later on the *same line* is read as an argument
//! list. Guessing wrong costs tree shape and nothing else, since the argument
//! text is preserved either way -- but the two ways of being wrong do not cost
//! the same, and the corpus says which to prefer. See
//! [Same line](#why-the-same-line).
//!
//! # Why the same line
//!
//! 22.5.1 requires a *formal* list to touch the macro's name; it puts no such
//! requirement on an actual list, and the house styles rely on that. Both PULP
//! and lowRISC align the parenthesis of a repeated call:
//!
//! ```systemverilog
//! `uvm_field_int   (is_active,   UVM_DEFAULT)
//! `AXI_ASSIGN (slink_slv_mux[0], slink_mst_ext)
//! ```
//!
//! Requiring adjacency instead misreads 256 of those, spread over 24 distinct
//! macro names, every one of which is unambiguously a call. Reading to the end
//! of the line misreads 12, all of them one macro in one file: `` `define WITH
//! iff `` is a nullary stand-in for a keyword, and `` `WITH (!rs3_valid) ``
//! hands it a parenthesised expression that is not an argument list. So the
//! rule is wrong twenty times less often, and it is wrong in the cheaper
//! direction -- a spurious argument list still reproduces its own bytes, while
//! a missed one leaves a parenthesised expression sitting in item position,
//! where the parser has no choice but to fall back to verbatim.
//!
//! No call in the corpus puts its `(` on the *next* line, so stopping at the
//! newline costs nothing measurable and bounds how far a wrong guess can reach.
//!
//! Note where the counterexample is *not* wrong. In the file that defines
//! `` `WITH ``, the arity is known and the parenthesis is correctly left alone;
//! the 12 misreadings are in the file that gets the definition from an include.
//! Getting them right needs the definition, not a better guess.
//!
//! Measured over the corpus commits in `corpus/MANIFEST` as resolved
//! 2026-08-23, deduplicated by file content to 4475 files: 29724 macro
//! references, of which 28242 have no definition in their own file. **Arity is
//! unknown for 95% of references and always will be**, because raw mode does
//! not follow includes (decision D6 in `docs/plan.md`). The fallback is the
//! common path, not the corner case.

use std::ops::Range;

use rustc_hash::FxHashMap;

use super::directive::{
    Directive, DirectiveName, MacroDef, Operands, end_of_line, significant, trim,
};
use crate::{SyntaxKind::*, Token};

/// How many arguments a macro takes, so far as the table knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arity {
    /// `` `define A `` -- no argument list, so a `(` that follows a reference
    /// belongs to whatever comes next and not to the macro.
    Nullary,
    /// `` `define A(x, y) `` -- an argument list of this many formals. Zero is
    /// `` `define A() ``, which still has to be invoked as `` `A() ``.
    Formals(usize),
    /// Either no definition is in scope, or two disagreed. See
    /// [Where arity is not knowable](self#where-arity-is-not-knowable).
    Unknown,
}

/// What the table holds for one name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The most recent definition, which is the one expansion substitutes.
    /// 22.5.1 lets a macro be redefined, and the later definition wins.
    pub def: MacroDef,
    /// The arity, which is `def`'s own unless a redefinition disagreed with it.
    pub arity: Arity,
}

/// Name to definition, as of some point in the token stream.
///
/// The table is built by walking a file forwards, so it always describes the
/// definitions that precede the reference being read -- which is the only thing
/// a reference may depend on, since a macro must be defined before it is used.
///
/// Token indices in an [`Entry`] address the slice the definition was read
/// from. That is one file today. Following an `` `include `` will make them
/// need a file alongside them.
#[derive(Debug, Clone, Default)]
pub struct MacroTable {
    entries: FxHashMap<String, Entry>,
}

impl MacroTable {
    pub fn new() -> MacroTable {
        MacroTable::default()
    }

    /// Applies a directive, if it is one of the three that change the table.
    pub fn apply(&mut self, source: &str, tokens: &[Token], directive: &Directive) {
        use DirectiveName::*;

        match (&directive.name, &directive.operands) {
            (Define, Operands::Define(def)) => self.define(source, tokens, def),
            (Undef, Operands::Name(at)) => {
                self.entries.remove(key(tokens[*at as usize].text(source)));
            }
            (UndefineAll, _) => self.entries.clear(),
            _ => {}
        }
    }

    /// Records a definition, keeping the arity only while it stays consistent.
    pub fn define(&mut self, source: &str, tokens: &[Token], def: &MacroDef) {
        let arity = match &def.formals {
            Some(formals) => Arity::Formals(formals.len()),
            None => Arity::Nullary,
        };
        let name = key(tokens[def.name as usize].text(source)).to_string();

        self.entries
            .entry(name)
            .and_modify(|entry| {
                entry.def = def.clone();
                if entry.arity != arity {
                    entry.arity = Arity::Unknown;
                }
            })
            .or_insert(Entry {
                def: def.clone(),
                arity,
            });
    }

    /// Looks a name up. `name` may be written with or without its backtick.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.get(key(name))
    }

    /// The arity to shape a reference with. An unknown *name* and an ambiguous
    /// *arity* are deliberately the same answer here; on the expanded path,
    /// where the includes have been followed, the first is an error and the
    /// caller has to tell them apart with [`MacroTable::get`].
    pub fn arity(&self, name: &str) -> Arity {
        self.get(name).map_or(Arity::Unknown, |entry| entry.arity)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Entry)> {
        self.entries
            .iter()
            .map(|(name, entry)| (name.as_str(), entry))
    }
}

/// How the table keys a name, given any of its three spellings.
///
/// A reference carries its backtick and a definition does not, and either may
/// be written as an escaped identifier -- whose `\` is not part of the name and
/// whose terminating whitespace is part of the *token* (5.6.1). Normalising all
/// of it in one place is what lets `` `define \FOO x `` be used as `` `FOO ``.
pub(crate) fn key(text: &str) -> &str {
    let text = text.strip_prefix('`').unwrap_or(text);
    text.strip_prefix('\\').unwrap_or(text).trim_end()
}

/// One use of a macro: `` `FOO `` or `` `FOO(a, b) ``.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroRef {
    /// The [`DIRECTIVE`] token holding the name, backtick included.
    pub name: u32,
    /// One token range per actual argument, each trimmed of trivia, or `None`
    /// when the reference has no argument list at all -- which is not the same
    /// as an empty one.
    ///
    /// An argument may be empty: `` `A(,) `` passes two of them, and `` `A() ``
    /// passes one, because the list is split on commas and nothing else. A
    /// macro with no formals reads that one empty argument as none, which needs
    /// the arity and so belongs to the caller rather than here.
    pub args: Option<Vec<Range<u32>>>,
    /// The tokens the reference covers: the introducer through the closing `)`.
    pub tokens: Range<u32>,
}

/// Reads the macro reference introduced at `at`, taking no token from `limit`
/// onwards.
///
/// The caller has already established that `tokens[at]` is a [`DIRECTIVE`]
/// whose name is not a directive's.
///
/// `limit` is the end of the text the reference is being read out of. Scanning
/// a file it is the end of the token slice, but a reference inside a macro body
/// or a macro argument may not reach past the text that holds it: an
/// unterminated call at the end of a body would otherwise take its arguments
/// from the call site, which is not text the body is allowed to see.
pub fn parse(source: &str, tokens: &[Token], at: u32, limit: u32, table: &MacroTable) -> MacroRef {
    let bare = MacroRef {
        name: at,
        args: None,
        tokens: at..at + 1,
    };

    let Some(open) = argument_list(source, tokens, at, limit, table) else {
        return bare;
    };
    match arguments(tokens, open, limit) {
        Some((args, end)) => MacroRef {
            name: at,
            args: Some(args),
            tokens: at..end,
        },
        // Never closed. Reading it as a nullary reference loses the call's
        // shape; reading it as a call swallows the rest of the file, which is
        // the worse of the two by a wide margin.
        None => bare,
    }
}

/// The index of the `(` that opens the reference's argument list, if it has
/// one.
///
/// A definition that says the macro takes no arguments is the only thing that
/// rules a parenthesis out. Otherwise the search runs to the end of the line
/// and no further; see [Why the same line](self#why-the-same-line).
fn argument_list(
    source: &str,
    tokens: &[Token],
    at: u32,
    limit: u32,
    table: &MacroTable,
) -> Option<u32> {
    if table.arity(tokens[at as usize].text(source)) == Arity::Nullary {
        return None;
    }
    let line = end_of_line(source, tokens, at + 1).min(limit);
    let next = significant(tokens, at + 1..line)?;
    (tokens[next as usize].kind == L_PAREN).then_some(next)
}

/// Splits a balanced argument list, `open` being its `(`. Returns the arguments
/// and the index just past the closing `)`, or `None` if there is not one.
///
/// Arguments are token soup. They balance on `()`, `[]` and `{}` and on nothing
/// else -- an argument is text, and parsing it as an expression is wrong. That
/// is also what makes a nested call's commas safe without knowing its arity:
/// its own parentheses protect them.
fn arguments(tokens: &[Token], open: u32, limit: u32) -> Option<(Vec<Range<u32>>, u32)> {
    let mut args = Vec::new();
    let mut depth = 1u32;
    let mut from = open + 1;
    let mut cursor = open + 1;

    while cursor < limit.min(tokens.len() as u32) {
        match tokens[cursor as usize].kind {
            // `'{` opens an assignment pattern and is closed by an ordinary
            // `}`, so it counts as a brace despite being one token.
            L_PAREN | L_BRACK | L_BRACE | APOSTROPHE_L_BRACE => depth += 1,
            R_PAREN if depth == 1 => {
                args.push(trim(tokens, from..cursor));
                return Some((args, cursor + 1));
            }
            // A closer with no opener is soup like anything else, and must not
            // take the depth below the list's own.
            R_PAREN | R_BRACK | R_BRACE if depth > 1 => depth -= 1,
            COMMA if depth == 1 => {
                args.push(trim(tokens, from..cursor));
                from = cursor + 1;
            }
            EOF => break,
            _ => {}
        }
        cursor += 1;
    }
    None
}
