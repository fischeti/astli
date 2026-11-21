//! The attributes that make a module a test, and its parameters swept, read
//! off a file's tree as written: what each stands on and is given is
//! [`points`](crate::points)'s to refuse.
//!
//! `points` reads a package's tests off each file's raw tree, which has
//! every branch of a conditional and no macro expanded, and a test's sweeps
//! off the expanded tree its design is analysed from.

use astli_index::{Declares, Location};
use astli_parse::Parsed;
use astli_syntax::SyntaxKind::{
    ATTRIBUTE_SPEC, ATTRIBUTES, CHECKER_DECL, CLASS_DECL, CONDITIONAL_BRANCH, CONDITIONAL_REGION,
    DECLARATOR, EQ, ESCAPED_IDENT, IDENT, INTERFACE_DECL, LITERAL_EXPR, MODULE_DECL, PACKAGE_DECL,
    PARAM_DECL, PROGRAM_DECL, STRING_LITERAL, UDP_DECL,
};
use astli_syntax::{
    SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, name_text, significant_child_tokens,
};
use astli_text::Origins;

/// The marks on a module, interface, program, package or class declared at
/// the top level, one that carries any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Marks {
    pub name: String,
    pub declares: Declares,
    /// Where its name is written, which tells it from another declaration
    /// of the name.
    pub location: Location,
    /// `(* astli_test *)` on it, which makes a module a test.
    pub test: Option<TestMark>,
    /// Each `astli_sweep` on a parameter declared in its body, in the order
    /// written.
    pub sweeps: Vec<SweepMark>,
    /// `(* astli_should_fail = "text" *)` on it, which makes a test pass
    /// only by failing with the text in its output.
    pub should_fail: Option<FailMark>,
}

/// An `astli_test` attribute on a declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TestMark {
    /// Where the attribute's name is written.
    pub location: Location,
    /// Whether it is given a value, which it takes none of.
    pub valued: bool,
}

/// An `astli_should_fail` attribute on a declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FailMark {
    /// Where the attribute's name is written.
    pub location: Location,
    /// The text between its string's quotes; `None` if it is given no
    /// string.
    pub text: Option<String>,
}

/// An `astli_sweep` attribute on a parameter declared in the body of a
/// declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SweepMark {
    /// Where the attribute's name is written.
    pub location: Location,
    /// The names the declaration it stands on declares, in order.
    pub parameters: Vec<String>,
    pub values: SweepValues,
}

/// What an `astli_sweep` is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SweepValues {
    /// Nothing, for every value of the parameter's type.
    Every,
    /// A string, by the text between its quotes.
    Listed(String),
    /// Something other than a string.
    NotAString,
}

/// The attribute that marks a test.
const TEST: &str = "astli_test";

/// The attribute that lists a parameter's values.
const SWEEP: &str = "astli_sweep";

/// The attribute that makes a test pass by failing.
const SHOULD_FAIL: &str = "astli_should_fail";

/// The marks on each declaration at the top level of `parsed`, in the order
/// written, those in every branch of a raw tree's conditionals included.
/// Of several of one attribute on one declaration the last counts, as for
/// any attribute.
///
/// An `extern` declaration has no body to run or sweep, and one in text
/// the grammar left `VERBATIM` does not parse, so neither carries marks.
pub(crate) fn marks(origins: &Origins, parsed: &Parsed) -> Vec<Marks> {
    let locate = |token: &SyntaxToken| {
        let span = parsed.span(token)?;
        let at = origins.spelled(origins.reported_at(span));
        Some(Location {
            path: origins.path(at.src_id)?.to_path_buf(),
            at: origins.line_col(at.src_id, at.start),
        })
    };

    let mut declarations = Vec::new();
    top_level(&parsed.root, &mut declarations);
    let mut found = Vec::new();
    for (declaration, declares) in declarations {
        let Some(name) = significant_child_tokens(&declaration)
            .find(|token| matches!(token.kind(), IDENT | ESCAPED_IDENT))
        else {
            continue;
        };
        let Some(location) = locate(&name) else {
            continue;
        };
        let test = (attributes(&declaration, TEST).last()).and_then(|(first, spec)| {
            Some(TestMark {
                location: locate(&first)?,
                valued: spec.children_with_tokens().any(|it| it.kind() == EQ),
            })
        });
        let should_fail =
            (attributes(&declaration, SHOULD_FAIL).last()).and_then(|(first, spec)| {
                Some(FailMark {
                    location: locate(&first)?,
                    text: given(&spec).and_then(|it| string(&it)),
                })
            });
        let sweeps: Vec<SweepMark> = (sweeps(&declaration).into_iter())
            .filter_map(|(first, parameters, values)| {
                Some(SweepMark {
                    location: locate(&first)?,
                    parameters,
                    values,
                })
            })
            .collect();
        if test.is_none() && should_fail.is_none() && sweeps.is_empty() {
            continue;
        }
        found.push(Marks {
            name: name_text(name.text()).to_string(),
            declares,
            location,
            test,
            sweeps,
            should_fail,
        });
    }
    found
}

