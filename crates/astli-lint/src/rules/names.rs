//! How declarations are named, after lowRISC's style guide by default.
//!
//! A project may name its own style: the cases a name is in, and patterns
//! around it, `*_t`, rather than a regex, which keeps a regex engine out of
//! the crate. An escaped identifier is left alone: it was named on purpose,
//! whatever the style.

use astli_syntax::SyntaxKind::{
    CLASS_DECL, CONSTRAINT_KW, IDENT, INOUT_KW, INPUT_KW, INTERFACE_DECL, MODULE_DECL, OUTPUT_KW,
    PACKAGE_DECL, PORT, PORT_DECL, PORT_LIST, PROGRAM_DECL, REF_KW, TICK_IDENT, TYPE_KW, VAR_DECL,
};
use astli_syntax::ast::{
    AstNode, DataType, Declarator, Instantiation, InterfaceDecl, ParamDecl, Typedef,
};
use astli_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::options::{Case, Style};
use crate::rule::Cx;

/// A parameter is `CamelCase`, or `ALL_CAPS` as C constants are. A type
/// parameter is named as a type.
pub(crate) fn parameter_name_style(cx: &mut Cx) {
    let style = cx.style();
    let decls = cx.root().descendants().filter_map(ParamDecl::cast);
    for decl in decls.filter(|decl| token(decl.syntax(), TYPE_KW).is_none()) {
        for name in decl.declarators().filter_map(|it| it.name()) {
            if name.kind() == IDENT && !style.holds(name.text()) {
                let message = format!("parameter `{name}` {}", style.describe());
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
    let style = cx.style();
    for directive in cx.root().descendants() {
        let Some(name) = after(&directive, TICK_IDENT, "`define") else {
            continue;
        };
        let text = name.text();
        let uvm = (text.strip_prefix("uvm_")).is_some_and(|it| Case::LowerSnake.holds(it));
        if !style.holds(text) && !uvm {
            let message = format!("macro `{text}` {}", style.describe());
            cx.report(cx.diagnostic(name.text_range(), message));
        }
    }
}

/// An enum type is `lower_snake_case`, ending in `_e`, or `_t` as any type.
pub(crate) fn enum_name_style(cx: &mut Cx) {
    let style = cx.style();
    for (name, ty) in typedefs(cx.root()) {
        let text = name.text();
        if matches!(ty, DataType::EnumType(_)) && !style.holds(text) {
            let message = format!("enum type `{text}` {}", style.describe());
            cx.report(cx.diagnostic(name.text_range(), message));
        }
    }
}

/// A struct or union type is `lower_snake_case`, ending in `_t`.
pub(crate) fn struct_union_name_style(cx: &mut Cx) {
    let style = cx.style();
    for (name, ty) in typedefs(cx.root()) {
        let what = match ty {
            DataType::StructType(_) => "struct",
            DataType::UnionType(_) => "union",
            _ => continue,
        };
        let text = name.text();
        if !style.holds(text) {
            let message = format!("{what} type `{text}` {}", style.describe());
            cx.report(cx.diagnostic(name.text_range(), message));
        }
    }
}

/// An interface is `lower_snake_case`, ending in `_if`.
pub(crate) fn interface_name_style(cx: &mut Cx) {
    let style = cx.style();
    let interfaces = cx.root().descendants().filter_map(InterfaceDecl::cast);
    for name in interfaces
        .filter_map(|it| it.name())
        .filter(|it| it.kind() == IDENT)
    {
        let text = name.text();
        if !style.holds(text) {
            let message = format!("interface `{text}` {}", style.describe());
            cx.report(cx.diagnostic(name.text_range(), message));
        }
    }
}

/// A constraint is `lower_snake_case`, ending in `_c`, with no empty word:
/// `size_c`, not `_c` or `size__c`.
pub(crate) fn constraint_name_style(cx: &mut Cx) {
    let style = cx.style();
    for decl in cx.root().descendants() {
        let Some(name) = after(&decl, CONSTRAINT_KW, "constraint") else {
            continue;
        };
        let text = name.text();
        let named = (style.stems(text))
            .any(|stem| style.cased(stem) && stem.split('_').all(|word| !word.is_empty()));
        if !named {
            let message = format!("constraint `{text}` {}", style.describe());
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

/// A signal (a net, a variable or a port of a module, interface or
/// program) is `lower_snake_case`. Variables in classes are verification
/// code, which this rule leaves alone.
pub(crate) fn signal_name_style(cx: &mut Cx) {
    let style = cx.style();
    let declarations = cx.root().descendants().filter(|it| match it.kind() {
        VAR_DECL => in_design(it),
        PORT_DECL => it.parent().is_some_and(|it| DESIGN.contains(&it.kind())),
        PORT => port_of_design(it) && !listed_only(it),
        _ => false,
    });
    let names = declarations.flat_map(|it| it.children().filter_map(Declarator::cast));
    for name in names
        .filter_map(|it| it.name())
        .filter(|it| it.kind() == IDENT)
    {
        if !style.holds(name.text()) {
            let message = format!("signal `{name}` {}", style.describe());
            cx.report(cx.diagnostic(name.text_range(), message));
        }
    }
}

/// An instance is `lower_snake_case`, as a signal is.
pub(crate) fn instance_name_style(cx: &mut Cx) {
    let style = cx.style();
    let instances = cx.root().descendants().filter_map(Instantiation::cast);
    for name in instances
        .flat_map(|it| it.instances())
        .filter_map(|it| it.name())
    {
        if name.kind() == IDENT && !style.holds(name.text()) {
            let message = format!("instance `{name}` {}", style.describe());
            cx.report(cx.diagnostic(name.text_range(), message));
        }
    }
}

/// A port says its direction in its name: by default `_i`, `_o` or `_io`,
/// after `n` for active low or either half of a differential pair, or `p`
/// for the other half, as `rst_ni` or `lvds_po`. A port with a type but no
/// direction may be an interface's, so it is left alone.
pub(crate) fn port_name_suffix(cx: &mut Cx) {
    let [input, output, inout] = ["input", "output", "inout"].map(|key| Style {
        cases: &[],
        patterns: cx.patterns(key),
    });
    let owners = cx
        .root()
        .descendants()
        .filter(|it| DESIGN.contains(&it.kind()));
    for owner in owners {
        let header =
            (owner.children().filter(|it| it.kind() == PORT_LIST)).flat_map(|it| it.children());
        let body = owner.children().filter(|it| it.kind() == PORT_DECL);
        let mut direction = None;
        for port in header
            .filter(|it| it.kind() == PORT && !listed_only(it))
            .chain(body)
        {
            let own = (port.children_with_tokens())
                .filter_map(|element| element.into_token())
                .find(|token| matches!(token.kind(), INPUT_KW | OUTPUT_KW | INOUT_KW | REF_KW));
            let typed = port.children().any(|it| DataType::can_cast(it.kind()));
            direction = match (own, typed) {
                (Some(own), _) => Some(own.kind()),
                (None, true) => None,
                (None, false) => direction,
            };
            let style = match direction {
                Some(INPUT_KW) => input,
                Some(OUTPUT_KW) => output,
                Some(INOUT_KW) => inout,
                _ => continue,
            };
            let names = port
                .children()
                .filter_map(Declarator::cast)
                .filter_map(|it| it.name());
            for name in names.filter(|it| it.kind() == IDENT) {
                if !style.holds(name.text()) {
                    let message = format!("port `{name}` {}", style.describe());
                    cx.report(cx.diagnostic(name.text_range(), message));
                }
            }
        }
    }
}

/// Design elements, whose nets, variables and ports are signals.
const DESIGN: [SyntaxKind; 3] = [MODULE_DECL, INTERFACE_DECL, PROGRAM_DECL];

/// Whether `node` stands in a design element rather than a class, or a
/// package or the compilation unit outside one.
fn in_design(node: &SyntaxNode) -> bool {
    let scope = (node.ancestors().skip(1))
        .find(|it| DESIGN.contains(&it.kind()) || matches!(it.kind(), CLASS_DECL | PACKAGE_DECL));
    scope.is_some_and(|it| DESIGN.contains(&it.kind()))
}

/// Whether `port` is a design element's, not a function's or a task's.
fn port_of_design(port: &SyntaxNode) -> bool {
    let owner = port
        .parent()
        .filter(|it| it.kind() == PORT_LIST)
        .and_then(|it| it.parent());
    owner.is_some_and(|it| DESIGN.contains(&it.kind()))
}

/// Whether `port` only names a port whose direction and type the body
/// declares, as in a Verilog-1995 header: no direction and no type, first
/// in its list.
fn listed_only(port: &SyntaxNode) -> bool {
    let first = port
        .parent()
        .and_then(|list| list.children().find(|it| it.kind() == PORT));
    let bare = |it: &SyntaxNode| {
        !it.children().any(|it| DataType::can_cast(it.kind()))
            && !(it.children_with_tokens())
                .filter_map(|element| element.into_token())
                .any(|token| matches!(token.kind(), INPUT_KW | OUTPUT_KW | INOUT_KW | REF_KW))
    };
    first.is_some_and(|first| bare(&first))
}

#[cfg(test)]
mod tests {
    use astli_parse::SyntaxTree;

    use crate::{Config, Level, lint};

    /// What `rule` alone finds in `source`, with `options` set.
    fn found(rule: &str, options: &[(&str, &[&str])], source: &str) -> Vec<String> {
        let mut config = Config::default();
        config.set("correctness", Level::Allow).unwrap();
        config.set("suspicious", Level::Allow).unwrap();
        config.set("lowrisc", Level::Allow).unwrap();
        config.set(rule, Level::Warn).unwrap();
        for (key, values) in options {
            config.set_option(rule, key, values).unwrap();
        }
        let tree = SyntaxTree::parse("top.sv", source.to_string());
        (lint(&tree, &config).into_iter())
            .map(|it| it.message)
            .collect()
    }

    #[test]
    fn ports_may_say_their_direction_first() {
        let source =
            "module top (input logic i_a, input logic a_i, output logic o_b);\nendmodule\n";
        let options: &[(&str, &[&str])] = &[("input", &["i_*", "ni_*"]), ("output", &["o_*"])];
        assert_eq!(
            found("port-name-suffix", options, source),
            ["port `a_i` does not start with `i_` or `ni_`"]
        );
    }

    #[test]
    fn instances_may_take_a_prefix() {
        let source = "module top;\n  sub u_sub ();\n  sub sub ();\nendmodule\n";
        assert_eq!(
            found("instance-name-style", &[("patterns", &["u_*"])], source),
            ["instance `sub` is not lower_snake_case starting with `u_`"]
        );
    }

    #[test]
    fn signals_may_be_camel_case() {
        let source = "module top;\n  logic dataValid;\n  logic data_valid;\nendmodule\n";
        assert_eq!(
            found("signal-name-style", &[("case", &["camelCase"])], source),
            ["signal `data_valid` is not camelCase"]
        );
    }

    #[test]
    fn an_option_must_be_declared() {
        let mut config = Config::default();
        let err = config.set_option("signal-name-style", "suffix", &["*_q"]);
        assert_eq!(
            err.unwrap_err().to_string(),
            "`signal-name-style` has no option `suffix`, only `case` and `patterns`"
        );
        let err = config.set_option("forbid-reg", "case", &["ALL_CAPS"]);
        assert_eq!(err.unwrap_err().to_string(), "`forbid-reg` has no options");
        let err = config.set_option("signal-name-style", "case", &["snake"]);
        assert!(
            err.unwrap_err()
                .to_string()
                .starts_with("`case`: `snake` is not a case")
        );
    }
}
