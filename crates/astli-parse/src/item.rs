//! Parsing rules for SystemVerilog items and top-level descriptions.
//!
//! Handles declarations that appear at package, compilation unit, module,
//! interface, program, or class scope:
//! - Module, interface, program, and package shells
//! - Class declarations and class members
//! - Procedural blocks (`always`, `initial`, `final`)
//! - Continuous assignments (`assign`)
//! - Module instantiations and port connections
//! - Subroutine declarations (`task`, `function`, and prototypes)
//! - Modport declarations and package import/export declarations
//! - Port lists and parameter port lists

use super::decl::{
    at_declarator_only, data_type, declaration_at, declarator, declarators, dimension, is_net_type,
    semicolon,
};
use super::event::{Completed, Marker};
use super::expr::{arguments, attributes};
use super::source::{Position, Tokens};
use super::stmt::{assignment, label, statement_at, timing_control};
use super::verbatim::{Context, verbatim};
use super::{Parser, Scope, Snapshot, any, preprocessor};
use astli_syntax::{SyntaxKind, SyntaxKind::*};

/// Parses one item at the cursor, falling back to verbatim recovery if no rule matches.
pub fn item<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    if parser.at(TICK_IDENT) && preprocessor::any(parser) {
        return;
    }
    if one(parser, limit).is_some() {
        return;
    }
    verbatim(parser, Context::Terminated, limit);
}

/// Attempts to parse an item, rolling back on failure.
fn one<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) -> Option<Completed> {
    let before = parser.snapshot();
    let marker = parser.start();
    attributes(parser);

    match parser.kind(0) {
        MODULE_KW | MACROMODULE_KW => shell(parser, marker, before, MODULE_DECL, limit),
        PACKAGE_KW => shell(parser, marker, before, PACKAGE_DECL, limit),
        PROGRAM_KW => shell(parser, marker, before, PROGRAM_DECL, limit),
        INTERFACE_KW if parser.kind(1) != CLASS_KW => {
            shell(parser, marker, before, INTERFACE_DECL, limit)
        }
        CLASS_KW | INTERFACE_KW => class(parser, marker, before, limit),
        VIRTUAL_KW if parser.kind(1) == CLASS_KW => class(parser, marker, before, limit),

        ASSIGN_KW => continuous_assign(parser, marker, before),
        ALWAYS_KW | ALWAYS_COMB_KW | ALWAYS_FF_KW | ALWAYS_LATCH_KW | INITIAL_KW | FINAL_KW => {
            procedural_block(parser, marker, limit)
        }
        GENERATE_KW => generate_region(parser, marker, before, limit),

        BEGIN_KW | IF_KW | CASE_KW | CASEX_KW | CASEZ_KW | FOR_KW => {
            statement_at(parser, marker, before, limit)
        }

        CONSTRAINT_KW => constraint(parser, marker, before),
        MODPORT_KW => modport_decl(parser, marker, before),
        IMPORT_KW | EXPORT_KW if at_package_import(parser) => import_decl(parser, marker, before),
        INPUT_KW | OUTPUT_KW | INOUT_KW | REF_KW => port_decl(parser, marker, before),

        _ if at_subroutine(parser) => subroutine(parser, marker, before, limit),
        _ if at_instantiation(parser) => instantiation(parser, marker, before),

        _ => {
            let node = declaration_at(parser, marker);
            if node.is_none() {
                parser.rollback(before);
            }
            node
        }
    }
}

/// Abandons the open marker and rolls back parser state to `before`.
fn decline<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.abandon(marker);
    parser.rollback(before);
    None
}

/// Returns the expected closing keyword kind for a given declaration shell kind.
fn closer(kind: SyntaxKind) -> SyntaxKind {
    match kind {
        INTERFACE_DECL => ENDINTERFACE_KW,
        PROGRAM_DECL => ENDPROGRAM_KW,
        PACKAGE_DECL => ENDPACKAGE_KW,
        CLASS_DECL => ENDCLASS_KW,
        _ => ENDMODULE_KW,
    }
}

/// Parses a description shell (`module`, `interface`, `program`, `package`).
fn shell<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    kind: SyntaxKind,
    limit: Option<Position>,
) -> Option<Completed> {
    parser.bump();
    lifetime(parser);
    name(parser);

    while at_package_import(parser) {
        let at = parser.snapshot();
        let import = parser.start();
        if import_decl(parser, import, at).is_none() {
            break;
        }
    }

    if parser.at(HASH) && parser.kind(1) == L_PAREN {
        param_port_list(parser);
    }
    if parser.at(L_PAREN) {
        port_list(parser);
    }
    semicolon(parser);

    body(parser, closer(kind), Scope::Item, limit);
    close(parser, marker, before, closer(kind), kind)
}