/// The declarations among the items of `node`, and among those of each
/// branch of a conditional in it, which raw mode keeps as written.
fn top_level(node: &SyntaxNode, found: &mut Vec<(SyntaxNode, Declares)>) {
    for item in node.children() {
        match item.kind() {
            CONDITIONAL_REGION | CONDITIONAL_BRANCH => top_level(&item, found),
            kind => found.extend(declares(kind).map(|declares| (item, declares))),
        }
    }
}

/// What a declaration node of `kind` declares, if it is one.
fn declares(kind: SyntaxKind) -> Option<Declares> {
    Some(match kind {
        MODULE_DECL => Declares::Module,
        INTERFACE_DECL => Declares::Interface,
        PROGRAM_DECL => Declares::Program,
        PACKAGE_DECL => Declares::Package,
        CLASS_DECL => Declares::Class,
        CHECKER_DECL => Declares::Checker,
        UDP_DECL => Declares::Primitive,
        _ => return None,
    })
}

/// What the attribute `spec` is given, after its `=`.
fn given(spec: &SyntaxNode) -> Option<SyntaxElement> {
    (spec.children_with_tokens())
        .skip_while(|it| it.kind() != EQ)
        .skip(1)
        .find(|it| !it.kind().is_trivia())
}

/// The text between the quotes of `given`, if it is a string alone.
fn string(given: &SyntaxElement) -> Option<String> {
    let node = given.as_node().filter(|node| node.kind() == LITERAL_EXPR)?;
    let mut tokens = significant_child_tokens(node);
    match (tokens.next(), tokens.next()) {
        (Some(token), None) if token.kind() == STRING_LITERAL => {
            let text = token.text().strip_prefix('"')?;
            Some(text.strip_suffix('"').unwrap_or(text).to_string())
        }
        _ => None,
    }
}

/// Each `astli_sweep` attribute on a parameter declared in the body of
/// `declaration`: the attribute's name, the names the parameter
/// declaration declares, and what the attribute is given.
fn sweeps(declaration: &SyntaxNode) -> Vec<(SyntaxToken, Vec<String>, SweepValues)> {
    let mut parameters = Vec::new();
    body_parameters(declaration, &mut parameters);
    (parameters.into_iter())
        .filter_map(|parameter| {
            let (first, spec) = attributes(&parameter, SWEEP).last()?;
            let names = (parameter.children())
                .filter(|node| node.kind() == DECLARATOR)
                .filter_map(|declarator| {
                    (declarator.children_with_tokens())
                        .filter_map(|element| element.into_token())
                        .find(|token| token.kind() == IDENT)
                })
                .map(|token| token.text().to_string())
                .collect();
            let values = match given(&spec) {
                None => SweepValues::Every,
                Some(given) => string(&given).map_or(SweepValues::NotAString, SweepValues::Listed),
            };
            Some((first, names, values))
        })
        .collect()
}

/// The parameter declarations among the items of `body`, and among those
/// of each branch of a conditional in it, which raw mode keeps as written.
fn body_parameters(body: &SyntaxNode, found: &mut Vec<SyntaxNode>) {
    for item in body.children() {
        match item.kind() {
            PARAM_DECL => found.push(item),
            CONDITIONAL_REGION | CONDITIONAL_BRANCH => body_parameters(&item, found),
            _ => {}
        }
    }
}

/// Each attribute called `name` among those `node` carries, in the order
/// written: its name's token, and the attribute.
fn attributes<'a>(
    node: &SyntaxNode,
    name: &'a str,
) -> impl Iterator<Item = (SyntaxToken, SyntaxNode)> + 'a {
    let specs = (node.children())
        .filter(|node| node.kind() == ATTRIBUTES)
        .flat_map(|attributes| attributes.children())
        .filter(|node| node.kind() == ATTRIBUTE_SPEC);
    specs.filter_map(move |spec| {
        let first = significant_child_tokens(&spec).next()?;
        (first.kind() == IDENT && first.text() == name).then_some((first, spec))
    })
}

#[cfg(test)]
mod tests {
    use astli_parse::{parse, parse_expanded};
    use astli_preproc::{MacroTable, Session};

    use super::*;

