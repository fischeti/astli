//! Data types, declarations, and the one question that decides between them.
//!
//! # `foo bar;`
//!
//! A declaration if and only if `foo` names a type, and there is nothing in
//! the token stream that says whether it does. Deciding properly means name
//! resolution -- following every `` `include `` and every `import`, which a
//! formatter does not do ([D6](../index.html)) -- so what is here instead is a
//! set of the names *this file* gives to a type, gathered by
//! [`type_names`] before the parse begins.
//!
//! Where the set has no answer the rule declines, and the caller falls back.
//! That is the bargain [D3](../index.html) struck: the cost of not knowing is
//! a region formatted as written, not a file that fails.
//!
//! # Why a forward pass and not a running set
//!
//! Raw mode keeps **every branch of every conditional**, so a `typedef` inside
//! an `` `ifdef `` names a type whichever branch a build ends up taking. A set
//! built as the parse goes would depend on which branch happened to be read,
//! and would answer differently for the same file depending on where in it the
//! question was asked. A forward pass has neither problem and costs one walk
//! of the tokens.
//!
//! # Nets and variables are one shape
//!
//! `wire [7:0] x;` and `logic [7:0] x;` differ in a keyword, not in a shape,
//! and a formatter lays them out identically -- so both are a [`VAR_DECL`]
//! and the keyword is a child. The same goes for `struct` against `union`,
//! except there the two get their own kinds, because telling them apart by
//! reading a child token is the sort of thing a match should not have to do.

use rustc_hash::FxHashSet;

use super::expr::{arguments, attributes, expr};
use super::source::Tokens;
use super::{Completed, Parser, preprocessor};
use crate::preproc::Input;
use crate::{SyntaxKind, SyntaxKind::*};

/// Every name this file gives to a type.
///
/// Found by walking the tokens once: for each `typedef`, the last identifier
/// before the `;` that ends it. That is where the name is in every form the
/// standard gives -- `typedef logic [7:0] byte_t;`, `typedef struct { … }
/// hdr_t;`, `typedef pkg::base_t derived_t;`, `typedef class C;` -- because
/// everything else the declaration mentions is either a keyword or nested
/// inside a bracket.
pub fn type_names(input: &Input) -> FxHashSet<String> {
    let mut names = FxHashSet::default();
    let len = input.len();
    let mut at = 0;

    while at < len {
        if input.kind(at) != TYPEDEF_KW {
            at += 1;
            continue;
        }

        let mut depth = 0i32;
        let mut last = None;
        let mut cursor = at + 1;

        while cursor < len {
            match input.kind(cursor) {
                L_BRACE | L_BRACK | L_PAREN => depth += 1,
                R_BRACE | R_BRACK | R_PAREN => depth -= 1,
                SEMICOLON if depth <= 0 => break,
                IDENT | ESCAPED_IDENT if depth == 0 => last = Some(cursor),
                _ => {}
            }
            cursor += 1;
        }

        if let Some(last) = last {
            names.insert(input.text(last).to_string());
        }
        at = cursor.max(at + 1);
    }

    names
}

/// Whether `kind` is a type all by itself.
fn is_builtin_type(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        BIT_KW
            | LOGIC_KW
            | REG_KW
            | BYTE_KW
            | SHORTINT_KW
            | INT_KW
            | LONGINT_KW
            | INTEGER_KW
            | TIME_KW
            | SHORTREAL_KW
            | REAL_KW
            | REALTIME_KW
            | STRING_KW
            | CHANDLE_KW
            | EVENT_KW
            | VOID_KW
    )
}

/// Whether `kind` is one of the net types (6.6).
fn is_net_type(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        WIRE_KW
            | TRI_KW
            | TRI0_KW
            | TRI1_KW
            | TRIAND_KW
            | TRIOR_KW
            | TRIREG_KW
            | WAND_KW
            | WOR_KW
            | UWIRE_KW
            | SUPPLY0_KW
            | SUPPLY1_KW
    )
}

/// Whether `kind` opens a type that writes its own contents.
fn opens_type(kind: SyntaxKind) -> bool {
    matches!(kind, ENUM_KW | STRUCT_KW | UNION_KW)
}

