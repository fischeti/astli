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
    is_strength, semicolon, type_keyword,
};
use super::delimiters;
use super::event::{Completed, Marker};
use super::expr::{
    argument, arguments, attribute, attributes, expr, lvalue, min_typ_max, parameter_values,
};
use super::property::is_concurrent;
use super::source::Tokens;
use super::stmt::{
    assignment, case_stmt, delay_control, is_immediate, label, statement_at, timing_control,
};
use super::verbatim::{Context, verbatim};
use super::{Parser, Scope, Snapshot, any, diagnostics, entries, preprocessor};
use astli_syntax::{SyntaxKind, SyntaxKind::*};

/// Parses one item at the cursor, falling back to verbatim recovery if no rule matches.
pub fn item<T: Tokens>(parser: &mut Parser<T>) {
    if super::verbatim::too_deep(parser) {
        return;
    }
    if parser.at(TICK_IDENT) && preprocessor::any(parser) {
        return;
    }
    if one(parser).is_some() {
        return;
    }
    verbatim(parser, Context::Terminated);
}

/// Attempts to parse an item, rolling back on failure.
fn one<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    let before = parser.snapshot();
    let marker = parser.start();
    attributes(parser);

    match parser.kind(0) {
        MODULE_KW | MACROMODULE_KW => shell(parser, marker, before, MODULE_DECL),
        PACKAGE_KW => shell(parser, marker, before, PACKAGE_DECL),
        PROGRAM_KW => shell(parser, marker, before, PROGRAM_DECL),
        INTERFACE_KW if parser.kind(1) != CLASS_KW => shell(parser, marker, before, INTERFACE_DECL),
        CLASS_KW | INTERFACE_KW => class(parser, marker, before),
        CHECKER_KW => checker(parser, marker, before),
        CONFIG_KW => config(parser, marker, before),
        // A prototype of a design unit, its header alone.
        EXTERN_KW if matches!(parser.kind(1), MODULE_KW | MACROMODULE_KW) => {
            shell(parser, marker, before, MODULE_DECL)
        }
        EXTERN_KW if parser.kind(1) == INTERFACE_KW => {
            shell(parser, marker, before, INTERFACE_DECL)
        }
        EXTERN_KW if parser.kind(1) == PROGRAM_KW => shell(parser, marker, before, PROGRAM_DECL),
        PRIMITIVE_KW => shell(parser, marker, before, UDP_DECL),
        EXTERN_KW if parser.kind(1) == PRIMITIVE_KW => shell(parser, marker, before, UDP_DECL),
        TABLE_KW => udp_table(parser, marker, before),
        VIRTUAL_KW if parser.kind(1) == CLASS_KW => class(parser, marker, before),

        ASSIGN_KW => continuous_assign(parser, marker, before),
        DEFPARAM_KW => defparam(parser, marker, before),
        ALIAS_KW => net_alias(parser, marker, before),
        kind if is_gate(kind) => gate_instantiation(parser, marker, before),
        ALWAYS_KW | ALWAYS_COMB_KW | ALWAYS_FF_KW | ALWAYS_LATCH_KW | INITIAL_KW | FINAL_KW => {
            procedural_block(parser, marker)
        }
        GENERATE_KW => generate_region(parser, marker, before),

        BEGIN_KW | IF_KW | FOR_KW => statement_at(parser, marker, before),
        CASE_KW | CASEX_KW | CASEZ_KW => case_stmt(parser, marker, before, true),
        // A deferred assertion stands among items, and may be labelled.
        ASSERT_KW | ASSUME_KW | COVER_KW if is_immediate(parser, 0) => {
            statement_at(parser, marker, before)
        }
        IDENT | ESCAPED_IDENT
            if parser.kind(1) == COLON && (is_immediate(parser, 2) || is_concurrent(parser, 2)) =>
        {
            statement_at(parser, marker, before)
        }
        // A generate block may be named before its `begin` as well as after.
        IDENT | ESCAPED_IDENT if parser.kind(1) == COLON && parser.kind(2) == BEGIN_KW => {
            statement_at(parser, marker, before)
        }
        _ if is_concurrent(parser, 0) => statement_at(parser, marker, before),
        PROPERTY_KW | SEQUENCE_KW => super::property::declaration_of(parser, marker, before),
        CLOCKING_KW => super::property::clocking(parser, marker, before),
        DEFAULT_KW | GLOBAL_KW if parser.kind(1) == CLOCKING_KW => {
            super::property::clocking(parser, marker, before)
        }
        DEFAULT_KW if parser.kind(1) == DISABLE_KW => {
            super::property::default_disable(parser, marker, before)
        }
        // `$error` and the like report at elaboration; a lone `;` is most
        // often written after a macro that stands for an item.
        SYSTEM_IDENT | SEMICOLON => statement_at(parser, marker, before),

        BIND_KW => bind(parser, marker, before),
        COVERGROUP_KW => super::coverage::covergroup(parser, marker, before),
        SPECIFY_KW => super::specify::block(parser, marker, before),
        CONSTRAINT_KW => constraint(parser, marker, before),
        STATIC_KW | EXTERN_KW | PURE_KW if at_constraint(parser) => {
            constraint(parser, marker, before)
        }
        MODPORT_KW => modport_decl(parser, marker, before),
        IMPORT_KW | EXPORT_KW if at_package_import(parser) => import_decl(parser, marker, before),
        TIMEUNIT_KW | TIMEPRECISION_KW => timeunit_decl(parser, marker, before),
        INPUT_KW | OUTPUT_KW | INOUT_KW | REF_KW => port_decl(parser, marker, before),

        _ if at_subroutine(parser) => subroutine(parser, marker, before),
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

/// Parses a description shell (`module`, `interface`, `program`, `package`,
/// `primitive`), or the header alone of an `extern` one.
fn shell<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    kind: SyntaxKind,
) -> Option<Completed> {
    let prototype = parser.at(EXTERN_KW);
    if prototype {
        parser.bump();
    }
    let closer = delimiters::closers(parser.kind(0))[0];
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
        return parser.decline(marker, before);
    }
    if prototype {
        return Some(parser.complete(marker, EXTERN_DECL));
    }

    entries(parser, Scope::Item, &[closer]);
    close(parser, marker, before, closer, kind)
}

