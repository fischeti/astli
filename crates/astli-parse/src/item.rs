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
    Init, at_declarator_only, data_type, declaration_at, declarator, declarators, dimension,
    is_strength, semicolon,
};
use super::event::{Completed, Marker};
use super::expr::{argument, arguments, attributes, expr, lvalue, min_typ_max, parameter_values};
use super::property::is_concurrent;
use super::source::{Position, Tokens};
use super::stmt::{assignment, delay_control, is_immediate, label, statement_at, timing_control};
use super::verbatim::{Context, verbatim};
use super::{Parser, Scope, Snapshot, any, preprocessor};
use astli_syntax::{SyntaxKind, SyntaxKind::*};

/// Parses one item at the cursor, falling back to verbatim recovery if no rule matches.
pub fn item<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    if super::verbatim::too_deep(parser, limit) {
        return;
    }
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
        CHECKER_KW => checker(parser, marker, before, limit),
        CONFIG_KW => config(parser, marker, before, limit),
        // A prototype of a design unit, its header alone.
        EXTERN_KW if matches!(parser.kind(1), MODULE_KW | MACROMODULE_KW) => {
            shell(parser, marker, before, MODULE_DECL, limit)
        }
        EXTERN_KW if parser.kind(1) == INTERFACE_KW => {
            shell(parser, marker, before, INTERFACE_DECL, limit)
        }
        EXTERN_KW if parser.kind(1) == PROGRAM_KW => {
            shell(parser, marker, before, PROGRAM_DECL, limit)
        }
        PRIMITIVE_KW => shell(parser, marker, before, UDP_DECL, limit),
        EXTERN_KW if parser.kind(1) == PRIMITIVE_KW => {
            shell(parser, marker, before, UDP_DECL, limit)
        }
        TABLE_KW => udp_table(parser, marker, before, limit),
        VIRTUAL_KW if parser.kind(1) == CLASS_KW => class(parser, marker, before, limit),

        ASSIGN_KW => continuous_assign(parser, marker, before),
        DEFPARAM_KW => defparam(parser, marker, before),
        kind if is_gate(kind) => gate_instantiation(parser, marker, before),
        ALWAYS_KW | ALWAYS_COMB_KW | ALWAYS_FF_KW | ALWAYS_LATCH_KW | INITIAL_KW | FINAL_KW => {
            procedural_block(parser, marker, limit)
        }
        GENERATE_KW => generate_region(parser, marker, before, limit),

        BEGIN_KW | IF_KW | CASE_KW | CASEX_KW | CASEZ_KW | FOR_KW => {
            statement_at(parser, marker, before, limit)
        }
        // A deferred assertion stands among items, and may be labelled.
        ASSERT_KW | ASSUME_KW | COVER_KW if is_immediate(parser, 0) => {
            statement_at(parser, marker, before, limit)
        }
        IDENT | ESCAPED_IDENT
            if parser.kind(1) == COLON && (is_immediate(parser, 2) || is_concurrent(parser, 2)) =>
        {
            statement_at(parser, marker, before, limit)
        }
        // A generate block may be named before its `begin` as well as after.
        IDENT | ESCAPED_IDENT if parser.kind(1) == COLON && parser.kind(2) == BEGIN_KW => {
            statement_at(parser, marker, before, limit)
        }
        _ if is_concurrent(parser, 0) => statement_at(parser, marker, before, limit),
        PROPERTY_KW | SEQUENCE_KW => super::property::declaration_of(parser, marker, before),
        CLOCKING_KW => super::property::clocking(parser, marker, before, limit),
        DEFAULT_KW | GLOBAL_KW if parser.kind(1) == CLOCKING_KW => {
            super::property::clocking(parser, marker, before, limit)
        }
        DEFAULT_KW if parser.kind(1) == DISABLE_KW => {
            super::property::default_disable(parser, marker, before)
        }
        // `$error` and the like report at elaboration; a lone `;` is most
        // often written after a macro that stands for an item.
        SYSTEM_IDENT | SEMICOLON => statement_at(parser, marker, before, limit),

        BIND_KW => bind(parser, marker, before),
        COVERGROUP_KW => super::coverage::covergroup(parser, marker, before, limit),
        SPECIFY_KW => super::specify::block(parser, marker, before, limit),
        CONSTRAINT_KW => constraint(parser, marker, before, limit),
        STATIC_KW | EXTERN_KW | PURE_KW if at_constraint(parser) => {
            constraint(parser, marker, before, limit)
        }
        MODPORT_KW => modport_decl(parser, marker, before),
        IMPORT_KW | EXPORT_KW if at_package_import(parser) => import_decl(parser, marker, before),
        TIMEUNIT_KW | TIMEPRECISION_KW => timeunit_decl(parser, marker, before),
        INPUT_KW | OUTPUT_KW | INOUT_KW | REF_KW => port_decl(parser, marker, before),

        _ if at_subroutine(parser) => subroutine(parser, marker, before, limit),
        _ if at_instantiation(parser, true) => instantiation(parser, marker, before),

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
        UDP_DECL => ENDPRIMITIVE_KW,
        _ => ENDMODULE_KW,
    }
}

