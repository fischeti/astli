//! Declarations, and what the `=` in one means.

use astli_syntax::SyntaxKind::{
    ATTRIBUTES, AUTOMATIC_KW, BLOCK, CHECKER_DECL, CLASS_DECL, COLON_COLON, CONST_KW,
    FUNCTION_DECL, FUNCTION_KW, GENVAR_KW, INTERFACE_DECL, MODULE_DECL, PACKAGE_DECL,
    PROCEDURAL_BLOCK, PROGRAM_DECL, STATIC_KW, TASK_DECL, TASK_KW, VAR_KW,
};
use astli_syntax::ast::{AstNode, VarDecl};
use astli_syntax::{SyntaxNode, SyntaxToken};
use rowan::{NodeOrToken, TextRange};

use crate::rule::Cx;
use crate::rules::generate::scope;

/// A variable's `=` in a design element sets its value once, before time
/// starts, and nothing drives it after: simulation keeps the value, and
/// synthesis for silicon, which has no time zero, drops it. A net's `=`
/// looks the same and is a continuous assignment.
///
/// A name that is a user-defined `nettype` is taken for a variable's type:
/// telling them apart needs the name resolved.
pub(crate) fn variable_initializer(cx: &mut Cx) {
    for decl in cx.root().descendants().filter_map(VarDecl::cast) {
        // A `genvar`'s `=` is its loop's start, and no variable's either.
        let net = decl
            .syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .any(|token| token.kind().is_net_type() || token.kind() == GENVAR_KW);
        let design = scope(decl.syntax())
            .is_some_and(|it| matches!(it.kind(), MODULE_DECL | INTERFACE_DECL));
        if net || !design {
            continue;
        }
        for declarator in decl.declarators() {
            let (Some(name), Some(_)) = (declarator.name(), declarator.init()) else {
                continue;
            };
            let message = format!(
                "`{}` has an initial value, which synthesis ignores",
                name.text()
            );
            let diagnostic = cx
                .diagnostic(Cx::range(declarator.syntax()), message)
                .pointing("make it a `localparam`, or drive it with `assign`");
            cx.report(diagnostic);
        }
    }
}

/// A variable declared in a procedural block or a routine is static unless
/// it, the routine or the design element holding them says otherwise, so
/// its `=` sets it once, before time starts, rather than each time the block
/// is entered. It reads as the second, so the standard asks for `static` to
/// say the first was meant.
pub(crate) fn explicit_static(cx: &mut Cx) {
    for decl in cx.root().descendants().filter_map(VarDecl::cast) {
        let node = decl.syntax();
        // Declared in a block or a routine's body, and not, say, a `for`
        // loop's header, whose variable is automatic.
        let procedural = (node.parent())
            .is_some_and(|it| matches!(it.kind(), BLOCK | FUNCTION_DECL | TASK_DECL));
        let lifetime =
            (node.children_with_tokens()).any(|it| matches!(it.kind(), STATIC_KW | AUTOMATIC_KW));
        if !procedural || lifetime || !static_by_default(node) {
            continue;
        }
        // The lifetime goes after `const` and `var`, and before the type.
        let at = (node.children_with_tokens())
            .filter(|it| !it.kind().is_trivia())
            .find(|it| !matches!(it.kind(), ATTRIBUTES | CONST_KW | VAR_KW))
            .and_then(|it| match it {
                NodeOrToken::Token(token) => Some(token),
                NodeOrToken::Node(node) => significant(&node).next(),
            })
            .map(|token| TextRange::empty(token.text_range().start()));
        let mut fix = at;
        for declarator in decl.declarators() {
            let (Some(name), Some(_)) = (declarator.name(), declarator.init()) else {
                continue;
            };
            let message = format!(
                "`{}` is static, so it is given its value once, not on each entry",
                name.text()
            );
            let mut diagnostic = cx
                .diagnostic(Cx::range(declarator.syntax()), message)
                .pointing("write `static` to say so, or `automatic` to set it on each entry");
            // One `static` covers every variable the declaration names.
            if let Some(at) = fix.take() {
                diagnostic = diagnostic.fix(cx.span(at), "static ");
            }
            cx.report(diagnostic);
        }
    }
}

/// Whether a variable declared in the block or routine `node` is in is
/// static unless it says otherwise. A routine's lifetime is its own, if it
/// says one, and a method's is automatic; otherwise the design element's
/// that holds the block or routine, which is static unless it says
/// `automatic`.
fn static_by_default(node: &SyntaxNode) -> bool {
    let Some(scope) = (node.ancestors())
        .find(|it| matches!(it.kind(), PROCEDURAL_BLOCK | FUNCTION_DECL | TASK_DECL))
    else {
        return false;
    };
    if scope.kind() != PROCEDURAL_BLOCK {
        // A `static` before the keyword is a method's qualifier; the
        // lifetime follows the keyword.
        let lifetime = (significant(&scope).filter(|it| it.parent().as_ref() == Some(&scope)))
            .skip_while(|it| !matches!(it.kind(), FUNCTION_KW | TASK_KW))
            .nth(1);
        match lifetime.map(|it| it.kind()) {
            Some(AUTOMATIC_KW) => return false,
            Some(STATIC_KW) => return true,
            _ => {}
        }
        // A method defined out of its class, as `C::f`.
        if (scope.children_with_tokens()).any(|it| it.kind() == COLON_COLON) {
            return false;
        }
    }
    let unit = (scope.ancestors()).find(|it| {
        matches!(
            it.kind(),
            CLASS_DECL | MODULE_DECL | INTERFACE_DECL | PROGRAM_DECL | PACKAGE_DECL | CHECKER_DECL
        )
    });
    match unit {
        Some(unit) if unit.kind() == CLASS_DECL => false,
        Some(unit) => !(unit.children_with_tokens()).any(|it| it.kind() == AUTOMATIC_KW),
        None => true,
    }
}

/// The tokens under `node` that are not trivia.
fn significant(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    (node.descendants_with_tokens())
        .filter_map(|it| it.into_token())
        .filter(|it| !it.kind().is_trivia())
}
