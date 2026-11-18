//! `astli elab` subcommand.
//!
//! Lowers every file of the design as `check` does, then elaborates it from
//! each top and prints the instance tree: each instance with its definition
//! and the values of its parameter port list, each generate block built,
//! and what sema cannot decide or find. With `--json` it writes the design
//! instead: each body the tops build once, with its parameters and ports,
//! and the tree as instances pointing at bodies.
//!
//! With no file named, the design is the package the current directory is
//! in and its dependencies, as `check` reads it, and the tops are those
//! `--top` names or the manifest's `[top]` tables give.
//!
//! What it writes is a preview, and may change in any release.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use astli_sema::{
    BodyId, Bound, ChildKind, Elaboration, FileId, Member, ParameterKind, Port, Resolution,
    SymbolId, SymbolKind, SymbolRef, TypeKind, Value,
};
use astli_text::{Origins, Span};
use usage_rs::{Args, RunWith};

use crate::analysis::{self, Analysis};
use crate::cli::{BuildArgs, FeatureArgs, PackageArg, Sources};
use crate::cmd::{self, Ctx};
use crate::discovery::Plan;
use crate::error::{Error, Result};
use crate::json::Json;
use crate::term;

/// What the help and the run say of the command's output.
const PREVIEW: &str = "`astli elab` is a preview: what it writes may change in any release";

/// How deep `--json` follows the tree, as the text does: a definition that
/// instantiates itself with a new parameter each time would go on forever.
const DEPTH: usize = 128;

/// Inspect the elaborated instance hierarchy for a top-level module (preview)
#[derive(Args)]
#[usage(
    after_help = "A preview: what it writes, the tree and the JSON alike, may change in any release."
)]
pub struct Elab {
    #[usage(flatten)]
    pub sources: Sources,
    #[usage(flatten)]
    pub package: PackageArg,
    #[usage(flatten)]
    pub features: FeatureArgs,
    /// Elaborate from the specified root module or [top.NAME] table (can be repeated)
    #[usage(long)]
    pub top: Vec<String>,
    /// Set a parameter of each top as NAME=VALUE, over what its [top.NAME] table sets (can be repeated)
    #[usage(short = 'G', long = "param")]
    pub param: Vec<String>,
    /// Write the elaborated design as JSON: each body built, with its parameters and ports, and the instance tree
    #[usage(long)]
    pub json: bool,
    #[usage(flatten)]
    pub build: BuildArgs,
}

impl RunWith<Ctx<'_>> for Elab {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        let target = cmd::choose(self.package.package.as_deref(), self.sources.named())?;
        let mut plan = Plan::new(
            &target,
            &self.sources,
            &self.features,
            &self.build,
            Some(&self.top),
            run,
        )?;
        if plan.tops.modules.is_empty() {
            return Err(Error::failed("name a top to elaborate from with --top"));
        }
        plan.tops.set(&self.param)?;
        term::note(PREVIEW).report(run);
        let mut analysis = Analysis::new(out, &mut plan, run, false)?;
        // What the files reported is printed, and the tree is still built
        // from what they hold.
        let parsed = std::mem::take(&mut analysis.parsed);
        let reported = (analysis.origins.iter().zip(parsed))
            .zip(&plan.units)
            .filter(|(_, unit)| unit.reported)
            .map(|((origins, found), _)| (Some(&**origins), Vec::new(), 0, found))
            .collect();
        let mut reported = analysis::report(out, reported)?;
        let written = match self.json {
            true => design(&analysis).to_string(),
            false => (analysis.tops.iter())
                .map(|top| analysis.elaboration.dump(top.body))
                .collect(),
        };
        out.write_all(written.as_bytes()).map_err(Error::Output)?;
        // An error a file holds still leaves a tree to print, but a file
        // that could not be read leaves it short of what that file holds.
        reported.wrong = 0;
        reported.failed = analysis.unread;
        analysis.leak();
        reported.finish()
    }
}

/// The design the tops build, as `--json` writes it:
///
/// ```json
/// {"tops": [{"name": "soc", "definition": "soc", "body": 0}],
///  "bodies": [{"id": 0, "definition": "soc", "keyword": "module",
///              "path": "rtl/soc.sv", "line": 3, "extern": false,
///              "parameters": [...], "ports": [...], "children": [...]}]}
/// ```
///
/// A body is a definition with its parameters decided, which every instance
/// that decides them alike shares, so each is written once and an instance
/// names its body by `id`. A generate block's children are written in it,
/// a block belonging to the body it is built in.
fn design(analysis: &Analysis) -> Json {
    let mut walk = Walk {
        elaboration: &analysis.elaboration,
        origins: (analysis.files.iter().copied())
            .zip(analysis.origins.iter().cloned())
            .collect(),
        ids: HashMap::new(),
        found: Vec::new(),
    };
    let tops = (analysis.tops.iter())
        .map(|top| {
            Json::Object(vec![
                ("name", Json::str(&top.name)),
                ("definition", Json::str(&top.module)),
                ("body", walk.id(top.body, 0)),
            ])
        })
        .collect();
    // Writing a body finds those of its instances, so the list grows as it
    // is walked.
    let mut bodies = Vec::new();
    while let Some(&(body, depth)) = walk.found.get(bodies.len()) {
        bodies.push(walk.body(bodies.len(), body, depth));
    }
    Json::Object(vec![
        ("tops", Json::Array(tops)),
        ("bodies", Json::Array(bodies)),
    ])
}

