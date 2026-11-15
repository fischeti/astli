//! What one file declares and uses at the top level, read off its tree.

use std::fmt;
use std::path::{Path, PathBuf};

use astli_parse::Parsed;
use astli_preproc::Session;
use astli_syntax::SyntaxKind::{
    ATTRIBUTE_SPEC, ATTRIBUTES, CONDITIONAL_BRANCH, CONDITIONAL_REGION, DECLARATOR, ENVELOPE, EQ,
    IDENT, INT_LITERAL, LITERAL_EXPR, PARAM_DECL, STRING_LITERAL,
};
use astli_syntax::{SyntaxNode, SyntaxToken};
use astli_text::{LineCol, SourceId};

use crate::names::{Role, declares, names, package_checkers, stub};

/// What a top-level declaration declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Declares {
    Module,
    Interface,
    Program,
    Package,
    Class,
    Checker,
    Primitive,
}

/// How a reference uses the name it names, which says what it may resolve to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Uses {
    /// The type of an instantiation: a module, interface or program.
    Instance,
    /// The package of an `import` or `export`.
    Import,
    /// The name left of `::`: a package, or a class that may be local.
    Scope,
    /// The head of a type: an interface, a class, or a typedef that may be
    /// local.
    Type,
    /// The target of a `bind`, or the head of its path: a module, an
    /// interface, or an instance. `held` if a design element holds the
    /// `bind`, which then applies once that one is elaborated rather than
    /// wherever its file is compiled.
    Bind { held: bool },
    /// A name in text the grammar left `VERBATIM`, shaped like an
    /// instantiation's type.
    Unparsed,
}

impl Uses {
    /// Whether the name must be declared at the top level of some file, so
    /// that finding it nowhere is a problem.
    pub fn is_global(self) -> bool {
        matches!(self, Uses::Instance | Uses::Import)
    }
}

/// A file an `` `include `` read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Include {
    pub path: PathBuf,
    /// The name a quoted `include` searched for, as it expanded, which a
    /// compiler given the include directories looks for too; `None` for
    /// `` `include <name> ``.
    pub name: Option<String>,
    /// Where the directive is: in the file, or in a header it read.
    pub at: Location,
}

/// Where a name is written: the file, and the line and column in it. A name
/// a macro wrote is where the outermost call was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    pub at: LineCol,
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.path.display(), self.at)
    }
}

/// A module, interface, program, package or class declared at the top level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub name: String,
    pub declares: Declares,
    pub location: Location,
    /// Whether it is an `extern` declaration, its ports and parameters
    /// alone, which a declaration with its body stands in for.
    pub prototype: bool,
    /// `(* astli_test *)` on it, which makes a module a test.
    pub test: Option<TestMark>,
    /// Each `astli_sweep` on a parameter declared in its body, in the order
    /// written.
    pub sweeps: Vec<SweepMark>,
    /// `(* astli_plusargs = "..." *)` on it, the plusargs a test runs under.
    pub plusargs: Option<AttributeMark>,
}

/// An attribute on a declaration and what it is given, as written: whether
/// it is the right thing on the right declaration is the reader's to
/// refuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeMark {
    /// Where the attribute's name is written.
    pub location: Location,
    pub given: Given,
}

/// What an attribute is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Given {
    Nothing,
    /// A string, by the text between its quotes.
    String(String),
    /// An integer literal, as written.
    Number(String),
    /// Anything else.
    Other,
}

/// An `astli_test` attribute on a declaration, as written: one with a value,
/// or on what is no module, is the reader's to refuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestMark {
    /// Where the attribute's name is written.
    pub location: Location,
    /// Whether it is given a value, which it takes none of.
    pub valued: bool,
}

/// An `astli_sweep` attribute on a parameter declared in the body of a
/// declaration, as written: what it stands on and what it is given are the
/// reader's to refuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SweepMark {
    /// Where the attribute's name is written.
    pub location: Location,
    /// The names the declaration it stands on declares, in order.
    pub parameters: Vec<String>,
    pub values: SweepValues,
}

