//! Declarations, and what the `=` in one means.

use astli_syntax::SyntaxKind::{GENVAR_KW, INTERFACE_DECL, MODULE_DECL};
use astli_syntax::ast::{AstNode, VarDecl};

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
