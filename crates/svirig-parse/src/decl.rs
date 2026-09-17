//! Data types, declarations, and the one question that decides between them.
//!
//! # `foo bar;`
//!
//! A declaration if and only if `foo` names a type, and there is nothing in
//! the token stream that says whether it does. Deciding properly means name
//! resolution -- following every `` `include `` and every `import`, which a
//! formatter does not do ([D6](../index.html)).
//!
//! What the rules here ask instead is what *shape* the tokens are in, which
//! settles the question without answering it. Two names in a row are a
//! declaration because nothing else in the language is written that way: an
//! instantiation carries its port parentheses even when it connects nothing,
//! and no item or statement puts a bare name in front of another. So `foo` is
//! read as a type because of where it sits, not because anyone knows what it
//! names -- and a file that declares `my_pkg::hdr_t h;` parses the same
//! whether or not `my_pkg` was ever seen.
//!
//! Where no shape fits, the rule declines and the caller falls back. That is
//! the bargain [D3](../index.html) struck: the cost of not knowing is a region
//! formatted as written, not a file that fails.
//!
//! # Nets and variables are one shape
//!
//! `wire [7:0] x;` and `logic [7:0] x;` differ in a keyword, not in a shape,
//! and a formatter lays them out identically -- so both are a [`VAR_DECL`]
//! and the keyword is a child. The same goes for `struct` against `union`,
//! except there the two get their own kinds, because telling them apart by
//! reading a child token is the sort of thing a match should not have to do.

use super::event::Marker;
use super::expr::{arguments, attributes, expr};
use super::source::Tokens;
use super::{Completed, Parser, preprocessor};
use svirig_syntax::{SyntaxKind, SyntaxKind::*};

/// Whether `kind` is a type all by itself.
pub(super) fn is_builtin_type(kind: SyntaxKind) -> bool {
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
pub(super) fn is_net_type(kind: SyntaxKind) -> bool {
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
    let marker = parser.start();
    let node = declaration_at(parser, marker);
    if node.is_none() {
        parser.rollback(before);
    }
    node
}

/// The same, into a node the caller has already opened.
///
/// For a caller that had to open one before it could know what it was looking
/// at -- an item's attributes are written before the item says what it is.
/// **The caller rolls back on `None`**, which is what undoes a declaration
/// that did not reach its own `;`.
pub(super) fn declaration_at<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
) -> Option<Completed> {
    let (node, terminated) = match parser.kind(0) {
        TYPEDEF_KW => typedef(parser, marker),
        PARAMETER_KW | LOCALPARAM_KW => parameter(parser, marker),
        _ if starts_declaration(parser) => variable(parser, marker),
        _ => {
            parser.abandon(marker);
            return None;
        }
    };

    // **All or nothing.** A declaration that did not reach its own `;` was
    // read wrongly, and the tokens are worth more to the caller than a node
    // over some prefix of them: the fallback wants to start where the
    // declaration started, not in the middle of what it half understood.
    terminated.then_some(node)
}

/// Whether what is at the cursor declares something.
///
/// Two ways to be sure. A keyword that only a declaration may open settles it,
/// and otherwise a name followed by a declarator does -- which is a question
/// about the tokens' shape rather than about what the first name means.
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

    // A class member's qualifier carries the declaration the way `const`
    // does: after `local` or `static`, what follows is a type whether or not
    // this file can resolve the name. `local::x` is a constraint's scope
    // rather than a qualifier, and `static function` is a method.
    if matches!(kind, LOCAL_KW | PROTECTED_KW | STATIC_KW | AUTOMATIC_KW)
        && !matches!(parser.kind(1), COLON_COLON | FUNCTION_KW | TASK_KW)
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

    declarator_follows_type(parser)
}

/// Whether a declarator follows the type at the cursor.
///
/// A qualified name, the parameters and packed dimensions that may qualify
/// it, and then a second name. The token after that second name is the whole
/// decider: a `(` there instantiates, and everything else -- a `;`, an `=`, a
/// `,`, its own unpacked dimensions -- can only belong to a declarator.
///
/// Two names in a row are otherwise nothing at all. A module instantiation is
/// written with its port parentheses even when it connects nothing, an
/// expression statement is one name, and no other item or statement in the
/// language puts a bare name in front of another. So the shape decides it
/// without anyone having to know what the first name means, which is what the
/// caller's doc comment says is impossible to know.
fn declarator_follows_type<T: Tokens>(parser: &Parser<T>) -> bool {
    let mut ahead = 0;

    loop {
        if !matches!(parser.kind(ahead), IDENT | ESCAPED_IDENT) {
            return false;
        }
        ahead += 1;

        // `C #(8)::t x;` parameterises a class before naming a type inside it.
        if parser.kind(ahead) == HASH {
            if parser.kind(ahead + 1) != L_PAREN {
                return false;
            }
            ahead = parser.past_group(ahead + 1, L_PAREN, R_PAREN);
        }

        if parser.kind(ahead) != COLON_COLON {
            break;
        }
        ahead += 1;
    }

    while parser.kind(ahead) == L_BRACK {
        ahead = parser.past_group(ahead, L_BRACK, R_BRACK);
    }

    matches!(parser.kind(ahead), IDENT | ESCAPED_IDENT) && parser.kind(ahead + 1) != L_PAREN
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
pub(super) fn at_declarator_only<T: Tokens>(parser: &Parser<T>) -> bool {
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
        // for, and like the rest of it, what settles it is where the names
        // sit rather than what any of them mean.
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
fn typedef<T: Tokens>(parser: &mut Parser<T>, marker: Marker) -> (Completed, bool) {
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
fn parameter<T: Tokens>(parser: &mut Parser<T>, marker: Marker) -> (Completed, bool) {
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
fn variable<T: Tokens>(parser: &mut Parser<T>, marker: Marker) -> (Completed, bool) {
    while matches!(
        parser.kind(0),
        CONST_KW
            | VAR_KW
            | STATIC_KW
            | AUTOMATIC_KW
            | RAND_KW
            | RANDC_KW
            | GENVAR_KW
            | LOCAL_KW
            | PROTECTED_KW
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
pub(super) fn declarators<T: Tokens>(parser: &mut Parser<T>, types: bool) {
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
pub(super) fn declarator<T: Tokens>(parser: &mut Parser<T>, types: bool) -> Option<Completed> {
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
pub(super) fn semicolon<T: Tokens>(parser: &mut Parser<T>) -> bool {
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
pub(super) fn dimension<T: Tokens>(parser: &mut Parser<T>) {
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