/// What an `astli_sweep` is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SweepValues {
    /// Nothing, for every value of the parameter's type.
    Every,
    /// A string, by the text between its quotes.
    Listed(String),
    /// Something other than a string.
    NotAString,
}

/// A name used where it can only mean something declared at the top level,
/// or might.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub name: String,
    pub uses: Uses,
    pub location: Location,
}

/// The top-level names one file declares and uses, and the headers it read.
///
/// It holds no span, so it outlives the session it was read from and can be
/// sent across threads: an [`Index`](crate::Index) is built from summaries
/// made in parallel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub path: PathBuf,
    /// In the order they are written, headers included where they are.
    pub declarations: Vec<Declaration>,
    /// Every use, in the order written; one name may be used many times.
    pub references: Vec<Reference>,
    /// The checkers the packages declare, in the order written, which an
    /// import makes a name an instance may use.
    pub checkers: Vec<String>,
    /// Every file an `` `include `` read, directly or through another, in the
    /// order read.
    pub includes: Vec<Include>,
    /// Whether the file holds an encrypted envelope, which may declare
    /// anything: the declarations above are only those written outside one.
    pub encrypted: bool,
    /// Whether the file holds `extern` declarations and nothing else: a
    /// stub for definitions a library supplies, whose ports a check reads
    /// but which a tool given the library needs none of.
    pub stub: bool,
}

impl Summary {
    /// Reads the summary of `file` off `parsed`, its tree in either mode.
    ///
    /// In expanded mode what the macros write and the headers declare is part
    /// of it, and only the branches the build takes. In raw mode neither is,
    /// and every branch is.
    pub fn new(session: &Session, file: SourceId, parsed: &Parsed) -> Summary {
        let origins = session.origins();
        let locate = |token: &SyntaxToken| {
            let span = parsed.span(token)?;
            let at = origins.spelled(origins.reported_at(span));
            Some(Location {
                path: origins.path(at.src_id)?.to_path_buf(),
                at: origins.line_col(at.src_id, at.start),
            })
        };

        let mut declarations = Vec::new();
        let mut references = Vec::new();
        for name in names(&parsed.root) {
            let Some(location) = locate(&name.token) else {
                continue;
            };
            match name.role {
                Role::Declares(declares) | Role::Announces(declares) => {
                    let prototype = matches!(name.role, Role::Announces(_));
                    let test = (test_attribute(&name.token))
                        .filter(|_| !prototype)
                        .and_then(|(token, valued)| {
                            Some(TestMark {
                                location: locate(&token)?,
                                valued,
                            })
                        });
                    let sweeps = match prototype {
                        true => Vec::new(),
                        false => (sweep_attributes(&name.token).into_iter())
                            .filter_map(|(token, parameters, values)| {
                                Some(SweepMark {
                                    location: locate(&token)?,
                                    parameters,
                                    values,
                                })
                            })
                            .collect(),
                    };
                    let valued = |attribute: &str| {
                        let (token, given) =
                            valued_attribute(&name.token, attribute).filter(|_| !prototype)?;
                        Some(AttributeMark {
                            location: locate(&token)?,
                            given,
                        })
                    };
                    declarations.push(Declaration {
                        name: name.text().to_string(),
                        declares,
                        location,
                        prototype,
                        test,
                        sweeps,
                        plusargs: valued(PLUSARGS),
                    })
                }
                Role::Uses(uses) => references.push(Reference {
                    name: name.text().to_string(),
                    uses,
                    location,
                }),
                Role::Closes(_) => {}
            }
        }

        let path = origins
            .path(file)
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let includes = origins
            .files()
            .filter(|&other| {
                let root = origins.include_trace(other).last();
                root.is_some_and(|site| origins.spelled(site).src_id == file)
            })
            .filter_map(|other| {
                let site = origins.spelled(origins.included_from(other)?);
                Some(Include {
                    path: origins.path(other)?.to_path_buf(),
                    name: origins.included_as(other).map(str::to_string),
                    at: Location {
                        path: origins.path(site.src_id)?.to_path_buf(),
                        at: origins.line_col(site.src_id, site.start),
                    },
                })
            })
            .collect();

        Summary {
            path,
            declarations,
            references,
            checkers: package_checkers(&parsed.root),
            includes,
            encrypted: encrypted(&parsed.root),
            stub: stub(&parsed.root),
        }
    }
}