/// Parses one declaration, or answers `None` without consuming anything.
///
/// The caller is at an item, member or statement boundary and is asking
/// whether what follows declares something.
pub fn declaration<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    let before = parser.snapshot();

    let (node, terminated) = match parser.kind(0) {
        TYPEDEF_KW => typedef(parser),
        PARAMETER_KW | LOCALPARAM_KW => parameter(parser),
        _ if starts_declaration(parser) => variable(parser),
        _ => return None,
    };

    // **All or nothing.** A declaration that did not reach its own `;` was
    // read wrongly, and the tokens are worth more to the caller than a node
    // over some prefix of them: the fallback wants to start where the
    // declaration started, not in the middle of what it half understood.
    if !terminated {
        parser.rollback(before);
        return None;
    }

    Some(node)
}

/// Whether what is at the cursor declares something.
///
/// Three ways to be sure and one way to decline. A keyword that only a
/// declaration may open settles it; a name the file typedef'd, followed by
/// something a declarator can start with, settles it; anything else is a name
/// this file cannot resolve, and saying so is more useful than guessing --
/// `my_module inst (…)` has the same shape and is not a declaration at all.
fn starts_declaration<T: Tokens>(parser: &Parser<T>) -> bool {
    let kind = parser.kind(0);

    // `int'(x)` is a cast, and a type name is exactly what the left of one
    // looks like. The `'` is what says the type is being *used* rather than
    // declared with.
    if parser.kind(1) == APOSTROPHE {
        return false;
    }

    if matches!(kind, CONST_KW | VAR_KW | GENVAR_KW | RAND_KW | RANDC_KW)
        || is_builtin_type(kind)
        || is_net_type(kind)
        || opens_type(kind)
    {
        return true;
    }

    // `virtual interface i vif;` -- a type in verification code, and the one
    // place `virtual` introduces one rather than qualifying a method.
    if kind == VIRTUAL_KW && matches!(parser.kind(1), INTERFACE_KW | IDENT) {
        return true;
    }

    if !matches!(kind, IDENT | ESCAPED_IDENT) {
        return false;
    }

    // `pkg::t x;` needs no type-name set. Nothing but a declaration is
    // written that way -- a scoped name cannot be instantiated and an
    // expression statement cannot be two names in a row -- so the scope
    // resolves the ambiguity the way the `'` of a cast does, by saying what
    // shape this is rather than what the name means.
    if parser.kind(1) == COLON_COLON {
        let mut ahead = 1;
        while parser.kind(ahead) == COLON_COLON {
            ahead += 2;
        }
        return matches!(parser.kind(ahead), IDENT | ESCAPED_IDENT | L_BRACK | HASH);
    }

    if !parser.at_type_name(0) {
        return false;
    }

    // The name is a type this file gave. What follows still has to look like a
    // declarator, since `my_t'(x)` uses the same name and declares nothing.
    matches!(parser.kind(1), IDENT | ESCAPED_IDENT | L_BRACK | HASH)
}

/// Whether the cursor is on a name that can only be what is being declared,
/// rather than the type in front of it.
///
/// `wire x;` writes no type and `const uvm_reg_data_t mask = 0;` writes one
/// this file never saw. Both begin with an identifier, and what separates them
/// is what comes *after*: a declarator is followed by its dimensions, its
/// initialiser, a comma or the `;`, and a type is followed by the name it
/// qualifies.
///
/// Asking this way rather than asking whether the name is a known type is what
/// lets a qualifier carry the declaration: once `const` has been written, the
/// thing after it is a type whether or not this file can resolve it.
fn at_declarator_only<T: Tokens>(parser: &Parser<T>) -> bool {
    if parser.at(TICK_IDENT) {
        return true;
    }
    if !matches!(parser.kind(0), IDENT | ESCAPED_IDENT) {
        return false;
    }
    match parser.kind(1) {
        EQ | COMMA | SEMICOLON => true,
        // `cfg_t [N-1:0] Configs;` and `csr_t regs [4];` are the same three
        // shapes of token, and the brackets belong to the type in one and to
        // the name in the other. What separates them is whether a *name*
        // comes after the brackets: if one does, the first identifier was the
        // type and these are its packed dimensions.
        //
        // This is the declaration half of the ambiguity this step is named
        // for, and unlike `foo bar;` it can be settled by looking rather than
        // by knowing -- which is why it is settled here and not left to the
        // type-name set, whose answer for a type from a package is always no.
        L_BRACK => !name_follows_dimensions(parser),
        _ => false,
    }
}

