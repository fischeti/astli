//! Macro definition tables and macro call reference resolution.
//!
//! SystemVerilog macro invocations (`` `NAME `` or `` `NAME(...) ``) require knowing
//! the arity of the macro to determine whether a subsequent parenthesized expression
//! is an argument list or following code. This module maintains the macro symbol table
//! and parses macro references and their argument lists.

use rustc_hash::FxHashMap;

use super::directive::{
    Directive, DirectiveType, MacroDef, Operands, end_of_line, significant, trim,
};
use super::tokens::{Input, TokenId, TokenSpan};
use svirig_syntax::SyntaxKind::*;

/// Arity of a macro definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arity {
    /// Object-like macro taking no arguments (`` `define NAME ``).
    Nullary,
    /// Function-like macro taking a specific number of formal arguments (`` `define NAME(a, b) ``).
    Formals(usize),
    /// Macro with unknown arity (e.g. defined in an external file or conflicting across branches).
    Unknown,
}

/// An entry in the [`MacroTable`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The most recent macro definition.
    pub def: MacroDef,
    /// The known arity of the macro.
    pub arity: Arity,
}

/// Symbol table tracking active macro definitions.
#[derive(Debug, Clone, Default)]
pub struct MacroTable {
    entries: FxHashMap<String, Entry>,
}

impl MacroTable {
    /// Creates a new, empty macro table.
    pub fn new() -> MacroTable {
        MacroTable::default()
    }

    /// Applies a directive to the table if it modifies definitions (`` `define ``, `` `undef ``, or `` `undefineall ``).
    pub fn apply(&mut self, input: &Input, directive: &Directive) {
        use DirectiveType::*;
        debug_assert_eq!(directive.tokens.file, input.file);

        match (&directive.ty, &directive.operands) {
            (Define, Operands::Define(def)) => self.define(input, def),
            (Undef, Operands::Name(at)) => {
                self.entries.remove(key(input.text(at.index)));
            }
            (UndefineAll, _) => self.entries.clear(),
            _ => {}
        }
    }

    /// Registers a new macro definition, updating arity tracking if redefinitions differ.
    pub fn define(&mut self, input: &Input, def: &MacroDef) {
        let arity = match &def.formals {
            Some(formals) => Arity::Formals(formals.len()),
            None => Arity::Nullary,
        };
        let name = key(input.text(def.name.index)).to_string();

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

    /// Looks up a macro definition by name.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.get(key(name))
    }

    /// Returns the arity for `name`, or [`Arity::Unknown`] if not defined.
    pub fn arity(&self, name: &str) -> Arity {
        self.get(name).map_or(Arity::Unknown, |entry| entry.arity)
    }

    /// Returns the number of defined macros in the table.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the table contains no definitions.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterates over all `(name, entry)` pairs in the table.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Entry)> {
        self.entries
            .iter()
            .map(|(name, entry)| (name.as_str(), entry))
    }
}

/// Normalizes a macro name for table lookup by stripping backticks, escape backslashes, and trailing whitespace.
pub(crate) fn key(text: &str) -> &str {
    let text = text.strip_prefix('`').unwrap_or(text);
    text.strip_prefix('\\').unwrap_or(text).trim_end()
}

/// A parsed macro invocation in a token stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroRef {
    /// Token identifier of the macro name.
    pub name: TokenId,
    /// Arguments passed to the macro call, or `None` if invoked without parentheses.
    pub args: Option<Vec<TokenSpan>>,
    /// Token span covering the macro name and argument list.
    pub tokens: TokenSpan,
}

/// Parses a macro reference starting at token index `at`, bounded by `limit`.
pub fn parse(input: &Input, at: u32, limit: u32, table: &MacroTable) -> MacroRef {
    let bare = MacroRef {
        name: input.id(at),
        args: None,
        tokens: input.span(at..at + 1),
    };

    let Some(open) = argument_list(input, at, limit, table) else {
        return bare;
    };
    match arguments(input, open, limit) {
        Some((args, end)) => MacroRef {
            name: input.id(at),
            args: Some(args),
            tokens: input.span(at..end),
        },
        None => bare,
    }
}

/// Locates the opening `(` of a macro call's argument list on the same line, if present.
fn argument_list(input: &Input, at: u32, limit: u32, table: &MacroTable) -> Option<u32> {
    if table.arity(input.text(at)) == Arity::Nullary {
        return None;
    }
    let line = end_of_line(input, at + 1).min(limit);
    let next = significant(input.tokens, at + 1..line)?;
    (input.kind(next) == L_PAREN).then_some(next)
}

/// Parses a comma-separated argument list enclosed in balanced parentheses.
fn arguments(input: &Input, open: u32, limit: u32) -> Option<(Vec<TokenSpan>, u32)> {
    let mut args = Vec::new();
    let mut depth = 1u32;
    let mut from = open + 1;
    let mut cursor = open + 1;

    while cursor < limit.min(input.len()) {
        match input.kind(cursor) {
            L_PAREN | L_BRACK | L_BRACE | APOSTROPHE_L_BRACE => depth += 1,
            R_PAREN if depth == 1 => {
                args.push(input.span(trim(input.tokens, from..cursor)));
                return Some((args, cursor + 1));
            }
            R_PAREN | R_BRACK | R_BRACE if depth > 1 => depth -= 1,
            COMMA if depth == 1 => {
                args.push(input.span(trim(input.tokens, from..cursor)));
                from = cursor + 1;
            }
            EOF => break,
            _ => {}
        }
        cursor += 1;
    }
    None
}