/// Parses a `checker`: its name, its formals, and the items it checks with.
fn checker<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    name(parser);
    if parser.at(L_PAREN) {
        formal_list(parser);
    }
    if !semicolon(parser) {
        return parser.decline(marker, before);
    }
    entries(parser, Scope::Item, &[ENDCHECKER_KW]);
    close(parser, marker, before, ENDCHECKER_KW, CHECKER_DECL)
}

/// Parses a `config`: its name, its local parameters, the cells it starts
/// from, and the rules that bind instances and cells to libraries.
fn config<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    name(parser);
    if !semicolon(parser) {
        return parser.decline(marker, before);
    }
    entries(parser, Scope::Config, &[ENDCONFIG_KW]);
    close(parser, marker, before, ENDCONFIG_KW, CONFIG_DECL)
}

/// Parses the `design` of a configuration or one of its rules, or else an
/// item, as its local parameters are.
pub(super) fn config_item<T: Tokens>(parser: &mut Parser<T>) {
    if matches!(
        parser.kind(0),
        DESIGN_KW | DEFAULT_KW | INSTANCE_KW | CELL_KW
    ) {
        config_rule(parser);
    } else {
        item(parser);
    }
}

/// Parses the `design` a configuration starts from, or one of its rules:
/// what it applies to, `default`, an instance or a cell, then the libraries
/// to search or the cell, the configuration or the parameters to use.
fn config_rule<T: Tokens>(parser: &mut Parser<T>) {
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
        verbatim(parser, Context::Terminated);
        return;
    }
    parser.complete(marker, CONFIG_RULE);
}