/// Parses a description shell (`module`, `interface`, `program`, `package`,
/// `primitive`), or the header alone of an `extern` one.
fn shell<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    kind: SyntaxKind,
    limit: Option<Position>,
) -> Option<Completed> {
    let prototype = parser.at(EXTERN_KW);
    if prototype {
        parser.bump();
    }
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
    if !semicolon(parser) && prototype {
        return decline(parser, marker, before);
    }
    if prototype {
        return Some(parser.complete(marker, EXTERN_DECL));
    }

    body(parser, closer(kind), Scope::Item, limit);
    close(parser, marker, before, closer(kind), kind)
}

/// Parses a `checker`: its name, its formals, and the items it checks with.
fn checker<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    limit: Option<Position>,
) -> Option<Completed> {
    parser.bump();
    name(parser);
    if parser.at(L_PAREN) {
        formal_list(parser);
    }
    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    body(parser, ENDCHECKER_KW, Scope::Item, limit);
    close(parser, marker, before, ENDCHECKER_KW, CHECKER_DECL)
}

/// Parses a `config`: its name, its local parameters, the cells it starts
/// from, and the rules that bind instances and cells to libraries.
fn config<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    limit: Option<Position>,
) -> Option<Completed> {
    parser.bump();
    name(parser);
    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    let outer = parser.set_scope(Scope::Item);
    while !parser.at_end() && !parser.at(ENDCONFIG_KW) {
        if limit.is_some_and(|limit| parser.position() >= limit) {
            break;
        }
        let at = parser.position();
        if matches!(
            parser.kind(0),
            DESIGN_KW | DEFAULT_KW | INSTANCE_KW | CELL_KW
        ) {
            config_rule(parser, limit);
        } else {
            item(parser, limit);
        }
        if parser.position() == at {
            break;
        }
    }
    parser.set_scope(outer);
    close(parser, marker, before, ENDCONFIG_KW, CONFIG_DECL)
}

