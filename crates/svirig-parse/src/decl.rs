//! Parsing rules for data types, type definitions, and variable/parameter declarations.
//!
//! Without global name resolution, distinguishing a user-defined type declaration
//! (`foo bar;`) from an instantiation or expression statement requires syntactic lookahead:
//! an identifier followed by another identifier (accounting for scopes, parameters,
//! and packed dimensions) forms a declaration.

use super::event::Marker;
use super::expr::{arguments, attributes, expr};
use super::source::Tokens;
use super::{Completed, Parser, preprocessor};
use svirig_syntax::{SyntaxKind, SyntaxKind::*};

/// Returns `true` if `kind` is a built-in SystemVerilog data type keyword.
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

/// Returns `true` if `kind` is a net type keyword (IEEE 1800-2023 Section 6.6).
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

/// Returns `true` if `kind` introduces a composite or enumerated type definition.
fn opens_type(kind: SyntaxKind) -> bool {
    matches!(kind, ENUM_KW | STRUCT_KW | UNION_KW)
}

/// Parses a declaration at the cursor, returning `None` and restoring parser state on failure.
pub fn declaration<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    let before = parser.snapshot();
    let marker = parser.start();
    let node = declaration_at(parser, marker);
    if node.is_none() {
        parser.rollback(before);
    }
    node
}

/// Parses a declaration into `marker`, which has already consumed any leading attributes.
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

    terminated.then_some(node)
}

/// Returns `true` if the tokens at the cursor initiate a declaration.
fn starts_declaration<T: Tokens>(parser: &Parser<T>) -> bool {
    let kind = parser.kind(0);

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

    if matches!(kind, LOCAL_KW | PROTECTED_KW | STATIC_KW | AUTOMATIC_KW)
        && !matches!(parser.kind(1), COLON_COLON | FUNCTION_KW | TASK_KW)
    {
        return true;
    }

    if kind == VIRTUAL_KW && matches!(parser.kind(1), INTERFACE_KW | IDENT) {
        return true;
    }

    if !matches!(kind, IDENT | ESCAPED_IDENT) {
        return false;
    }

    declarator_follows_type(parser)
}

/// Checks whether a type expression at the cursor is followed by a declarator identifier.
fn declarator_follows_type<T: Tokens>(parser: &Parser<T>) -> bool {
    let mut ahead = 0;

    loop {
        if !matches!(parser.kind(ahead), IDENT | ESCAPED_IDENT) {
            return false;
        }
        ahead += 1;

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

/// Returns `true` if the identifier at the cursor is a declarator rather than a type name.
pub(super) fn at_declarator_only<T: Tokens>(parser: &Parser<T>) -> bool {
    if parser.at(TICK_IDENT) {
        return true;
    }
    if !matches!(parser.kind(0), IDENT | ESCAPED_IDENT) {
        return false;
    }
    match parser.kind(1) {
        EQ | COMMA | SEMICOLON => true,
        L_BRACK => !name_follows_dimensions(parser),
        _ => false,
    }
}

/// Checks whether an identifier follows a sequence of bracketed dimension specifications.
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

/// Parses a `typedef` declaration (both definitions and forward declarations).
fn typedef<T: Tokens>(parser: &mut Parser<T>, marker: Marker) -> (Completed, bool) {
    parser.bump();

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

/// Returns `true` if the `typedef` at the cursor is a forward declaration.
fn forward_declaration<T: Tokens>(parser: &Parser<T>) -> bool {
    let after = match (parser.kind(0), parser.kind(1)) {
        (INTERFACE_KW, CLASS_KW) => 2,
        (CLASS_KW | ENUM_KW | STRUCT_KW | UNION_KW, _) => 1,
        _ => return false,
    };
    matches!(parser.kind(after), IDENT | ESCAPED_IDENT) && parser.kind(after + 1) == SEMICOLON
}

/// Parses a `parameter` or `localparam` declaration.
fn parameter<T: Tokens>(parser: &mut Parser<T>, marker: Marker) -> (Completed, bool) {
    parser.bump();

    let types = parser.at(TYPE_KW);
    if types {
        parser.bump();
    } else if !at_declarator_only(parser) {
        data_type(parser);
    }

    declarators(parser, types);
    let terminated = semicolon(parser);
    (parser.complete(marker, PARAM_DECL), terminated)
}

/// Parses a net or variable declaration.
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

    if !at_declarator_only(parser) {
        data_type(parser);
    }

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

/// Parses a comma-separated list of declarators.
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

/// Parses a single declarator (identifier, unpacked dimensions, and initial value).
pub(super) fn declarator<T: Tokens>(parser: &mut Parser<T>, types: bool) -> Option<Completed> {
    let marker = parser.start();

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

/// Consumes a terminating semicolon if present, returning `true`.
pub(super) fn semicolon<T: Tokens>(parser: &mut Parser<T>) -> bool {
    let found = parser.at(SEMICOLON);
    if found {
        parser.bump();
    }
    found
}

/// Parses a data type specification.
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

/// Parses a named type reference, built-in type, or implicit type with packed dimensions.
fn named_type<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    let kind = parser.kind(0);
    let named = matches!(kind, IDENT | ESCAPED_IDENT)
        || is_builtin_type(kind)
        || is_net_type(kind)
        || kind == VIRTUAL_KW
        || kind == TICK_IDENT;

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

    if parser.at(VIRTUAL_KW) {
        parser.bump();
        if parser.at(INTERFACE_KW) {
            parser.bump();
        }
    }

    if parser.at(TICK_IDENT) {
        preprocessor::any(parser);
    } else {
        parser.bump();
    }

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

    if parser.at(DOT) && matches!(parser.kind(1), IDENT | ESCAPED_IDENT) {
        parser.bump();
        parser.bump();
    }

    while parser.at(L_BRACK) {
        dimension(parser);
    }

    Some(parser.complete(marker, TYPE_REF))
}

/// Parses an `enum [base_type] { ... }` enumeration type.
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
            let took_macro = parser.at(TICK_IDENT);
            if took_macro {
                preprocessor::any(parser);
            } else if matches!(parser.kind(0), IDENT | ESCAPED_IDENT) {
                parser.bump();
            }
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

/// Parses a `struct` or `union` composite type definition.
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

/// Parses a dimension specification `[ ... ]` (ranges, sizes, or array indices).
pub(super) fn dimension<T: Tokens>(parser: &mut Parser<T>) {
    let marker = parser.start();
    parser.bump();

    if parser.at(STAR) {
        parser.bump();
    } else if !parser.at(R_BRACK) {
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