/// Parses a `class` declaration.
fn class<T: Tokens>(parser: &mut Parser<T>, marker: Marker, before: Snapshot) -> Option<Completed> {
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

    entries(parser, Scope::Item, &[ENDCLASS_KW]);
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
        // Decrypted, an envelope may hold the closer: vendors encrypt a
        // module through its `endmodule`. Its header still declares it.
        if parser.at_end() && parser.enveloped(&before) {
            return Some(parser.complete(marker, kind));
        }
        return parser.decline(marker, before);
    }
    parser.bump();
    label(parser);
    Some(parser.complete(marker, kind))
}

/// Parses a `generate` … `endgenerate` block.
fn generate_region<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    entries(parser, Scope::Item, &[ENDGENERATE_KW]);
    if !parser.at(ENDGENERATE_KW) {
        return parser.decline(marker, before);
    }
    parser.bump();
    Some(parser.complete(marker, GENERATE_REGION))
}

/// Parses a procedural block (`always`, `always_comb`, `always_ff`, `always_latch`, `initial`, `final`).
fn procedural_block<T: Tokens>(parser: &mut Parser<T>, marker: Marker) -> Option<Completed> {
    parser.bump();
    let scope = parser.set_scope(Scope::Statement);
    any(parser);
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
        return parser.decline(marker, before);
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
            return parser.decline(marker, before);
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
        return parser.decline(marker, before);
    }
    Some(parser.complete(marker, DEFPARAM))
}

/// Parses `alias` and the nets it joins, two or more between `=`s.
fn net_alias<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    let mut nets = 0;
    while lvalue(parser).is_some() {
        nets += 1;
        if !parser.at(EQ) {
            break;
        }
        parser.bump();
    }
    if nets < 2 || !semicolon(parser) {
        return parser.decline(marker, before);
    }
    Some(parser.complete(marker, NET_ALIAS))
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
        return parser.decline(marker, before);
    }
    Some(parser.complete(marker, GATE_INSTANTIATION))
}

/// Parses a UDP's `table` and its rows.
fn udp_table<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    entries(parser, Scope::Table, &[ENDTABLE_KW]);
    if !parser.at(ENDTABLE_KW) {
        return parser.decline(marker, before);
    }
    parser.bump();
    Some(parser.complete(marker, UDP_TABLE))
}

/// Parses one row of a UDP's table, falling back to verbatim recovery up to
/// its `;` if it is not one.
pub(super) fn udp_row<T: Tokens>(parser: &mut Parser<T>) {
    if parser.at(TICK_IDENT) && preprocessor::any(parser) {
        return;
    }
    if !udp_entry(parser) {
        verbatim(parser, Context::Terminated);
    }
}