/// Parses a `class` declaration.
fn class<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    limit: Option<Position>,
) -> Option<Completed> {
    if matches!(parser.kind(0), VIRTUAL_KW | INTERFACE_KW) {
        parser.bump();
    }
    parser.bump();
    lifetime(parser);
    name(parser);

    if parser.at(HASH) && parser.kind(1) == L_PAREN {
        param_port_list(parser);
    }
    if parser.at(EXTENDS_KW) {
        parser.bump();
        data_type(parser);
        if parser.at(L_PAREN) {
            arguments(parser);
        }
    }
    if parser.at(IMPLEMENTS_KW) {
        parser.bump();
        loop {
            data_type(parser);
            if parser.at(COMMA) {
                parser.bump();
            } else {
                break;
            }
        }
    }
    semicolon(parser);

    body(parser, ENDCLASS_KW, Scope::Item, limit);
    close(parser, marker, before, ENDCLASS_KW, CLASS_DECL)
}

/// Consumes the matching closing keyword and label, or rolls back.
fn close<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    closer: SyntaxKind,
    kind: SyntaxKind,
) -> Option<Completed> {
    if !parser.at(closer) {
        return decline(parser, marker, before);
    }
    parser.bump();
    label(parser);
    Some(parser.complete(marker, kind))
}

/// Parses body items up to the specified closing token kind.
fn body<T: Tokens>(
    parser: &mut Parser<T>,
    closer: SyntaxKind,
    scope: Scope,
    limit: Option<Position>,
) {
    let outer = parser.set_scope(scope);
    while !parser.at_end() && !parser.at(closer) {
        if limit.is_some_and(|limit| parser.position() >= limit) {
            break;
        }
        let before = parser.position();
        any(parser, limit);
        if parser.position() == before {
            break;
        }
    }
    parser.set_scope(outer);
}

/// Parses a `generate` … `endgenerate` block.
fn generate_region<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    limit: Option<Position>,
) -> Option<Completed> {
    parser.bump();
    body(parser, ENDGENERATE_KW, Scope::Item, limit);
    if !parser.at(ENDGENERATE_KW) {
        return decline(parser, marker, before);
    }
    parser.bump();
    Some(parser.complete(marker, GENERATE_REGION))
}

/// Parses a procedural block (`always`, `always_comb`, `always_ff`, `always_latch`, `initial`, `final`).
fn procedural_block<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    limit: Option<Position>,
) -> Option<Completed> {
    parser.bump();
    let scope = parser.set_scope(Scope::Statement);
    any(parser, limit);
    parser.set_scope(scope);
    Some(parser.complete(marker, PROCEDURAL_BLOCK))
}

/// Parses a continuous assignment statement `assign [delay] lhs = rhs, ...;`.
fn continuous_assign<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    timing_control(parser);

    loop {
        if assignment(parser).is_none() {
            break;
        }
        if parser.at(COMMA) {
            parser.bump();
        } else {
            break;
        }
    }

    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, CONTINUOUS_ASSIGN))
}

/// Parses a package `import` or `export` declaration.
fn import_decl<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    loop {
        if parser.at(STAR) {
            parser.bump();
        } else {
            name(parser);
        }
        while parser.at(COLON_COLON) {
            parser.bump();
            if parser.at(STAR) {
                parser.bump();
            } else {
                name(parser);
            }
        }
        if parser.at(COMMA) {
            parser.bump();
        } else {
            break;
        }
    }

    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, IMPORT_DECL))
}

/// Returns `true` if the cursor is at a package import/export rather than a DPI declaration.
fn at_package_import<T: Tokens>(parser: &Parser<T>) -> bool {
    matches!(parser.kind(0), IMPORT_KW | EXPORT_KW)
        && (parser.kind(1) == STAR
            || (matches!(parser.kind(1), IDENT | ESCAPED_IDENT) && parser.kind(2) == COLON_COLON))
}

/// Parses a `modport` declaration.
fn modport_decl<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    loop {
        let one = parser.start();
        name(parser);
        if parser.at(L_PAREN) {
            port_list(parser);
        }
        parser.complete(one, MODPORT);

        if parser.at(COMMA) {
            parser.bump();
        } else {
            break;
        }
    }

    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, MODPORT_DECL))
}