/// Whether a name follows the run of bracket groups beginning one token ahead.
fn name_follows_dimensions<T: Tokens>(parser: &Parser<T>) -> bool {
    let mut ahead = 1;
    while parser.kind(ahead) == L_BRACK {
        let mut depth = 0u32;
        loop {
            match parser.kind(ahead) {
                L_BRACK => depth += 1,
                R_BRACK => depth -= 1,
                EOF => return false,
                _ => {}
            }
            ahead += 1;
            if depth == 0 {
                break;
            }
        }
    }
    matches!(parser.kind(ahead), IDENT | ESCAPED_IDENT | TICK_IDENT)
}

/// `typedef <type> <name> <dimensions>;`, and the forward forms.
fn typedef<T: Tokens>(parser: &mut Parser<T>) -> (Completed, bool) {
    let marker = parser.start();
    parser.bump();

    // `typedef class C;` and its siblings name no type at all: they promise
    // one is coming and carry a keyword where the type would be.
    //
    // Recognised by what *follows* rather than by the keyword, because the
    // keyword is the same one that opens a real type: `typedef enum e;` is a
    // promise and `typedef enum logic [1:0] { … } e;` is a definition, and
    // only the shape after the keyword tells them apart.
    if forward_declaration(parser) {
        if parser.at(INTERFACE_KW) {
            parser.bump();
        }
        parser.bump();
    } else {
        data_type(parser);
    }

    declarator(parser, false);
    let terminated = semicolon(parser);
    (parser.complete(marker, TYPEDEF), terminated)
}

/// Whether a `typedef` only promises a type rather than defining one.
///
/// The whole of the form is a keyword, a name and a `;`, so that is what is
/// looked for -- anything else after the keyword is a type being written out.
fn forward_declaration<T: Tokens>(parser: &Parser<T>) -> bool {
    let after = match (parser.kind(0), parser.kind(1)) {
        // `typedef interface class C;`
        (INTERFACE_KW, CLASS_KW) => 2,
        (CLASS_KW | ENUM_KW | STRUCT_KW | UNION_KW, _) => 1,
        _ => return false,
    };
    matches!(parser.kind(after), IDENT | ESCAPED_IDENT) && parser.kind(after + 1) == SEMICOLON
}

/// `parameter` / `localparam`, with or without a type.
fn parameter<T: Tokens>(parser: &mut Parser<T>) -> (Completed, bool) {
    let marker = parser.start();
    parser.bump();

    // `parameter type T = int;` gives a *type* a default rather than a value.
    let types = parser.at(TYPE_KW);
    if types {
        parser.bump();
    } else if !at_declarator_only(parser) {
        // No type written means the name comes straight after, which is the
        // common `parameter WIDTH = 8` form.
        data_type(parser);
    }

    declarators(parser, types);
    let terminated = semicolon(parser);
    (parser.complete(marker, PARAM_DECL), terminated)
}

/// A net or variable declaration: qualifiers, a type, and its names.
fn variable<T: Tokens>(parser: &mut Parser<T>) -> (Completed, bool) {
    let marker = parser.start();

    while matches!(
        parser.kind(0),
        CONST_KW | VAR_KW | STATIC_KW | AUTOMATIC_KW | RAND_KW | RANDC_KW | GENVAR_KW
    ) {
        parser.bump();
    }

    let net = is_net_type(parser.kind(0));
    if net {
        parser.bump();
        if matches!(parser.kind(0), VECTORED_KW | SCALARED_KW) {
            parser.bump();
        }
    }

    // A declaration may leave the data type out -- `wire [7:0] x;` has one,
    // `wire x;` does not -- so a type that is not there is not a failure.
    if !at_declarator_only(parser) {
        data_type(parser);
    }

    // A net's delay sits between its type and the names it gives: `wire
    // #0.1 x = y;`, and the three-value `#(rise, fall, off)` form.
    if net && parser.at(HASH) {
        parser.bump();
        if parser.at(L_PAREN) {
            arguments(parser);
        } else if !parser.at_end() {
            parser.bump();
        }
    }

    declarators(parser, false);
    let terminated = semicolon(parser);
    (parser.complete(marker, VAR_DECL), terminated)
}