/// Parses a row of a UDP's table: its inputs, a colon, then one symbol, or
/// two with a colon between them for a sequential primitive, and `;`.
///
/// Which symbol may stand where depends on its text, and a row of the right
/// shape is taken whatever its symbols, with an error for each misplaced
/// one. A sequential primitive's input may be an edge, one per row, its
/// state is a level, and its output may be `-`, to keep the state.
fn udp_entry<T: Tokens>(parser: &mut Parser<T>) -> bool {
    const LEVEL: &str = "a level: `0`, `1`, `x`, `?` or `b`";
    const INPUT: &str = "a level or an edge";
    const OUTPUT: &str = "an output: `0`, `1` or `x`";
    const NEXT: &str = "an output: `0`, `1`, `x` or `-`";
    let level = |text: &str| matches!(text, "0" | "1" | "x" | "X" | "?" | "b" | "B");
    let edge = |text: &str| matches!(text, "r" | "R" | "f" | "F" | "p" | "P" | "n" | "N" | "*");
    let output = |text: &str| matches!(text, "0" | "1" | "x" | "X");
    let symbol = |kind| matches!(kind, UDP_SYMBOL | QUESTION | STAR | MINUS);

    // The shape first, read ahead: whether the primitive is sequential
    // decides what its inputs may be.
    let mut inputs = 0;
    loop {
        if symbol(parser.kind(inputs)) {
            inputs += 1;
        } else if let [L_PAREN, from, to, R_PAREN] =
            std::array::from_fn(|at| parser.kind(inputs + at))
            && symbol(from)
            && symbol(to)
        {
            inputs += 4;
        } else {
            break;
        }
    }
    let field = |at: usize| parser.kind(inputs + 2 * at) == COLON;
    let mut fields = 0;
    while fields < 2 && field(fields) && symbol(parser.kind(inputs + 2 * fields + 1)) {
        fields += 1;
    }
    if inputs == 0 || fields == 0 || parser.kind(inputs + 2 * fields) != SEMICOLON {
        return false;
    }
    let sequential = fields == 2;

    let marker = parser.start();
    let mut edges = 0;
    while !parser.at(COLON) {
        let at = parser.span();
        let is_edge = if parser.at(L_PAREN) {
            let pair = parser.start();
            parser.bump();
            table_symbol(parser, LEVEL, level);
            table_symbol(parser, LEVEL, level);
            parser.bump();
            parser.complete(pair, UDP_EDGE);
            true
        } else {
            let is_edge = sequential && edge(parser.text(0));
            table_symbol(parser, if sequential { INPUT } else { LEVEL }, |text| {
                is_edge || level(text)
            });
            is_edge
        };
        edges += usize::from(is_edge);
        if edges == 2
            && is_edge
            && let Some(at) = at
        {
            parser.events.report(diagnostics::second_edge(at));
        }
    }
    parser.bump();
    if sequential {
        table_symbol(parser, LEVEL, level);
        parser.bump();
        table_symbol(parser, NEXT, |text| output(text) || text == "-");
    } else {
        table_symbol(parser, OUTPUT, output);
    }
    parser.bump();
    parser.complete(marker, UDP_ENTRY);
    true
}

/// Takes the symbol of a table's row at the cursor, with an error if `takes`
/// does not hold for its text: its place in the row takes `expected`.
fn table_symbol<T: Tokens>(parser: &mut Parser<T>, expected: &str, takes: impl Fn(&str) -> bool) {
    let text = parser.text(0);
    if !takes(text)
        && let Some(at) = parser.span()
    {
        let misplaced = diagnostics::misplaced_table_symbol(at, text, expected);
        parser.events.report(misplaced);
    }
    parser.bump();
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
            return parser.decline(marker, before);
        }
        if parser.at(COMMA) {
            parser.bump();
        } else {
            break;
        }
    }

    if !semicolon(parser) {
        return parser.decline(marker, before);
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
        return parser.decline(marker, before);
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
            || parser.kind(1) == TICK_IDENT
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
            let list = parser.start();
            parser.bump();
            elements(parser, Scope::Modport { methods: false });
            parser.complete(list, PORT_LIST);
        }
        parser.complete(one, MODPORT);

        if parser.at(COMMA) {
            parser.bump();
        } else {
            break;
        }
    }

    if !semicolon(parser) {
        return parser.decline(marker, before);
    }
    Some(parser.complete(marker, MODPORT_DECL))
}

/// Parses a `constraint` declaration, or its prototype.
fn constraint<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
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
        super::constraint::block(parser);
    } else if !semicolon(parser) {
        return parser.decline(marker, before);
    }
    Some(parser.complete(marker, CONSTRAINT_DECL))
}