/// Parses the `design` a configuration starts from, or one of its rules:
/// what it applies to, `default`, an instance or a cell, then the libraries
/// to search or the cell, the configuration or the parameters to use.
fn config_rule<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    let before = parser.snapshot();
    let marker = parser.start();
    loop {
        match parser.kind(0) {
            DESIGN_KW | DEFAULT_KW | INSTANCE_KW | CELL_KW | LIBLIST_KW | USE_KW | COLON
            | CONFIG_KW => parser.bump(),
            HASH if parser.kind(1) == L_PAREN => {
                parser.bump();
                parameter_values(parser);
            }
            IDENT | ESCAPED_IDENT => {
                lvalue(parser);
            }
            _ => break,
        }
    }
    if !semicolon(parser) {
        parser.abandon(marker);
        parser.rollback(before);
        verbatim(parser, Context::Terminated, limit);
        return;
    }
    parser.complete(marker, CONFIG_RULE);
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
        // An interface class may extend several.
        while parser.at(COMMA) {
            parser.bump();
            data_type(parser);
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

/// Parses a continuous assignment statement `assign [strength] [delay] lhs = rhs, ...;`.
fn continuous_assign<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    super::decl::strength(parser);
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

/// Parses `defparam` and each parameter it overrides: a path and a value,
/// which may be `min:typ:max`.
fn defparam<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    while let Some(path) = lvalue(parser) {
        if !parser.at(EQ) {
            return decline(parser, marker, before);
        }
        let assignment = parser.precede(path);
        parser.bump();
        if let Some(min) = expr(parser) {
            min_typ_max(parser, min);
        }
        parser.complete(assignment, ASSIGNMENT);
        if parser.at(COMMA) {
            parser.bump();
        } else {
            break;
        }
    }
    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, DEFPARAM))
}

/// Returns `true` if `kind` names a gate or a switch.
fn is_gate(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        AND_KW
            | NAND_KW
            | OR_KW
            | NOR_KW
            | XOR_KW
            | XNOR_KW
            | BUF_KW
            | NOT_KW
            | BUFIF0_KW
            | BUFIF1_KW
            | NOTIF0_KW
            | NOTIF1_KW
            | NMOS_KW
            | PMOS_KW
            | RNMOS_KW
            | RPMOS_KW
            | CMOS_KW
            | RCMOS_KW
            | TRAN_KW
            | RTRAN_KW
            | TRANIF0_KW
            | TRANIF1_KW
            | RTRANIF0_KW
            | RTRANIF1_KW
            | PULLUP_KW
            | PULLDOWN_KW
    )
}

/// Parses an instantiation of a gate or a switch: its strength, its delay,
/// and its instances.
fn gate_instantiation<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    super::decl::strength(parser);
    if parser.at(HASH) {
        delay_control(parser);
    }
    instances(parser);
    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, GATE_INSTANTIATION))
}

/// Parses a UDP's `table` and its rows.
fn udp_table<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    limit: Option<Position>,
) -> Option<Completed> {
    parser.bump();
    let scope = parser.set_scope(Scope::Table);
    while !parser.at_end() && !parser.at(ENDTABLE_KW) {
        if limit.is_some_and(|limit| parser.position() >= limit) {
            break;
        }
        let at = parser.position();
        udp_row(parser, limit);
        if parser.position() == at {
            break;
        }
    }
    parser.set_scope(scope);
    if !parser.at(ENDTABLE_KW) {
        return decline(parser, marker, before);
    }
    parser.bump();
    Some(parser.complete(marker, UDP_TABLE))
}

/// Parses one row of a UDP's table, falling back to verbatim recovery up to
/// its `;` if it is not one.
pub(super) fn udp_row<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    if parser.at(TICK_IDENT) && preprocessor::any(parser) {
        return;
    }
    if !udp_entry(parser) {
        verbatim(parser, Context::Terminated, limit);
    }
}

