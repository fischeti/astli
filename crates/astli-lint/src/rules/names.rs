//! How declarations are named, after lowRISC's style guide.
//!
//! Each style is written out by hand rather than as a pattern, which keeps
//! a regex engine out of the crate. An escaped identifier is left alone: it
//! was named on purpose, whatever the style.

use astli_syntax::SyntaxKind::{CONSTRAINT_KW, IDENT, TICK_IDENT, TYPE_KW};
use astli_syntax::ast::{AstNode, DataType, InterfaceDecl, ParamDecl, Typedef};
use astli_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::rule::Cx;

/// A parameter is `CamelCase`, or `ALL_CAPS` as C constants are. A type
/// parameter is named as a type.
pub(crate) fn parameter_name_style(cx: &mut Cx) {
    let decls = cx.root().descendants().filter_map(ParamDecl::cast);
    for decl in decls.filter(|decl| token(decl.syntax(), TYPE_KW).is_none()) {
        for name in decl.declarators().filter_map(|it| it.name()) {
            if name.kind() == IDENT && !camel_case(name.text()) && !all_caps(name.text()) {
                let message = format!("parameter `{name}` is neither CamelCase nor ALL_CAPS");
                cx.report(cx.diagnostic(name.text_range(), message));
            }
        }
    }
}

/// A parameter that turns something on reads without a double negative:
/// `EnableParity = 1`, not `DisableParity = 0`.
pub(crate) fn positive_meaning_parameter_name(cx: &mut Cx) {
    let decls = cx.root().descendants().filter_map(ParamDecl::cast);
    for name in decls
        .flat_map(|it| it.declarators())
        .filter_map(|it| it.name())
    {
        let text = name.text();
        let negative = text
            .get(..7)
            .is_some_and(|it| it.eq_ignore_ascii_case("disable"));
        if negative {
            let message = format!("parameter `{text}` names what it turns off");
            cx.report(
                cx.diagnostic(name.text_range(), message)
                    .pointing("name what it turns on, as `Enable...`"),
            );
        }
    }
}

/// A macro is `ALL_CAPS`. UVM's own are `uvm_` in lower case, which a
/// testbench's may follow.
pub(crate) fn macro_name_style(cx: &mut Cx) {
    for directive in cx.root().descendants() {
        let Some(name) = after(&directive, TICK_IDENT, "`define") else {
            continue;
        };
        let text = name.text();
        let uvm = text.strip_prefix("uvm_").is_some_and(lower_snake_case);
        if !all_caps(text) && !uvm {
            let message = format!("macro `{text}` is not ALL_CAPS");
            cx.report(cx.diagnostic(name.text_range(), message));
        }
    }
}

/// An enum type is `lower_snake_case`, ending in `_e`, or `_t` as any type.
pub(crate) fn enum_name_style(cx: &mut Cx) {
    for (name, ty) in typedefs(cx.root()) {
        let text = name.text();
        let suffixed = text.strip_suffix("_e").or(text.strip_suffix("_t"));
        if matches!(ty, DataType::EnumType(_)) && !suffixed.is_some_and(lower_snake_case) {
            let message =
                format!("enum type `{text}` is not lower_snake_case ending in `_e` or `_t`");
            cx.report(cx.diagnostic(name.text_range(), message));
        }
    }
}

/// A struct or union type is `lower_snake_case`, ending in `_t`.
pub(crate) fn struct_union_name_style(cx: &mut Cx) {
    for (name, ty) in typedefs(cx.root()) {
        let what = match ty {
            DataType::StructType(_) => "struct",
            DataType::UnionType(_) => "union",
            _ => continue,
        };
        let text = name.text();
        if !text.strip_suffix("_t").is_some_and(lower_snake_case) {
            let message = format!("{what} type `{text}` is not lower_snake_case ending in `_t`");
            cx.report(cx.diagnostic(name.text_range(), message));
        }
    }
}

/// An interface is `lower_snake_case`, ending in `_if`.
pub(crate) fn interface_name_style(cx: &mut Cx) {
    let interfaces = cx.root().descendants().filter_map(InterfaceDecl::cast);
    for name in interfaces
        .filter_map(|it| it.name())
        .filter(|it| it.kind() == IDENT)
    {
        let text = name.text();
        if !text.strip_suffix("_if").is_some_and(lower_snake_case) {
            let message = format!("interface `{text}` is not lower_snake_case ending in `_if`");
            cx.report(cx.diagnostic(name.text_range(), message));
        }
    }
}

/// A constraint is `lower_snake_case`, ending in `_c`, with no empty word:
/// `size_c`, not `_c` or `size__c`.
pub(crate) fn constraint_name_style(cx: &mut Cx) {
    for decl in cx.root().descendants() {
        let Some(name) = after(&decl, CONSTRAINT_KW, "constraint") else {
            continue;
        };
        let text = name.text();
        let words = text.strip_suffix("_c").map(|stem| stem.split('_'));
        let named = words
            .is_some_and(|mut words| words.all(|word| !word.is_empty() && lower_snake_case(word)));
        if !named {
            let message = format!("constraint `{text}` is not lower_snake_case ending in `_c`");
            cx.report(cx.diagnostic(name.text_range(), message));
        }
    }
}

/// Each `typedef` under `root` that names a type it declares: its name, and
/// the type.
fn typedefs(root: &SyntaxNode) -> impl Iterator<Item = (SyntaxToken, DataType)> {
    root.descendants()
        .filter_map(Typedef::cast)
        .filter_map(|typedef| {
            let name = typedef
                .declarator()?
                .name()
                .filter(|it| it.kind() == IDENT)?;
            Some((name, typedef.data_type()?))
        })
}

/// The identifier right after `node`'s own `keyword` token spelled `text`,
/// such as the name a `` `define `` defines.
fn after(node: &SyntaxNode, keyword: SyntaxKind, text: &str) -> Option<SyntaxToken> {
    let mut tokens = (node.children_with_tokens())
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia());
    tokens.find(|token| token.kind() == keyword && token.text() == text)?;
    tokens.next().filter(|token| token.kind() == IDENT)
}

fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    (node.children_with_tokens())
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == kind)
}

/// `lower_snake_case`: lower-case letters, digits and underscores.
fn lower_snake_case(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// `ALL_CAPS`: upper-case letters, digits and underscores.
fn all_caps(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

/// `CamelCase`: letters and digits, starting with an upper-case letter or a
/// digit, so `AXI4Lite` passes and `numPorts` or `Num_Ports` do not. It may
/// end in an underscore and digits, as in `InitHash_256`.
fn camel_case(name: &str) -> bool {
    let stem = match name.rsplit_once('_') {
        Some((stem, digits))
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) =>
        {
            stem
        }
        _ => name,
    };
    let mut bytes = stem.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && bytes.all(|b| b.is_ascii_alphanumeric())
}