/// A walk of the tree from the tops, numbering each body as it is found.
struct Walk<'a> {
    elaboration: &'a Elaboration,
    /// What each file's spans resolve against.
    origins: HashMap<FileId, Arc<Origins>>,
    ids: HashMap<BodyId, usize>,
    /// Each body numbered, by its number, and how deep in the tree it was
    /// first found.
    found: Vec<(BodyId, usize)>,
}

impl Walk<'_> {
    /// The number of `body`, found `depth` deep, numbering it if it is new;
    /// null past [`DEPTH`].
    fn id(&mut self, body: BodyId, depth: usize) -> Json {
        if let Some(&id) = self.ids.get(&body) {
            return Json::int(id as u64);
        }
        if depth > DEPTH {
            return Json::Null;
        }
        let id = self.found.len();
        self.ids.insert(body, id);
        self.found.push((body, depth));
        Json::int(id as u64)
    }

    /// The body numbered `id`: its definition, where that is declared, its
    /// parameters and ports, and what it holds.
    fn body(&mut self, id: usize, body: BodyId, depth: usize) -> Json {
        let elaboration = self.elaboration;
        let design = elaboration.design();
        let (file, scope) = elaboration.scope(body);
        let hir = &design[file];
        let mut fields = vec![("id", Json::int(id as u64))];
        let definition = (hir[scope].owner).map(|symbol| SymbolRef { file, symbol });
        match definition.map(|at| (at, &design.symbol(at).kind)) {
            Some((
                at,
                SymbolKind::Definition {
                    keyword,
                    ports,
                    prototype,
                    ..
                },
            )) => {
                let name = &design.symbol(at).name;
                let (path, line) = self.place(file, name.span);
                let keyword = keyword.keyword_text().unwrap_or_default();
                fields.extend([
                    ("definition", Json::str(&*name.text)),
                    ("keyword", Json::str(keyword)),
                    ("path", path),
                    ("line", line),
                    ("extern", Json::Bool(*prototype)),
                    ("parameters", self.parameters(at, body)),
                    ("ports", Json::Array(self.ports(file, body, ports))),
                ]);
            }
            _ => fields.push(("definition", Json::Null)),
        }
        fields.push(("children", Json::Array(self.children(body, depth + 1))));
        Json::Object(fields)
    }

    /// Each parameter `definition` declares, in its parameter port list or
    /// its body, with its value in `body`, and whether that is the value
    /// its definition's defaults give it, as a `localparam` computed from
    /// an overridden parameter's is not.
    fn parameters(&self, definition: SymbolRef, body: BodyId) -> Json {
        let elaboration = self.elaboration;
        let design = elaboration.design();
        let hir = &design[definition.file];
        let SymbolKind::Definition { scope, .. } = design.symbol(definition).kind else {
            return Json::Array(Vec::new());
        };
        let defaults = elaboration.body(definition);
        let mut parameters = Vec::new();
        for member in &hir[scope].members {
            let &Member::Declare(symbol) = member else {
                continue;
            };
            let SymbolKind::Parameter(parameter) = &hir[symbol].kind else {
                continue;
            };
            if parameter.kind == ParameterKind::Specparam {
                continue;
            }
            let is_type = parameter.is_type;
            let mut fields = vec![
                ("name", Json::str(&*hir[symbol].name.text)),
                ("local", Json::Bool(parameter.is_local())),
            ];
            if is_type {
                let ty = elaboration.type_of(body, symbol);
                let default = defaults.is_none_or(|at| elaboration.type_of(at, symbol) == ty);
                fields.extend([
                    ("kind", Json::str("type")),
                    ("type", Json::Null),
                    ("value", Json::str(ty.to_string())),
                    ("integer", Json::Null),
                    ("default", Json::Bool(default)),
                ]);
            } else {
                let value = elaboration.value(body, symbol);
                let default = defaults.is_none_or(|at| elaboration.value(at, symbol) == value);
                let integer = match &value {
                    Value::Bits(bits) => bits.to_i64(),
                    _ => None,
                };
                let value = match value {
                    Value::Unknown => Json::Null,
                    known => Json::str(known.to_string()),
                };
                let ty = elaboration.type_of(body, symbol);
                fields.extend([
                    ("kind", Json::str("value")),
                    ("type", Json::str(ty.to_string())),
                    ("value", value),
                    ("integer", Json::or_null(integer, Json::int)),
                    ("default", Json::Bool(default)),
                ]);
            }
            parameters.push(Json::Object(fields));
        }
        Json::Array(parameters)
    }

    /// Each of `ports`, of a definition in `file`, with its direction and
    /// its type in `body`. An interface port has no direction, and its type
    /// is the interface as written, with its modport.
    fn ports(&self, file: FileId, body: BodyId, ports: &[SymbolId]) -> Vec<Json> {
        let elaboration = self.elaboration;
        let hir = &elaboration.design()[file];
        let ports = ports.iter().map(|&symbol| {
            let at = SymbolRef { file, symbol };
            let (direction, named) = match &hir[symbol].kind {
                SymbolKind::Port(Port { direction, ty, .. }) => (
                    direction.and_then(|it| it.keyword_text()),
                    match ty.kind {
                        TypeKind::Named(named) => Some(named),
                        _ => None,
                    },
                ),
                _ => (None, None),
            };
            let name = ("name", Json::str(&*hir[symbol].name.text));
            if let (Some(_), Some(named)) = (elaboration.interface_port(at), named) {
                return Json::Object(vec![
                    name,
                    ("interface", Json::Bool(true)),
                    ("direction", Json::Null),
                    ("type", self.text(file, hir[named].span)),
                    ("width", Json::Null),
                ]);
            }
            let ty = elaboration.symbol_type(body, at);
            Json::Object(vec![
                name,
                ("interface", Json::Bool(false)),
                ("direction", Json::or_null(direction, Json::str)),
                ("type", Json::str(ty.to_string())),
                ("width", Json::or_null(ty.width(), Json::int)),
            ])
        });
        ports.collect()
    }

    /// What `body` holds once built, found `depth` deep.
    fn children(&mut self, body: BodyId, depth: usize) -> Vec<Json> {
        let elaboration = self.elaboration;
        let design = elaboration.design();
        let mut children = Vec::new();
        for child in elaboration.children(body).iter() {
            let name = ("name", Json::str(&*child.name));
            let fields = match &child.kind {
                ChildKind::Instance {
                    symbol,
                    definition,
                    body,
                    dims,
                } => {
                    let (path, line) = self.place(symbol.file, design.symbol(*symbol).name.span);
                    let dims = Json::or_null(dims.as_ref(), |dims| {
                        Json::Array(
                            (dims.iter())
                                .map(|&(left, right)| {
                                    Json::Array(vec![Json::int(left), Json::int(right)])
                                })
                                .collect(),
                        )
                    });
                    vec![
                        ("kind", Json::str("instance")),
                        name,
                        (
                            "definition",
                            Json::str(&*design.symbol(*definition).name.text),
                        ),
                        ("body", self.id(*body, depth)),
                        ("dims", dims),
                        ("path", path),
                        ("line", line),
                        ("connections", self.connections(*symbol, *definition)),
                    ]
                }
                ChildKind::Missing { symbol } | ChildKind::Gate { symbol } => {
                    let (path, line) = self.place(symbol.file, design.symbol(*symbol).name.span);
                    let SymbolKind::Instance(instance) = &design.symbol(*symbol).kind else {
                        continue;
                    };
                    let (kind, what) = match child.kind {
                        ChildKind::Missing { .. } => ("missing", "definition"),
                        _ => ("gate", "gate"),
                    };
                    vec![
                        ("kind", Json::str(kind)),
                        name,
                        (what, Json::str(&*instance.definition.text)),
                        ("path", path),
                        ("line", line),
                    ]
                }
                ChildKind::Block { body } => vec![
                    ("kind", Json::str("block")),
                    name,
                    ("children", Json::Array(self.children(*body, depth + 1))),
                ],
                ChildKind::Undecided { .. } => vec![("kind", Json::str("undecided")), name],
            };
            children.push(Json::Object(fields));
        }
        children
    }

    /// What each port of `definition` the instance `at` connects is
    /// connected to: the expression's text, or the port's name for `.name`
    /// and `.*`, which connect what that names, and null where nothing is
    /// declared by that name.
    fn connections(&self, at: SymbolRef, definition: SymbolRef) -> Json {
        let design = self.elaboration.design();
        let declared = &design[definition.file];
        let connections = self
            .elaboration
            .connections(at)
            .into_iter()
            .map(|(port, found)| {
                let port = &*declared[port].name.text;
                let text = match found {
                    Bound::Expr(expr) => self.text(at.file, design[at.file][expr].span),
                    Bound::Name(Resolution::Undeclared(_), _) => Json::Null,
                    Bound::Name(..) => Json::str(port),
                };
                Json::Object(vec![("port", Json::str(port)), ("text", text)])
            });
        Json::Array(connections.collect())
    }

    /// The path and the line where the user wrote `span`, of `file`: a
    /// macro's call, for what a macro expands to.
    fn place(&self, file: FileId, span: Span) -> (Json, Json) {
        let Some(origins) = self.origins.get(&file) else {
            return (Json::Null, Json::Null);
        };
        let at = origins.reported_at(span);
        let path = origins
            .path(at.src_id)
            .map(|path| path.display().to_string());
        let line = origins.line_col(at.src_id, at.start).line;
        (Json::or_null(path, Json::str), Json::int(line))
    }

    /// The text of `span`, of `file`.
    fn text(&self, file: FileId, span: Span) -> Json {
        let text = self.origins.get(&file).map(|origins| origins.slice(span));
        Json::or_null(text, Json::str)
    }
}