/// Parses a `constraint` declaration.
fn constraint<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    name(parser);

    if semicolon(parser) {
        return Some(parser.complete(marker, CONSTRAINT_DECL));
    }
    if !parser.at(L_BRACE) {
        return decline(parser, marker, before);
    }

    let end = parser.ahead(parser.past_group(0, L_BRACE, R_BRACE) as u32);
    while !parser.at_end() && parser.position() < end {
        let at = parser.position();
        verbatim(parser, Context::Terminated, Some(end));
        if parser.position() == at {
            break;
        }
    }
    Some(parser.complete(marker, CONSTRAINT_DECL))
}

/// Parses non-ANSI port declarations (`input`, `output`, `inout`, `ref`).
fn port_decl<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    while matches!(parser.kind(0), VAR_KW | CONST_KW) {
        parser.bump();
    }
    if is_net_type(parser.kind(0)) {
        parser.bump();
    }
    if !at_declarator_only(parser) {
        data_type(parser);
    }
    declarators(parser, false);

    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, PORT_DECL))
}

/// Parses a module, interface, or program instantiation.
fn instantiation<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    let ty = parser.start();
    parser.bump();
    parser.complete(ty, TYPE_REF);

    if parser.at(HASH) && parser.kind(1) == L_PAREN {
        parser.bump();
        arguments(parser);
    }

    loop {
        let one = parser.start();
        name(parser);
        while parser.at(L_BRACK) {
            dimension(parser);
        }
        if parser.at(L_PAREN) {
            arguments(parser);
        }
        parser.complete(one, INSTANCE);

        if parser.at(COMMA) {
            parser.bump();
        } else {
            break;
        }
    }

    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, INSTANTIATION))
}

/// Returns `true` if the cursor is at an instantiation construct.
fn at_instantiation<T: Tokens>(parser: &Parser<T>) -> bool {
    if !matches!(parser.kind(0), IDENT | ESCAPED_IDENT) {
        return false;
    }

    let mut ahead = 1;
    if parser.kind(ahead) == HASH {
        if parser.kind(ahead + 1) != L_PAREN {
            return false;
        }
        ahead = parser.past_group(ahead + 1, L_PAREN, R_PAREN);
    }

    if !matches!(parser.kind(ahead), IDENT | ESCAPED_IDENT) {
        return false;
    }
    ahead += 1;
    while parser.kind(ahead) == L_BRACK {
        ahead = parser.past_group(ahead, L_BRACK, R_BRACK);
    }

    parser.kind(ahead) == L_PAREN
}

/// Returns `true` if the cursor is at a subroutine declaration (`task` or `function`).
fn at_subroutine<T: Tokens>(parser: &Parser<T>) -> bool {
    if matches!(parser.kind(0), IMPORT_KW | EXPORT_KW) && parser.kind(1) == STRING_LITERAL {
        return true;
    }
    let mut ahead = 0;
    while matches!(
        parser.kind(ahead),
        EXTERN_KW | PURE_KW | VIRTUAL_KW | STATIC_KW | LOCAL_KW | PROTECTED_KW
    ) {
        ahead += 1;
    }
    matches!(parser.kind(ahead), FUNCTION_KW | TASK_KW)
}

/// Parses a task or function declaration (or prototype).
fn subroutine<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    limit: Option<Position>,
) -> Option<Completed> {
    let mut prototype = false;

    if matches!(parser.kind(0), IMPORT_KW | EXPORT_KW) && parser.kind(1) == STRING_LITERAL {
        prototype = true;
        parser.bump();
        parser.bump();
        while matches!(parser.kind(0), CONTEXT_KW | PURE_KW) {
            parser.bump();
        }
        if matches!(parser.kind(0), IDENT | ESCAPED_IDENT) && parser.kind(1) == EQ {
            parser.bump();
            parser.bump();
        }
    }

    while matches!(
        parser.kind(0),
        EXTERN_KW | PURE_KW | VIRTUAL_KW | STATIC_KW | LOCAL_KW | PROTECTED_KW
    ) {
        prototype |= matches!(parser.kind(0), EXTERN_KW | PURE_KW);
        parser.bump();
    }

    let task = parser.at(TASK_KW);
    parser.bump();
    lifetime(parser);

    if !task && !at_subroutine_name(parser) {
        data_type(parser);
    }
    subroutine_name(parser);

    if parser.at(L_PAREN) {
        port_list(parser);
    }
    if !semicolon(parser) {
        return decline(parser, marker, before);
    }

    let kind = match task {
        true => TASK_DECL,
        false => FUNCTION_DECL,
    };
    if prototype {
        return Some(parser.complete(marker, kind));
    }

    let closer = match task {
        true => ENDTASK_KW,
        false => ENDFUNCTION_KW,
    };
    body(parser, closer, Scope::Statement, limit);
    close(parser, marker, before, closer, kind)
}