/// One or more comma-separated declarators.
fn declarators<T: Tokens>(parser: &mut Parser<T>, types: bool) {
    loop {
        if declarator(parser, types).is_none() {
            break;
        }
        if parser.at(COMMA) {
            parser.bump();
        } else {
            break;
        }
    }
}

/// A name, its unpacked dimensions, and its initialiser.
///
/// `types` says what an initialiser holds. A `parameter type T = int;` gives a
/// *type* its default, and `int` is not an expression -- nothing else in the
/// language puts one where a value goes, which is why this is a parameter
/// rather than something the rule could work out for itself.
fn declarator<T: Tokens>(parser: &mut Parser<T>, types: bool) -> Option<Completed> {
    let marker = parser.start();

    // A macro may stand for the name, and for its dimensions with it:
    // `logic [31:0] `X(mcause);` is one declaration whose declarator is
    // written entirely by an expansion. An atom in every position is what
    // Level B claims, and a name is a position.
    if parser.at(TICK_IDENT) {
        preprocessor::any(parser);
    } else if matches!(parser.kind(0), IDENT | ESCAPED_IDENT) {
        parser.bump();
    } else {
        parser.abandon(marker);
        return None;
    }

    while parser.at(L_BRACK) {
        dimension(parser);
    }

    if parser.at(EQ) {
        parser.bump();
        if types {
            data_type(parser);
        } else {
            expr(parser);
        }
    }

    Some(parser.complete(marker, DECLARATOR))
}

/// Takes the `;` that ends a declaration, and says whether it was there.
///
/// A declaration that did not find one was read wrongly, and its caller undoes
/// the whole thing rather than keeping a node over a prefix.
fn semicolon<T: Tokens>(parser: &mut Parser<T>) -> bool {
    let found = parser.at(SEMICOLON);
    if found {
        parser.bump();
    }
    found
}

/// One data type, in any of the forms 6.x gives.
pub fn data_type<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    match parser.kind(0) {
        ENUM_KW => Some(enum_type(parser)),
        STRUCT_KW | UNION_KW => Some(struct_type(parser)),
        TYPE_KW if parser.kind(1) == L_PAREN => {
            let marker = parser.start();
            parser.bump();
            parser.bump();
            expr(parser);
            if parser.at(R_PAREN) {
                parser.bump();
            }
            Some(parser.complete(marker, TYPE_REFERENCE))
        }
        _ => named_type(parser),
    }
}

/// A builtin type, or a name that stands for one.
fn named_type<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    let kind = parser.kind(0);
    let named = matches!(kind, IDENT | ESCAPED_IDENT)
        || is_builtin_type(kind)
        || is_net_type(kind)
        || kind == VIRTUAL_KW
        || kind == TICK_IDENT;

    // `wire [7:0] x;` writes no type at all -- 6.10 calls it implicit, and
    // what is left of it is the signing and the dimensions. A node still,
    // because the dimensions have to hang off something and the formatter
    // aligns them against the types that are written out.
    if !named {
        if !matches!(kind, L_BRACK | SIGNED_KW | UNSIGNED_KW) {
            return None;
        }
        let marker = parser.start();
        if matches!(parser.kind(0), SIGNED_KW | UNSIGNED_KW) {
            parser.bump();
        }
        while parser.at(L_BRACK) {
            dimension(parser);
        }
        return Some(parser.complete(marker, TYPE_REF));
    }

    let marker = parser.start();

    // `virtual interface i` and `virtual i` alike: the keyword, then the
    // interface's own name.
    if parser.at(VIRTUAL_KW) {
        parser.bump();
        if parser.at(INTERFACE_KW) {
            parser.bump();
        }
    }

    if parser.at(TICK_IDENT) {
        // A macro may stand for a whole type, and raw mode cannot know.
        preprocessor::any(parser);
    } else {
        parser.bump();
    }

    // `pkg::t`, `C#(W)::t`.
    while parser.at(COLON_COLON) {
        parser.bump();
        if matches!(parser.kind(0), IDENT | ESCAPED_IDENT) {
            parser.bump();
        }
    }

    if matches!(parser.kind(0), SIGNED_KW | UNSIGNED_KW) {
        parser.bump();
    }

    if parser.at(HASH) && parser.kind(1) == L_PAREN {
        parser.bump();
        arguments(parser);
    }

    // A virtual interface may name the modport it is bound through.
    if parser.at(DOT) && matches!(parser.kind(1), IDENT | ESCAPED_IDENT) {
        parser.bump();
        parser.bump();
    }

    while parser.at(L_BRACK) {
        dimension(parser);
    }

    Some(parser.complete(marker, TYPE_REF))
}

