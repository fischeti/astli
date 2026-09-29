//! Items a project may choose to write only one way.

use astli_syntax::SyntaxKind::{
    CLASS_DECL, GENVAR_KW, INTERFACE_DECL, LOCALPARAM_KW, MINUS, MODULE_DECL, PACKAGE_DECL,
    PARAM_PORT_LIST, PARAMETER_KW, PAREN_EXPR, PROGRAM_DECL,
};
use astli_syntax::ast::{AstNode, Dimension, Expr, GenerateRegion, ModuleDecl, ParamDecl, VarDecl};
use astli_syntax::{SyntaxKind, SyntaxToken};

use crate::rule::Cx;

/// `generate` and `endgenerate` have been optional since 2005: a loop or a
/// conditional in a module is a generate construct without them.
pub(crate) fn legacy_generate_region(cx: &mut Cx) {
    for region in cx.root().descendants().filter_map(GenerateRegion::cast) {
        let Some(keyword) = region.generate_token() else {
            continue;
        };
        let diagnostic = cx
            .diagnostic(keyword.text_range(), "a `generate` region")
            .pointing("the loops and conditionals inside need no `generate`");
        cx.report(diagnostic);
    }
}

/// A `genvar` declared apart from its loop can be reused by another; one
/// declared in the loop's header, `for (genvar i = 0; ...)`, cannot.
pub(crate) fn legacy_genvar_declaration(cx: &mut Cx) {
    for decl in cx.root().descendants().filter_map(VarDecl::cast) {
        let in_header = decl
            .syntax()
            .parent()
            .is_some_and(|it| it.kind() == PAREN_EXPR);
        let Some(keyword) = token(decl.syntax(), GENVAR_KW).filter(|_| !in_header) else {
            continue;
        };
        let diagnostic = cx
            .diagnostic(
                keyword.text_range(),
                "a `genvar` declared apart from its loop",
            )
            .pointing("declare it in the loop: `for (genvar i = 0; ...)`");
        cx.report(diagnostic);
    }
}

/// One module per file, so that a module can be found by its file's name.
pub(crate) fn one_module_per_file(cx: &mut Cx) {
    let modules = (cx.root().descendants().filter_map(ModuleDecl::cast)).filter(|module| {
        !module
            .syntax()
            .ancestors()
            .skip(1)
            .any(|it| it.kind() == MODULE_DECL)
    });
    for module in modules.skip(1) {
        let Some(name) = module.name() else {
            continue;
        };
        let message = format!("module `{}` is not the file's first", name.text());
        cx.report(
            cx.diagnostic(name.text_range(), message)
                .pointing("move it to a file of its own"),
        );
    }
}

/// A `parameter` belongs in the parameter list of what it parameterizes,
/// where it can be overridden; one in a body or a package is a constant, and
/// `localparam` says so. A `localparam` belongs in a design element, a class
/// or a package, not the compilation unit.
pub(crate) fn proper_parameter_declaration(cx: &mut Cx) {
    const OWNERS: [SyntaxKind; 5] = [
        MODULE_DECL,
        INTERFACE_DECL,
        PROGRAM_DECL,
        CLASS_DECL,
        PACKAGE_DECL,
    ];
    for decl in cx.root().descendants().filter_map(ParamDecl::cast) {
        let listed = decl
            .syntax()
            .parent()
            .is_some_and(|it| it.kind() == PARAM_PORT_LIST);
        let owned = decl
            .syntax()
            .ancestors()
            .any(|it| OWNERS.contains(&it.kind()));
        let (keyword, message) =
            if let Some(keyword) = token(decl.syntax(), PARAMETER_KW).filter(|_| !listed) {
                (keyword, "`parameter` outside a parameter list")
            } else if let Some(keyword) = token(decl.syntax(), LOCALPARAM_KW).filter(|_| !owned) {
                (
                    keyword,
                    "`localparam` outside a module, interface, program, class or package",
                )
            } else {
                continue;
            };
        let pointing = match keyword.kind() {
            PARAMETER_KW => "make it a `localparam`, or move it to the parameter list",
            _ => "move it into a package",
        };
        cx.report(
            cx.diagnostic(keyword.text_range(), message)
                .pointing(pointing),
        );
    }
}

/// A negative bound, as in `[-1:0]`, is legal and almost never meant.
pub(crate) fn forbid_negative_array_dim(cx: &mut Cx) {
    for dimension in cx.root().descendants().filter_map(Dimension::cast) {
        let bounds = dimension.syntax().children().filter_map(Expr::cast);
        for bound in bounds {
            let Expr::UnaryExpr(negated) = bound else {
                continue;
            };
            let literal = matches!(negated.expr(), Some(Expr::LiteralExpr(_)));
            if negated.op().is_some_and(|op| op.kind() == MINUS) && literal {
                let range = Cx::range(negated.syntax());
                cx.report(cx.diagnostic(range, "a negative bound in a dimension"));
            }
        }
    }
}

fn token(node: &astli_syntax::SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    (node.children_with_tokens())
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == kind)
}