/// Parses a row of a UDP's table: its inputs, a colon, then one symbol, or
/// two with a colon between them for a sequential primitive, and `;`. Which
/// symbol may stand where depends on its text, which the finished tree is
/// checked for.
fn udp_entry<T: Tokens>(parser: &mut Parser<T>) -> bool {
    let symbol = |kind| matches!(kind, UDP_SYMBOL | QUESTION | STAR | MINUS);
    let before = parser.snapshot();
    let marker = parser.start();
    let mut inputs = 0;
    loop {
        if symbol(parser.kind(0)) {
            parser.bump();
        } else if let [L_PAREN, from, to, R_PAREN] = std::array::from_fn(|ahead| parser.kind(ahead))
            && symbol(from)
            && symbol(to)
        {
            let edge = parser.start();
            for _ in 0..4 {
                parser.bump();
            }
            parser.complete(edge, UDP_EDGE);
        } else {
            break;
        }
        inputs += 1;
    }
    let mut fields = 0;
    while fields < 2 && parser.at(COLON) && symbol(parser.kind(1)) {
        parser.bump();
        parser.bump();
        fields += 1;
    }
    if inputs == 0 || fields == 0 || !parser.at(SEMICOLON) {
        parser.abandon(marker);
        parser.rollback(before);
        return false;
    }
    parser.bump();
    parser.complete(marker, UDP_ENTRY);
    true
}

/// Parses a package `import` or `export` declaration.
pub(super) fn import_decl<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    loop {
        if !import_item(parser) {
            return decline(parser, marker, before);
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

/// Parses one `pkg::name` or `pkg::*`, and whether it was one. Packages do
/// not nest, so a second `::` is not; a macro call may stand for any part,
/// so an item holding one is taken as it comes.
fn import_item<T: Tokens>(parser: &mut Parser<T>) -> bool {
    let mut parts = 0;
    let mut called = false;
    loop {
        match parser.kind(0) {
            STAR | IDENT | ESCAPED_IDENT => parser.bump(),
            TICK_IDENT if preprocessor::any(parser) => called = true,
            _ => return false,
        }
        parts += 1;
        if !parser.at(COLON_COLON) {
            return parts == 2 || called;
        }
        parser.bump();
    }
}

/// Parses `timeunit` or `timeprecision` and its time.
fn timeunit_decl<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    if super::expr::expr(parser).is_none() || !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, TIMEUNIT_DECL))
}

/// Returns `true` if qualifiers at the cursor come before `constraint`.
fn at_constraint<T: Tokens>(parser: &Parser<T>) -> bool {
    let qualifiers = (0..)
        .take_while(|&ahead| matches!(parser.kind(ahead), STATIC_KW | EXTERN_KW | PURE_KW))
        .count();
    parser.kind(qualifiers) == CONSTRAINT_KW
}

/// Returns `true` if the cursor is at a package import/export rather than a DPI declaration.
pub(super) fn at_package_import<T: Tokens>(parser: &Parser<T>) -> bool {
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

/// Parses a `constraint` declaration, or its prototype.
fn constraint<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    limit: Option<Position>,
) -> Option<Completed> {
    while matches!(parser.kind(0), STATIC_KW | EXTERN_KW | PURE_KW) {
        parser.bump();
    }
    parser.bump();
    // `:initial`, `:extends` and `:final`, which say how a subclass may
    // override it.
    while parser.at(COLON) && matches!(parser.kind(1), INITIAL_KW | EXTENDS_KW | FINAL_KW) {
        parser.bump();
        parser.bump();
    }
    // `class::name`, for one defined outside its class.
    while matches!(parser.kind(0), IDENT | ESCAPED_IDENT) && parser.kind(1) == COLON_COLON {
        parser.bump();
        parser.bump();
    }
    name(parser);

    if parser.at(L_BRACE) {
        super::constraint::block(parser, limit);
    } else if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, CONSTRAINT_DECL))
}

/// Parses non-ANSI port declarations (`input`, `output`, `inout`, `ref`), a
/// module's or, in its body, a function's or a task's.
pub(super) fn port_decl<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    while matches!(parser.kind(0), VAR_KW | CONST_KW) {
        parser.bump();
    }
    if parser.kind(0).is_net_type() {
        parser.bump();
    }
    if !at_declarator_only(parser) {
        data_type(parser);
    }
    declarators(parser, Init::Value);

    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, PORT_DECL))
}

