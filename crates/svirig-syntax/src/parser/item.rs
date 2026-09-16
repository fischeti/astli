//! Descriptions, and the items that live inside one.
//!
//! A module, an interface, a program and a package are one shape with four
//! names: a keyword, a header, a body, and the `end…` that matches. So there
//! is one rule for the shell and a table of closers, and what differs between
//! them -- a package has no ports, a class has `extends` -- is written where
//! it differs and nowhere else.
//!
//! # All or nothing, again
//!
//! A shell that never finds its own `end…` was misread, and it gives every
//! token back rather than closing a node over half a file. That matters for
//! the number M3 is graded on as much as for the tree: a [`MODULE_DECL`] over
//! a module *and everything after it* would lower the verbatim rate by being
//! wrong, which is the one way the metric could lie.
//!
//! # `foo bar (…)` is not `foo bar;`
//!
//! An instantiation and a declaration are the same two identifiers, and what
//! separates them is the `(` after the second one -- a question about shape,
//! which a parser can answer, rather than about what `foo` means, which
//! [`decl`](super::decl) explains it cannot. So an instantiation is
//! recognised by looking past the optional `#(…)` and the optional instance
//! array for that parenthesis, and a declaration is what is left.
//!
//! # What stays verbatim on purpose
//!
//! Concurrent assertions, `specify` sections, covergroups, sequences and the
//! inside of a constraint are not here. They are large, they are rare in RTL,
//! and a formatter that leaves them exactly as written is doing the right
//! thing until someone asks otherwise -- which is the fallback earning its
//! keep rather than a gap in it. See `docs/plan.md`.
//!
//! A `constraint` still gets a shell, and that is not a contradiction: its
//! body is braced rather than terminated by a `;`, and the fallback reads a
//! closing bracket as no boundary at all, so without the shell the run would
//! carry on past the `}` and swallow the member after it.

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
use crate::{SyntaxKind, SyntaxKind::*};

/// Parses one item at the cursor, falling back where no rule fits.
///
/// Always takes at least one token unless the cursor is at the end or already
/// at `limit`.
pub fn item<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) {
    if parser.at(TICK_IDENT) && preprocessor::any(parser) {
        return;
    }
    if one(parser, limit).is_some() {
        return;
    }
    verbatim(parser, Context::Terminated, limit);
}