/// Whether the tree holds an encrypted envelope.
fn encrypted(root: &SyntaxNode) -> bool {
    (root.descendants_with_tokens()).any(|element| element.kind() == ENVELOPE)
}

/// The attribute that marks a test.
const TEST: &str = "astli_test";

/// The attribute that lists a parameter's values.
const SWEEP: &str = "astli_sweep";

/// The attribute that lists the plusargs a test runs under.
const PLUSARGS: &str = "astli_plusargs";

/// The `astli_test` attribute on the declaration `name` names, if any: the
/// attribute's name, and whether it is given a value. Of several the last
/// counts, as for any attribute.
///
/// A declaration in text the grammar left `VERBATIM` has no attributes to
/// read; it does not parse, so it is no test either.
fn test_attribute(name: &SyntaxToken) -> Option<(SyntaxToken, bool)> {
    let declaration = (name.parent_ancestors()).find(|node| declares(node.kind()).is_some())?;
    (attributes(&declaration, TEST).last())
        .map(|(first, spec)| (first, spec.children_with_tokens().any(|it| it.kind() == EQ)))
}

/// The attribute called `attribute` on the declaration `name` names, if
/// any: its name, and what it is given. Of several the last counts.
fn valued_attribute(name: &SyntaxToken, attribute: &str) -> Option<(SyntaxToken, Given)> {
    let declaration = (name.parent_ancestors()).find(|node| declares(node.kind()).is_some())?;
    (attributes(&declaration, attribute).last()).map(|(first, spec)| (first, given(&spec)))
}

/// What the attribute `spec` is given.
fn given(spec: &SyntaxNode) -> Given {
    let given = (spec.children_with_tokens())
        .skip_while(|it| it.kind() != EQ)
        .skip(1)
        .find(|it| !it.kind().is_trivia());
    let Some(given) = given else {
        return Given::Nothing;
    };
    let Some(literal) = given.as_node().filter(|node| node.kind() == LITERAL_EXPR) else {
        return Given::Other;
    };
    let mut tokens = (literal.children_with_tokens())
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia());
    match (tokens.next(), tokens.next()) {
        (Some(token), None) if token.kind() == STRING_LITERAL => {
            let text = token.text().strip_prefix('"').unwrap_or(token.text());
            Given::String(text.strip_suffix('"').unwrap_or(text).to_string())
        }
        (Some(token), None) if token.kind() == INT_LITERAL => {
            Given::Number(token.text().to_string())
        }
        _ => Given::Other,
    }
}

/// Each `astli_sweep` attribute on a parameter declared in the body of the
/// declaration `name` names: the attribute's name, the names the parameter
/// declaration declares, and what the attribute is given. Of several on
/// one declaration the last counts, as for any attribute.
fn sweep_attributes(name: &SyntaxToken) -> Vec<(SyntaxToken, Vec<String>, SweepValues)> {
    let Some(declaration) = (name.parent_ancestors()).find(|node| declares(node.kind()).is_some())
    else {
        return Vec::new();
    };
    let mut parameters = Vec::new();
    body_parameters(&declaration, &mut parameters);
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
                Given::Nothing => SweepValues::Every,
                Given::String(text) => SweepValues::Listed(text),
                Given::Number(_) | Given::Other => SweepValues::NotAString,
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
fn attributes(node: &SyntaxNode, name: &str) -> impl Iterator<Item = (SyntaxToken, SyntaxNode)> {
    let specs = (node.children())
        .filter(|node| node.kind() == ATTRIBUTES)
        .flat_map(|attributes| attributes.children())
        .filter(|node| node.kind() == ATTRIBUTE_SPEC);
    let name = name.to_string();
    specs.filter_map(move |spec| {
        let first = (spec.children_with_tokens())
            .filter_map(|element| element.into_token())
            .find(|token| !token.kind().is_trivia())?;
        (first.kind() == IDENT && first.text() == name).then_some((first, spec))
    })
}