/// Parses `bind`, the module or instance it binds into, and the
/// instantiation it adds there.
fn bind<T: Tokens>(parser: &mut Parser<T>, marker: Marker, before: Snapshot) -> Option<Completed> {
    parser.bump();
    lvalue(parser);
    if parser.at(COLON) {
        parser.bump();
        while lvalue(parser).is_some() && parser.at(COMMA) {
            parser.bump();
        }
    }
    if !at_instantiation(parser, false) {
        return decline(parser, marker, before);
    }
    let inner = parser.snapshot();
    let instance = parser.start();
    if instantiation(parser, instance, inner).is_none() {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, BIND_DIRECTIVE))
}

/// Parses a module, interface, program, checker or UDP instantiation. A
/// checker may be a package's, `pkg::chk`; a UDP takes a strength and a
/// delay.
pub(super) fn instantiation<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    let ty = parser.start();
    parser.bump();
    if parser.at(COLON_COLON) {
        parser.bump();
        parser.bump();
    }
    parser.complete(ty, TYPE_REF);

    super::decl::strength(parser);
    if parser.at(HASH) && parser.kind(1) == L_PAREN {
        parser.bump();
        parameter_values(parser);
    } else if parser.at(HASH) {
        delay_control(parser);
    }

    instances(parser);
    if !semicolon(parser) {
        return decline(parser, marker, before);
    }
    Some(parser.complete(marker, INSTANTIATION))
}

/// Parses the instances of an instantiation, each a name, which a gate's or
/// a UDP's may leave out, and its connections.
fn instances<T: Tokens>(parser: &mut Parser<T>) {
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
}