/// `enum [base] { A, B = 2, C[4] }`.
fn enum_type<T: Tokens>(parser: &mut Parser<T>) -> Completed {
    let marker = parser.start();
    parser.bump();

    if !parser.at(L_BRACE) {
        named_type(parser);
    }

    if parser.at(L_BRACE) {
        parser.bump();
        while !parser.at_end() && !parser.at(R_BRACE) {
            let variant = parser.start();
            // A macro that writes several names writes the commas between
            // them too, so the one after it is inside the expansion and not
            // here. Level B again: an atom may stand for anything, including
            // punctuation this loop would otherwise insist on seeing.
            let took_macro = parser.at(TICK_IDENT);
            if took_macro {
                preprocessor::any(parser);
            } else if matches!(parser.kind(0), IDENT | ESCAPED_IDENT) {
                parser.bump();
            }
            // `enum { A[4] }` names A0 through A3.
            while parser.at(L_BRACK) {
                dimension(parser);
            }
            if parser.at(EQ) {
                parser.bump();
                expr(parser);
            }
            let macro_named = parser.kind(0) != COMMA && !parser.at(R_BRACE);
            parser.complete(variant, ENUM_VARIANT);

            if parser.at(COMMA) {
                parser.bump();
            } else if !(macro_named && took_macro) {
                break;
            }
        }
        if parser.at(R_BRACE) {
            parser.bump();
        }
    }

    while parser.at(L_BRACK) {
        dimension(parser);
    }

    parser.complete(marker, ENUM_TYPE)
}

/// `struct packed signed { … }`, and the union that takes the same shape.
fn struct_type<T: Tokens>(parser: &mut Parser<T>) -> Completed {
    let marker = parser.start();
    let kind = match parser.kind(0) {
        UNION_KW => UNION_TYPE,
        _ => STRUCT_TYPE,
    };
    parser.bump();

    while matches!(
        parser.kind(0),
        PACKED_KW | TAGGED_KW | SIGNED_KW | UNSIGNED_KW
    ) {
        parser.bump();
    }

    if parser.at(L_BRACE) {
        parser.bump();
        while !parser.at_end() && !parser.at(R_BRACE) {
            let member = parser.start();
            attributes(parser);
            // A member of a randomised struct carries its own qualifier.
            while matches!(parser.kind(0), RAND_KW | RANDC_KW | CONST_KW) {
                parser.bump();
            }
            if data_type(parser).is_none() {
                parser.abandon(member);
                break;
            }
            declarators(parser, false);
            semicolon(parser);
            parser.complete(member, STRUCT_MEMBER);
        }
        if parser.at(R_BRACE) {
            parser.bump();
        }
    }

    while parser.at(L_BRACK) {
        dimension(parser);
    }

    parser.complete(marker, kind)
}

/// One `[ … ]`, whatever it turns out to hold.
///
/// A range, a size, a queue's `$`, an associative array's key type or `*`, or
/// nothing at all. The bracket is what the node is named for, because which of
/// those it is depends on the type it qualifies rather than on its contents.
fn dimension<T: Tokens>(parser: &mut Parser<T>) {
    let marker = parser.start();
    parser.bump();

    if parser.at(STAR) {
        parser.bump();
    } else if !parser.at(R_BRACK) {
        // An associative array keyed by a type rather than by a value.
        if is_builtin_type(parser.kind(0)) || opens_type(parser.kind(0)) {
            data_type(parser);
        } else {
            expr(parser);
            if parser.at(COLON) {
                parser.bump();
                expr(parser);
            }
        }
    }

    // Whatever is left is taken so that the node covers its own brackets. It
    // is bounded by the nesting rather than by the first `]`, so a dimension
    // whose contents no rule understood still ends where it was written to.
    let mut depth = 0u32;
    while !parser.at_end() {
        match parser.kind(0) {
            L_BRACK => depth += 1,
            R_BRACK if depth == 0 => break,
            R_BRACK => depth -= 1,
            _ => {}
        }
        parser.bump();
    }
    if parser.at(R_BRACK) {
        parser.bump();
    }

    parser.complete(marker, DIMENSION);
}
