//! Types written out: of parameters and arguments, of enums, and the order
//! of a range.

use astli_syntax::SyntaxKind::{
    DIMENSION, ENUM_KW, FUNCTION_DECL, INOUT_KW, INPUT_KW, INT_LITERAL, OUTPUT_KW, PORT_DECL,
    PORT_LIST, REF_KW, SIGNED_KW, STRING_LITERAL, TASK_DECL, TYPE_KW, TYPEDEF, UNSIGNED_KW,
};
use astli_syntax::ast::{AstNode, DataType, Declarator, EnumType, Expr, ParamDecl};
use astli_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::rule::Cx;

/// A parameter without a type takes the type of whatever overrides it, so
/// `Width = 8` becomes a 1-bit parameter when an instance passes `1'b1`. A
/// string value with no type is the Verilog way, which lowRISC keeps.
pub(crate) fn explicit_parameter_storage_type(cx: &mut Cx) {
    for decl in cx.root().descendants().filter_map(ParamDecl::cast) {
        if token(decl.syntax(), TYPE_KW).is_some() || explicit(decl.syntax()) {
            continue;
        }
        for declarator in decl.declarators() {
            let string = declarator.init().is_some_and(|init| {
                let first = (init.syntax().descendants_with_tokens())
                    .filter_map(|element| element.into_token())
                    .find(|token| !token.kind().is_trivia());
                first.is_some_and(|token| token.kind() == STRING_LITERAL)
            });
            let Some(name) = declarator.name().filter(|_| !string) else {
                continue;
            };
            let message = format!("parameter `{name}` without a type");
            cx.report(
                cx.diagnostic(name.text_range(), message)
                    .pointing("give it one, as `int` or `logic [7:0]`"),
            );
        }
    }
}

/// An argument without a type is a 1-bit `logic`, which is rarely what an
/// argument named anything but a flag is meant to be. One with neither a
/// direction nor a type takes the one before it's, and is fine after one
/// with a type. Arguments declared in the body count as those in the header.
pub(crate) fn explicit_function_task_parameter_type(cx: &mut Cx) {
    let routines =
        (cx.root().descendants()).filter(|it| matches!(it.kind(), FUNCTION_DECL | TASK_DECL));
    for routine in routines {
        let header = (routine.children().filter(|it| it.kind() == PORT_LIST))
            .flat_map(|list| list.children());
        let body = routine.children().filter(|it| it.kind() == PORT_DECL);
        let mut typed_before = false;
        for port in header.chain(body) {
            let typed = explicit(&port);
            let directed = (port.children_with_tokens())
                .filter_map(|element| element.into_token())
                .any(|token| matches!(token.kind(), INPUT_KW | OUTPUT_KW | INOUT_KW | REF_KW));
            let inherits = !directed && !typed && typed_before;
            if !typed && !inherits {
                let names = port
                    .children()
                    .filter_map(Declarator::cast)
                    .filter_map(|it| it.name());
                for name in names {
                    let message = format!("argument `{name}` without a type");
                    cx.report(
                        cx.diagnostic(name.text_range(), message)
                            .pointing("give it one, as `logic` or `int`"),
                    );
                }
            }
            typed_before = typed || inherits;
        }
    }
}

/// An `enum` without a `typedef` has a type no other declaration can name,
/// so nothing else can hold its values without a cast.
pub(crate) fn typedef_enums(cx: &mut Cx) {
    for ty in cx.root().descendants().filter_map(EnumType::cast) {
        let named = ty.syntax().parent().is_some_and(|it| it.kind() == TYPEDEF);
        let Some(keyword) = token(ty.syntax(), ENUM_KW).filter(|_| !named) else {
            continue;
        };
        let diagnostic = cx
            .diagnostic(keyword.text_range(), "an `enum` without a `typedef`")
            .pointing("declare it as a type: `typedef enum ... name_e;`");
        cx.report(diagnostic);
    }
}

/// A packed range runs from its most significant bit down, `[7:0]`, so
/// that bit 0 is the least significant. `[0:N-1]` is read as a bug.
pub(crate) fn packed_dimensions_range_ordering(cx: &mut Cx) {
    for (dimension, left, right) in ranges(cx.root(), true) {
        let ascending = match (value(&left), value(&right)) {
            (Some(left), Some(right)) => left < right,
            (Some(0), None) => true,
            _ => false,
        };
        if ascending {
            let range = Cx::range(&dimension);
            cx.report(
                cx.diagnostic(range, "a packed range in ascending order")
                    .pointing("write it `[high:low]`"),
            );
        }
    }
}

/// An unpacked range runs from 0 up, `[0:N-1]`, or is a size, `[N]`, as an
/// array is indexed in software.
pub(crate) fn unpacked_dimensions_range_ordering(cx: &mut Cx) {
    for (dimension, left, right) in ranges(cx.root(), false) {
        let descending = match (value(&left), value(&right)) {
            (Some(left), Some(right)) => left > right,
            (None, Some(0)) => true,
            _ => false,
        };
        if descending {
            let range = Cx::range(&dimension);
            let diagnostic = cx
                .diagnostic(range, "an unpacked range in descending order")
                .pointing("write it `[0:N-1]`, or as a size, `[N]`");
            cx.report(diagnostic);
        }
    }
}

/// The `[left:right]` ranges under `root`, of packed dimensions or unpacked
/// ones. A dimension in a type is packed; one after a name is unpacked.
fn ranges(root: &SyntaxNode, packed: bool) -> impl Iterator<Item = (SyntaxNode, Expr, Expr)> {
    root.descendants()
        .filter(|it| it.kind() == DIMENSION)
        .filter_map(move |dimension| {
            let parent = dimension.parent()?;
            let in_type = DataType::can_cast(parent.kind());
            let after_name = Declarator::can_cast(parent.kind());
            (if packed { in_type } else { after_name }).then_some(())?;
            let mut bounds = dimension.children().filter_map(Expr::cast);
            let (left, right) = (bounds.next()?, bounds.next()?);
            bounds.next().is_none().then_some((dimension, left, right))
        })
}

/// The value of a bound written as a plain decimal number.
fn value(bound: &Expr) -> Option<u64> {
    let Expr::LiteralExpr(literal) = bound else {
        return None;
    };
    let mut tokens = (literal.syntax().children_with_tokens())
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia());
    let number = tokens.next().filter(|it| it.kind() == INT_LITERAL)?;
    if tokens.next().is_some() {
        return None;
    }
    number.text().replace('_', "").parse().ok()
}

/// Whether `node` has a type of its own: a data type naming one, not only
/// `signed` or a packed range.
fn explicit(node: &SyntaxNode) -> bool {
    node.children().filter_map(DataType::cast).any(|ty| {
        (ty.syntax().children_with_tokens())
            .filter(|element| element.kind() != DIMENSION)
            .filter_map(|element| element.into_token())
            .any(|token| {
                !token.kind().is_trivia() && !matches!(token.kind(), SIGNED_KW | UNSIGNED_KW)
            })
            || !matches!(ty, DataType::TypeRef(_))
    })
}

fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    (node.children_with_tokens())
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == kind)
}