    /// The marks in `text`, a file `top.sv`: expanded, and as written.
    fn read(text: &str) -> (Vec<Marks>, Vec<Marks>) {
        let mut session = Session::new();
        let file = session.add("top.sv", text.to_string());
        let tokens = session.expand(file).tokens;
        let expanded = marks(session.origins(), &parse_expanded(&session, &tokens));
        let raw = parse(&session, file, MacroTable::new());
        (expanded, marks(session.origins(), &raw))
    }

    #[test]
    fn astli_test_marks_a_declaration_in_either_mode() {
        // Read as written, a value or a mark on what is no module included,
        // which is for the reader to refuse.
        let text = "(* astli_test *)\nmodule tb_a;\nendmodule\n\
                    (* other, astli_test = 1 *) module tb_b; endmodule\n\
                    module plain;\nendmodule\n\
                    `ifdef FAST\n(* astli_test *) interface tb_i; endinterface\n`endif\n";
        let tests = |marks: Vec<Marks>| {
            (marks.into_iter())
                .map(|it| {
                    let test = (it.test).map(|t| (t.location.to_string(), t.valued));
                    (it.name, it.declares, test)
                })
                .collect::<Vec<_>>()
        };
        let test = |name: &str, declares, at: &str, valued| {
            (name.to_string(), declares, Some((at.to_string(), valued)))
        };

        let (expanded, raw) = read(text);
        let written = vec![
            test("tb_a", Declares::Module, "top.sv:1:4", false),
            test("tb_b", Declares::Module, "top.sv:4:11", true),
        ];
        assert_eq!(tests(expanded), written);
        let mut every = written;
        every.push(test("tb_i", Declares::Interface, "top.sv:8:4", false));
        assert_eq!(tests(raw), every);
    }

    #[test]
    fn astli_should_fail_reads_its_text_as_written() {
        // Without a string, or on what is no test, it is for the reader to
        // refuse; on a prototype, which has no body to run, it is not read.
        let text = "(* astli_test, astli_should_fail = \"[OVF]\" *)\nmodule tb_a;\nendmodule\n\
                    (* astli_should_fail *) module tb_b; endmodule\n\
                    (* astli_should_fail = 1 *) module tb_c; endmodule\n\
                    (* astli_should_fail = \"x\" *) extern module tb_d;\n";
        let fails: Vec<_> = (read(text).0.into_iter())
            .map(|it| {
                let fail = (it.should_fail).map(|f| (f.location.to_string(), f.text));
                (it.name, fail)
            })
            .collect();
        let fail = |name: &str, at: &str, text: Option<&str>| {
            let at = Some((at.to_string(), text.map(str::to_string)));
            (name.to_string(), at)
        };

        assert_eq!(
            fails,
            [
                fail("tb_a", "top.sv:1:16", Some("[OVF]")),
                fail("tb_b", "top.sv:4:4", None),
                fail("tb_c", "top.sv:5:4", None),
            ]
        );
    }

    #[test]
    fn astli_sweep_marks_a_parameter_of_a_body_in_either_mode() {
        // Read as written, on a `localparam` or a declaration of two
        // included; one in a parameter port list, which the parser refuses,
        // is not.
        let text = "module tb #((* astli_sweep *) parameter int P = 0);\n\
                    \x20 (* astli_sweep = \"1, 2\" *) parameter int Depth = 4;\n\
                    \x20 (* other *) (* astli_sweep *) localparam bit B = 0, C = 1;\n\
                    \x20 (* astli_sweep = 3 *) parameter int N = 1;\n\
                    `ifdef FAST\n  (* astli_sweep *) parameter bit F = 0;\n`endif\n\
                    \x20 parameter int Plain = 0;\n\
                    endmodule\n";
        let sweeps = |marks: Vec<Marks>| -> Vec<(String, Vec<String>, SweepValues)> {
            (marks.into_iter())
                .flat_map(|it| it.sweeps)
                .map(|s| (s.location.to_string(), s.parameters, s.values))
                .collect()
        };
        let sweep = |at: &str, names: &[&str], values: SweepValues| {
            let names = names.iter().map(|it| it.to_string()).collect();
            (at.to_string(), names, values)
        };
        let written = vec![
            sweep("top.sv:2:6", &["Depth"], SweepValues::Listed("1, 2".into())),
            sweep("top.sv:3:18", &["B", "C"], SweepValues::Every),
            sweep("top.sv:4:6", &["N"], SweepValues::NotAString),
        ];

        let (expanded, raw) = read(text);
        assert_eq!(sweeps(expanded), written);
        let mut every = written;
        every.push(sweep("top.sv:6:6", &["F"], SweepValues::Every));
        assert_eq!(sweeps(raw), every);
    }
}