/// One item, or `None` with the cursor and the events put back.
///
/// The node is opened before the dispatch rather than by the rule chosen,
/// because an item's attributes are written in front of the keyword that says
/// what the item is -- so something has to be open before anything is known.
fn one<T: Tokens>(parser: &mut Parser<T>, limit: Option<Position>) -> Option<Completed> {
    let before = parser.snapshot();
    let marker = parser.start();
    attributes(parser);

    match parser.kind(0) {
        MODULE_KW | MACROMODULE_KW => shell(parser, marker, before, MODULE_DECL, limit),
        PACKAGE_KW => shell(parser, marker, before, PACKAGE_DECL, limit),
        PROGRAM_KW => shell(parser, marker, before, PROGRAM_DECL, limit),
        // `interface class C;` is a class, and `endclass` closes it.
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

        // A generate loop, conditional or block is the statement rule over an
        // item body -- which is what the scope here already says it is.
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

/// Gives back the tokens and the events, for a rule that could not finish.
fn decline<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.abandon(marker);
    parser.rollback(before);
    None
}

/// What closes a shell of `kind`.
fn closer(kind: SyntaxKind) -> SyntaxKind {
    match kind {
        INTERFACE_DECL => ENDINTERFACE_KW,
        PROGRAM_DECL => ENDPROGRAM_KW,
        PACKAGE_DECL => ENDPACKAGE_KW,
        CLASS_DECL => ENDCLASS_KW,
        _ => ENDMODULE_KW,
    }
}

/// `module m #(…) (…); … endmodule`, and the three descriptions written the
/// same way.
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

    // `module m import pkg::*; #(…) (…);` -- an import list may stand between
    // the name and the parameter ports, and nothing else may.
    //
    // Each import gets a snapshot of its own. Handing it the shell's would
    // have one that never found its `;` roll the *module* back, across a
    // marker still open -- which is a panic rather than a bad tree, because
    // `Events` counts open markers precisely so that this cannot pass
    // silently.
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

/// `class C extends B implements I; … endclass`.
fn class<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    limit: Option<Position>,
) -> Option<Completed> {
    // `virtual class C` and `interface class C` alike: a word in front of the
    // keyword that says which kind of class, and closes with the same
    // `endclass` either way.
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
        // `extends B(a, b)` passes the arguments its constructor takes.
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

/// Takes the `end…` that closes a shell, or gives the whole shell back.
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

/// Everything up to the `end…`, in `scope`.
///
/// The scope is set here rather than by the caller because it is the body
/// that has one: a description holds items and a subroutine holds statements,
/// whatever was being parsed around them.
///
/// The progress check is the loop's own safety net rather than a claim about
/// the rules: one that took nothing would spin here forever, and a `break`
/// leaves the rest to the fallback.
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

/// `generate` … `endgenerate`, whose contents are items like any others.
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

/// `always_ff @(posedge clk) …`, and the five other keywords written the same
/// way.
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

/// `assign #1 a = b, c = d;`.
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

/// `import pkg::*, pkg::name;`, and the `export` that mirrors it.
fn import_decl<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    parser.bump();
    loop {
        // `export *::*;` re-exports whatever was imported.
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

/// Whether the cursor is on a package import rather than on a DPI one.
///
/// `import "DPI-C" function void f();` shares the keyword and declares a
/// subroutine; the quoted string is what says so.
fn at_package_import<T: Tokens>(parser: &Parser<T>) -> bool {
    matches!(parser.kind(0), IMPORT_KW | EXPORT_KW)
        && (parser.kind(1) == STAR
            || (matches!(parser.kind(1), IDENT | ESCAPED_IDENT) && parser.kind(2) == COLON_COLON))
}

/// `modport controller (input a, output b), peripheral (…);`.
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

/// `constraint name { … }`, and the `constraint name;` a prototype writes.
///
/// The body is not parsed and is not meant to be. What the rule buys is the
/// *bound*: a braced body ends at its `}` and carries no `;`, and the
/// fallback -- which reads a closing bracket as no boundary at all, so that
/// `(a + b) + c` carries on -- would otherwise run past it and swallow the
/// member after it.
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

    let end = parser.ahead(past_group(parser, 0, L_BRACE, R_BRACE) as u32);
    while !parser.at_end() && parser.position() < end {
        let at = parser.position();
        verbatim(parser, Context::Terminated, Some(end));
        if parser.position() == at {
            break;
        }
    }
    Some(parser.complete(marker, CONSTRAINT_DECL))
}

/// `input logic [7:0] a, b;` -- a port declared as an item, which is how a
/// non-ANSI header says what its ports are.
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

/// `foo #(.W(8)) u_foo (.a(x)), u_bar (.a(y));`.
fn instantiation<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
) -> Option<Completed> {
    // The type being instantiated. A `TYPE_REF` rather than a bare token, so
    // that a formatter finds the name in the same place it finds a
    // declaration's.
    let ty = parser.start();
    parser.bump();
    parser.complete(ty, TYPE_REF);

    // `#(…)` overrides parameters, which is the same shape as a call's
    // arguments and named the same way.
    if parser.at(HASH) && parser.kind(1) == L_PAREN {
        parser.bump();
        arguments(parser);
    }

    loop {
        let one = parser.start();
        name(parser);
        // `u_foo [3:0] (…)` instantiates an array of them.
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

/// Whether the cursor is on a type being instantiated.
///
/// A name, optional parameter overrides, a second name, optional instance
/// dimensions, and then a `(`. The parenthesis is the whole of the test: a
/// declaration never has one there, and an instantiation always does.
fn at_instantiation<T: Tokens>(parser: &Parser<T>) -> bool {
    if !matches!(parser.kind(0), IDENT | ESCAPED_IDENT) {
        return false;
    }

    let mut ahead = 1;
    if parser.kind(ahead) == HASH {
        // `#5` is a delay, not a parameter override.
        if parser.kind(ahead + 1) != L_PAREN {
            return false;
        }
        ahead = past_group(parser, ahead + 1, L_PAREN, R_PAREN);
    }

    if !matches!(parser.kind(ahead), IDENT | ESCAPED_IDENT) {
        return false;
    }
    ahead += 1;
    while parser.kind(ahead) == L_BRACK {
        ahead = past_group(parser, ahead, L_BRACK, R_BRACK);
    }

    parser.kind(ahead) == L_PAREN
}

/// The index just past the bracket group opening at `ahead`.
///
/// Answers the end of the tokens on a group that never closes, so that a
/// caller's loop terminates on malformed input rather than on trust.
fn past_group<T: Tokens>(
    parser: &Parser<T>,
    ahead: usize,
    open: SyntaxKind,
    close: SyntaxKind,
) -> usize {
    let mut depth = 0u32;
    let mut at = ahead;
    loop {
        let kind = parser.kind(at);
        if kind == open {
            depth += 1;
        } else if kind == close {
            depth -= 1;
        } else if kind == EOF {
            return at;
        }
        at += 1;
        if depth == 0 {
            return at;
        }
    }
}

/// Whether the cursor is on a subroutine, whatever qualifies it.
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

/// `function int f(input int a); … endfunction`, the `task` written the same
/// way, and the prototype forms of both.
fn subroutine<T: Tokens>(
    parser: &mut Parser<T>,
    marker: Marker,
    before: Snapshot,
    limit: Option<Position>,
) -> Option<Completed> {
    // `extern`, `pure` and a DPI import all promise a body somewhere else, so
    // the declaration ends at its own `;` and there is no `end…` to look for.
    let mut prototype = false;

    if matches!(parser.kind(0), IMPORT_KW | EXPORT_KW) && parser.kind(1) == STRING_LITERAL {
        prototype = true;
        parser.bump();
        parser.bump();
        while matches!(parser.kind(0), CONTEXT_KW | PURE_KW) {
            parser.bump();
        }
        // `import "DPI-C" c_name = function void f();` gives the foreign name
        // separately from the SystemVerilog one.
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

    // A `task` returns nothing, so what follows it is the name. A `function`
    // may write a return type first, and the same question decides it as
    // decides a declaration: is the first name the type, or the thing being
    // named?
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

/// Whether the cursor is on a subroutine's name rather than on its return
/// type.
///
/// Both are an identifier, and `function pkg::t f();` writes one of each. What
/// separates them is what comes after the `::` chain: a name is followed by
/// its ports or by the `;`, and a type by the name it qualifies.
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

/// The name a subroutine is given, which may name the class it belongs to.
fn subroutine_name<T: Tokens>(parser: &mut Parser<T>) {
    name(parser);
    while parser.at(COLON_COLON) {
        parser.bump();
        name(parser);
    }
}

/// `#( parameter W = 8, type T = int )`.
fn param_port_list<T: Tokens>(parser: &mut Parser<T>) {
    let list = parser.start();
    // The `#` and the `(` both: the two are one introducer here, unlike the
    // `#` of a delay, which stands on its own.
    parser.bump();
    parser.bump();
    elements(parser, param_port);
    parser.complete(list, PARAM_PORT_LIST);
}

/// One element of a [`PARAM_PORT_LIST`], whose `parameter` keyword may be left
/// out once the first has written it.
fn param_port<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    let marker = parser.start();
    if matches!(parser.kind(0), PARAMETER_KW | LOCALPARAM_KW) {
        parser.bump();
    }

    // `parameter type T = int;` gives a *type* a default rather than a value.
    let types = parser.at(TYPE_KW);
    if types {
        parser.bump();
    } else if !at_port_name(parser) {
        data_type(parser);
    }

    // One name, not a list: the `,` in `#(parameter int A = 1, B = 2)`
    // separates elements of *this* list, and a declarator loop would eat it
    // and then find a keyword where the next name should be. `B = 2` is an
    // element in its own right, which is the same tokens and the same tree.
    declarator(parser, types);
    Some(parser.complete(marker, PARAM_DECL))
}

/// The `( … )` of a header, a `modport` or a subroutine.
fn port_list<T: Tokens>(parser: &mut Parser<T>) {
    let list = parser.start();
    parser.bump();
    elements(parser, port);
    parser.complete(list, PORT_LIST);
}

/// One element of a [`PORT_LIST`]: a direction, a type and a name, in
/// whatever combination the form allows -- all three of them optional, since
/// a non-ANSI header writes only names and a list inherits what the element
/// before it wrote.
fn port<T: Tokens>(parser: &mut Parser<T>) -> Option<Completed> {
    let marker = parser.start();
    attributes(parser);

    // `.name(expr)` -- a port whose external name differs from what is wired
    // to it, and the named connection of an instance.
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

    // `interface.modport p` takes an interface of any kind, which is the one
    // place the bare keyword names a type.
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

/// Whether the cursor is on a port's name rather than on its type.
///
/// The same question [`at_declarator_only`] answers for a declaration, with
/// the `)` that ends a list added to the things a name may be followed by.
fn at_port_name<T: Tokens>(parser: &Parser<T>) -> bool {
    (matches!(parser.kind(0), IDENT | ESCAPED_IDENT) && parser.kind(1) == R_PAREN)
        || at_declarator_only(parser)
}

/// A comma-separated list up to its `)`, each element `one` or, where that
/// declines, a verbatim run bounded by the comma after it.
///
/// The fallback is what makes a list safe to parse at all: one element the
/// rules cannot shape costs that element and nothing else, where a list rule
/// that gave up would cost the header and the body under it.
fn elements<T: Tokens>(parser: &mut Parser<T>, one: impl Fn(&mut Parser<T>) -> Option<Completed>) {
    while !parser.at_end() && !parser.at(R_PAREN) {
        let before = parser.snapshot();
        let at = parser.position();

        if !(parser.at(TICK_IDENT) && preprocessor::any(parser)) {
            let taken = one(parser).is_some();
            // An element has to end where the next one begins. One that did
            // not is an element some rule misread, and the tokens are worth
            // more as a run than as a node over part of them.
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

/// `static` or `automatic`, which either may be written and neither has to
/// be.
fn lifetime<T: Tokens>(parser: &mut Parser<T>) {
    if matches!(parser.kind(0), STATIC_KW | AUTOMATIC_KW) {
        parser.bump();
    }
}

/// One name, which a macro may write in place of.
fn name<T: Tokens>(parser: &mut Parser<T>) {
    if parser.at(TICK_IDENT) {
        preprocessor::any(parser);
    } else if matches!(parser.kind(0), IDENT | ESCAPED_IDENT | NEW_KW) {
        parser.bump();
    }
}