/// Returns `true` if the cursor is at a subroutine name rather than a return type.
fn at_subroutine_name<T: Tokens>(parser: &Parser<T>) -> bool {
    if !matches!(parser.kind(0), IDENT | ESCAPED_IDENT | NEW_KW) {
        return false;
    }
    let mut ahead = 1;
    while parser.kind(ahead) == COLON_COLON
        && matches!(parser.kind(ahead + 1), IDENT | ESCAPED_IDENT | NEW_KW)
    {
        ahead += 2;
    }
    matches!(parser.kind(ahead), L_PAREN | SEMICOLON)
}

/// Consumes a subroutine identifier with optional class scope qualifiers (`C::name`).
fn subroutine_name<T: Tokens>(parser: &mut Parser<T>) {
    name(parser);
    while parser.at(COLON_COLON) {
        parser.bump();
        name(parser);
    }
}

/// Parses a parameter port list `#(parameter int W = 8, ...)`.
fn param_port_list<T: Tokens>(parser: &mut Parser<T>) {
    let list = parser.start();
    parser.bump();
    parser.bump();
    elements(parser, param_port);
    parser.complete(list, PARAM_PORT_LIST);
}

/// Parses a single element within a parameter port list.
fn param_port<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    let marker = parser.start();
    if matches!(parser.kind(0), PARAMETER_KW | LOCALPARAM_KW) {
        parser.bump();
    }

    let types = parser.at(TYPE_KW);
    if types {
        parser.bump();
    } else if !at_port_name(parser) {
        data_type(parser);
    }

    declarator(parser, types);
    Some(parser.complete(marker, PARAM_DECL))
}

/// Parses a port list `(input logic clk, ...)`.
fn port_list<T: Tokens>(parser: &mut Parser<T>) {
    let list = parser.start();
    parser.bump();
    elements(parser, port);
    parser.complete(list, PORT_LIST);
}

/// Parses a single port declaration within an ANSI port list.
fn port<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    let marker = parser.start();
    attributes(parser);

    if parser.at(DOT) {
        parser.bump();
        name(parser);
        if parser.at(L_PAREN) {
            arguments(parser);
        }
        return Some(parser.complete(marker, PORT));
    }

    let mut wrote = false;
    while matches!(
        parser.kind(0),
        INPUT_KW | OUTPUT_KW | INOUT_KW | REF_KW | CONST_KW | VAR_KW
    ) {
        parser.bump();
        wrote = true;
    }
    if is_net_type(parser.kind(0)) {
        parser.bump();
        wrote = true;
    }

    if parser.at(INTERFACE_KW) {
        parser.bump();
        if parser.at(DOT) {
            parser.bump();
            name(parser);
        }
        wrote = true;
    } else if !at_port_name(parser) {
        wrote |= data_type(parser).is_some();
    }

    if declarator(parser, false).is_none() && !wrote {
        parser.abandon(marker);
        return None;
    }
    Some(parser.complete(marker, PORT))
}

/// Returns `true` if the cursor is at a port name rather than a port type.
fn at_port_name<T: Tokens>(parser: &Parser<T>) -> bool {
    (matches!(parser.kind(0), IDENT | ESCAPED_IDENT) && parser.kind(1) == R_PAREN)
        || at_declarator_only(parser)
}

/// Parses a comma-separated list of elements bounded by a closing parenthesis `)`.
fn elements<T: Tokens>(parser: &mut Parser<T>, one: impl Fn(&mut Parser<T>) -> Option<Completed>) {
    while !parser.at_end() && !parser.at(R_PAREN) {
        let before = parser.snapshot();
        let at = parser.position();

        if !(parser.at(TICK_IDENT) && preprocessor::any(parser)) {
            let taken = one(parser).is_some();
            if !taken || !matches!(parser.kind(0), COMMA | R_PAREN) {
                parser.rollback(before);
                verbatim(parser, Context::Element, None);
            }
        }

        if parser.at(COMMA) {
            parser.bump();
        } else if parser.position() == at {
            break;
        }
    }

    if parser.at(R_PAREN) {
        parser.bump();
    }
}

/// Consumes an optional `static` or `automatic` lifetime keyword.
fn lifetime<T: Tokens>(parser: &mut Parser<T>) {
    if matches!(parser.kind(0), STATIC_KW | AUTOMATIC_KW) {
        parser.bump();
    }
}

/// Consumes an identifier or macro invocation name.
fn name<T: Tokens>(parser: &mut Parser<T>) {
    if parser.at(TICK_IDENT) {
        preprocessor::any(parser);
    } else if matches!(parser.kind(0), IDENT | ESCAPED_IDENT | NEW_KW) {
        parser.bump();
    }
}