/// Returns `true` if the cursor is at an instantiation construct. Only among
/// items may an instance be `unnamed`, as a UDP's may: in a procedure,
/// `f(a);` is a call.
pub(super) fn at_instantiation<T: Tokens>(parser: &Parser<T>, unnamed: bool) -> bool {
    if !matches!(parser.kind(0), IDENT | ESCAPED_IDENT) {
        return false;
    }

    let mut ahead = 1;
    if parser.kind(1) == COLON_COLON && matches!(parser.kind(2), IDENT | ESCAPED_IDENT) {
        ahead = 3;
    }
    // A UDP's strength, and its delay, which may be a single value.
    if parser.kind(ahead) == L_PAREN && is_strength(parser.kind(ahead + 1)) {
        ahead = parser.past_group(ahead, L_PAREN, R_PAREN);
    }
    if parser.kind(ahead) == HASH {
        ahead = match parser.kind(ahead + 1) {
            L_PAREN => parser.past_group(ahead + 1, L_PAREN, R_PAREN),
            _ => ahead + 2,
        };
    }

    if matches!(parser.kind(ahead), IDENT | ESCAPED_IDENT) {
        ahead += 1;
        while parser.kind(ahead) == L_BRACK {
            ahead = parser.past_group(ahead, L_BRACK, R_BRACK);
        }
    } else if !unnamed {
        return false;
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
        EXTERN_KW | PURE_KW | VIRTUAL_KW | STATIC_KW | LOCAL_KW | PROTECTED_KW | FORKJOIN_KW
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

    // `extern forkjoin task` in an interface: a task each module that
    // exports it defines, and that runs in all of them at once.
    while matches!(
        parser.kind(0),
        EXTERN_KW | PURE_KW | VIRTUAL_KW | STATIC_KW | LOCAL_KW | PROTECTED_KW | FORKJOIN_KW
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
    elements(parser, Scope::Parameters);
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

    declarator(parser, if types { Init::Type } else { Init::Parameter });
    Some(parser.complete(marker, PARAM_DECL))
}

/// Parses a port list `(input logic clk, ...)`.
pub(super) fn port_list<T: Tokens>(parser: &mut Parser<T>) {
    ports(parser, false);
}

/// Parses the formals of a property, a sequence, a checker or a `let`.
pub(super) fn formal_list<T: Tokens>(parser: &mut Parser<T>) {
    ports(parser, true);
}

fn ports<T: Tokens>(parser: &mut Parser<T>, formals: bool) {
    let list = parser.start();
    parser.bump();
    elements(parser, Scope::Ports { formals });
    parser.complete(list, PORT_LIST);
}

/// Parses a single port declaration within an ANSI port list.
fn port<T: Tokens>(parser: &mut Parser<T>, formals: bool) -> Option<Completed> {
    let marker = parser.start();
    attributes(parser);

    // `.*`, in the header of a module whose `extern` prototype declared its
    // ports, takes them all.
    if parser.at(DOT) {
        parser.bump();
        if parser.at(STAR) {
            parser.bump();
        } else {
            name(parser);
        }
        if parser.at(L_PAREN) {
            arguments(parser);
        }
        return Some(parser.complete(marker, PORT));
    }

    let mut wrote = false;
    // A formal may be `local`, a variable of the sequence or property alone.
    while matches!(
        parser.kind(0),
        INPUT_KW | OUTPUT_KW | INOUT_KW | REF_KW | CONST_KW | VAR_KW
    ) || (formals && parser.at(LOCAL_KW))
    {
        parser.bump();
        wrote = true;
    }
    if parser.kind(0).is_net_type() {
        parser.bump();
        wrote = true;
    }

    // A modport names the clocking blocks it may use.
    if parser.at(CLOCKING_KW) || (formals && matches!(parser.kind(0), SEQUENCE_KW | PROPERTY_KW)) {
        parser.bump();
        wrote = true;
    } else if parser.at(INTERFACE_KW) {
        parser.bump();
        if parser.at(DOT) {
            parser.bump();
            name(parser);
        }
        wrote = true;
    } else if !at_port_name(parser) {
        wrote |= data_type(parser).is_some();
    }

    let init = match formals {
        true => Init::Actual,
        false => Init::Value,
    };
    if declarator(parser, init).is_none() && !wrote {
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

/// Parses the entries of the list `scope` names, between commas, up to and
/// including the `)` that closes it.
pub(super) fn elements<T: Tokens>(parser: &mut Parser<T>, scope: Scope) {
    let outer = parser.set_scope(scope);
    while !parser.at_end() && !parser.at(R_PAREN) {
        let at = parser.position();
        element(parser, None);
        if parser.position() == at {
            break;
        }
    }
    parser.set_scope(outer);

    if parser.at(R_PAREN) {
        parser.bump();
    }
}

/// Parses one entry of the list the scope names, falling back to verbatim
/// recovery if no rule matches, and the comma after it.
pub(super) fn element<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    // A macro among arguments stands for an expression far more often than
    // for arguments of its own.
    let preprocessed = parser.at(TICK_IDENT)
        && !(matches!(parser.scope, Scope::Arguments { .. }) && parser.macro_call().is_some())
        && preprocessor::any(parser);
    // Only an argument may be empty; a branch may open with the comma that
    // ends the entry before it.
    let empty = parser.at(COMMA) && !matches!(parser.scope, Scope::Arguments { .. });
    if !preprocessed && !empty {
        let before = parser.snapshot();
        let taken = match parser.scope {
            Scope::Parameters => param_port(parser),
            Scope::Arguments { .. } => Some(argument(parser)),
            Scope::Ports { formals } => port(parser, formals),
            _ => port(parser, false),
        };
        let ends = matches!(parser.kind(0), COMMA | R_PAREN)
            || limit.is_some_and(|limit| parser.position() >= limit);
        if taken.is_none() || !ends {
            parser.rollback(before);
            verbatim(parser, Context::Element, limit);
        }
    }
    if parser.at(COMMA) {
        parser.bump();
        // `(a, )` ends in an empty argument.
        if matches!(parser.scope, Scope::Arguments { .. }) && parser.at(R_PAREN) {
            argument(parser);
        }
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