/// Parses non-ANSI port declarations (`input`, `output`, `inout`, `ref`), a
/// module's or, in its body, a function's or a task's, which may be
/// `const ref`.
pub(super) fn port_decl<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    reference(parser);
    while matches!(parser.kind(0), REF_KW | VAR_KW | CONST_KW) {
        reference(parser);
    }
    if parser.kind(0).is_net_type() {
        parser.bump();
    }
    if !at_declarator_only(parser) {
        data_type(parser);
    }
    declarators(parser, Init::Value);

    if !semicolon(parser) {
        return parser.decline(marker, before);
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
        return parser.decline(marker, before);
    }
    let inner = parser.snapshot();
    let instance = parser.start();
    if instantiation(parser, instance, inner).is_none() {
        return parser.decline(marker, before);
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
        return parser.decline(marker, before);
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
    let scoped = parser.kind(1) == COLON_COLON && matches!(parser.kind(2), IDENT | ESCAPED_IDENT);
    // A checker may be named through `$unit::`, the only system name a `::`
    // can follow.
    let mut ahead = match parser.kind(0) {
        IDENT | ESCAPED_IDENT | SYSTEM_IDENT if scoped => 3,
        IDENT | ESCAPED_IDENT => 1,
        _ => return false,
    };
    // A UDP's strength, and its delay, which may be a single value.
    if parser.kind(ahead) == L_PAREN && is_strength(parser.kind(ahead + 1)) {
        ahead = parser.past_group(ahead);
    }
    if parser.kind(ahead) == HASH {
        ahead = match parser.kind(ahead + 1) {
            L_PAREN => parser.past_group(ahead + 1),
            _ => ahead + 2,
        };
    }

    if matches!(parser.kind(ahead), IDENT | ESCAPED_IDENT) {
        ahead += 1;
        while parser.kind(ahead) == L_BRACK {
            ahead = parser.past_group(ahead);
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
        return parser.decline(marker, before);
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
    entries(parser, Scope::Statement, &[closer]);
    close(parser, marker, before, closer, kind)
}

/// Returns `true` if the cursor is at a subroutine name rather than a return type.
fn at_subroutine_name<T: Tokens>(parser: &Parser<T>) -> bool {
    if !matches!(parser.kind(0), IDENT | ESCAPED_IDENT | NEW_KW) {
        return false;
    }
    let mut ahead = 1;
    if parser.kind(ahead) == DOT && matches!(parser.kind(ahead + 1), IDENT | ESCAPED_IDENT) {
        ahead += 2;
    }
    while parser.kind(ahead) == COLON_COLON
        && matches!(parser.kind(ahead + 1), IDENT | ESCAPED_IDENT | NEW_KW)
    {
        ahead += 2;
    }
    matches!(parser.kind(ahead), L_PAREN | SEMICOLON)
}

/// Consumes a subroutine identifier, which a class scope may qualify,
/// `C::name`, or an interface port, `port.name`, for the body of a method
/// the port's interface exports.
fn subroutine_name<T: Tokens>(parser: &mut Parser<T>) {
    name(parser);
    if parser.at(DOT) {
        parser.bump();
        name(parser);
    }
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
    elements(parser, Scope::Parameters { types: false });
    parser.complete(list, PARAM_PORT_LIST);
}

/// Parses a single element within a parameter port list. One with neither a
/// keyword nor a type continues the one before it, which may have declared
/// types.
fn param_port<T: Tokens>(parser: &mut Parser<T>, types: bool) -> Option<Completed> {
    let marker = parser.start();
    // The language gives a parameter here no attribute, though a port and a
    // parameter in a body take one. Each is kept, and the parameter after it
    // parsed as if it were absent, so the one error is all it causes.
    while parser.at(L_PAREN) && parser.kind(1) == STAR {
        if let Some(at) = attribute(parser) {
            parser
                .events
                .report(diagnostics::parameter_port_attribute(at));
        }
    }
    let keyword = matches!(parser.kind(0), PARAMETER_KW | LOCALPARAM_KW);
    if keyword {
        parser.bump();
    }

    let types = if parser.at(TYPE_KW) {
        type_keyword(parser);
        true
    } else if !at_port_name(parser) {
        data_type(parser);
        false
    } else {
        types && !keyword
    };
    parser.set_scope(Scope::Parameters { types });

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
    port_after_attributes(parser, marker, formals)
}

/// Parses one entry of a modport: a port, a clocking block it may use, or a
/// subroutine it imports or exports, which an entry without a keyword of its
/// own takes from the entry before it.
fn modport_entry<T: Tokens>(parser: &mut Parser<T>, methods: bool) -> Option<Completed> {
    let marker = parser.start();
    attributes(parser);
    let methods = match parser.kind(0) {
        IMPORT_KW | EXPORT_KW => true,
        INPUT_KW | OUTPUT_KW | INOUT_KW | REF_KW | CLOCKING_KW => false,
        _ => methods,
    };
    parser.set_scope(Scope::Modport { methods });
    if !methods {
        return port_after_attributes(parser, marker, false);
    }

    if matches!(parser.kind(0), IMPORT_KW | EXPORT_KW) {
        parser.bump();
    }
    if matches!(parser.kind(0), FUNCTION_KW | TASK_KW) {
        let task = parser.at(TASK_KW);
        parser.bump();
        if !task && !at_subroutine_name(parser) {
            data_type(parser);
        }
    }
    let at = parser.position();
    name(parser);
    if parser.position() == at {
        parser.abandon(marker);
        return None;
    }
    if parser.at(L_PAREN) {
        port_list(parser);
    }
    Some(parser.complete(marker, MODPORT_METHOD))
}

/// Parses the rest of a port, after the attributes `marker` opens.
fn port_after_attributes<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    formals: bool,
) -> Option<Completed> {
    // `default` among a constructor's arguments, from SystemVerilog 2023,
    // stands for those of the base class's constructor.
    if parser.at(DEFAULT_KW) && matches!(parser.kind(1), COMMA | R_PAREN) && !formals {
        parser.bump();
        return Some(parser.complete(marker, PORT));
    }
    let mut wrote = false;
    // A formal may be `local`, a variable of the sequence or property alone.
    while matches!(
        parser.kind(0),
        INPUT_KW | OUTPUT_KW | INOUT_KW | REF_KW | CONST_KW | VAR_KW
    ) || (formals && parser.at(LOCAL_KW))
    {
        reference(parser);
        wrote = true;
    }

    // A non-ANSI port may join several signals, `{a, b}`.
    if parser.at(L_BRACE) && !wrote && !formals {
        lvalue(parser);
        return Some(parser.complete(marker, PORT));
    }

    // `.name(signal)` names a port apart from what it connects to, after a
    // direction in a header or a modport. `.*`, in the header of a module
    // whose `extern` prototype declared its ports, takes them all.
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

/// Consumes a port's keyword, and the `static` after a `ref`, which
/// SystemVerilog 2023 adds for an argument bound only to a variable that
/// outlives the call.
fn reference<T: Tokens>(parser: &mut Parser<T>) {
    let reference = parser.at(REF_KW);
    parser.bump();
    if reference && parser.at(STATIC_KW) {
        parser.bump();
    }
}

/// Returns `true` if the cursor is at a port name rather than a port type:
/// one that ends the list, or the conditional branch it is the last port of.
fn at_port_name<T: Tokens>(parser: &Parser<T>) -> bool {
    (matches!(parser.kind(0), IDENT | ESCAPED_IDENT) && matches!(parser.kind(1), R_PAREN | EOF))
        || at_declarator_only(parser)
}

/// Parses the entries of the list `scope` names, between commas, up to and
/// including the `)` that closes it.
///
/// Not through [`entries`]: what [`any`] does with an entry nested too deep
/// is take it up to a `;`, past the `)`, while an entry's own recovery stops
/// at the `,` or `)` after it.
pub(super) fn elements<T: Tokens>(parser: &mut Parser<T>, scope: Scope) {
    let outer = parser.set_scope(scope);
    while !parser.at_end() && !parser.at(R_PAREN) {
        let at = parser.position();
        element(parser);
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
pub(super) fn element<T: Tokens>(parser: &mut Parser<T>) {
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
            Scope::Parameters { types } => param_port(parser, types),
            Scope::Arguments { values } => Some(argument(parser, values)),
            Scope::Ports { formals } => port(parser, formals),
            Scope::Modport { methods } => modport_entry(parser, methods),
            _ => port(parser, false),
        };
        let ends = matches!(parser.kind(0), COMMA | R_PAREN | EOF);
        if taken.is_none() || !ends {
            parser.rollback(before);
            verbatim(parser, Context::Element);
        }
    }
    if parser.at(COMMA) {
        parser.bump();
        // `(a, )` ends in an empty argument.
        if let Scope::Arguments { values } = parser.scope
            && parser.at(R_PAREN)
        {
            argument(parser, values);
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
