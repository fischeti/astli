//! Which tops a run elaborates from, what they define and what their
//! parameters are set to: those `--top` names, each a `[top.NAME]` of the
//! root or else a module, or without any, the root's default.

use astli_package::{Package, mark};
use astli_sema::{BodyId, Elaboration, TopError};

use crate::error::Result;
use crate::sources::split_define;
use crate::term;

/// The tops of a run.
#[derive(Default)]
pub struct Chosen {
    /// What is elaborated from, in the order named.
    pub modules: Vec<String>,
    /// What each of `modules` is called: the table's name, or the module's
    /// where no table names it.
    pub names: Vec<String>,
    /// What the tables chosen define, each once, as `-D` takes it.
    pub defines: Vec<String>,
    /// What each of `modules` has its parameters set to, by name, each
    /// value as `-G` takes it.
    pub params: Vec<Vec<(String, String)>>,
    /// Each table chosen that builds a module of another name, with that
    /// module's name.
    aliases: Vec<(String, String)>,
}

impl Chosen {
    /// The tops `asked` names, which are modules, since only a package has
    /// tables.
    pub fn named(asked: &[String]) -> Chosen {
        Chosen {
            modules: asked.to_vec(),
            names: asked.to_vec(),
            params: vec![Vec::new(); asked.len()],
            ..Chosen::default()
        }
    }

    /// The tops `asked` names of `package`'s, or without any its
    /// `default-top`, else its only table, else none if it has no table.
    pub fn of(package: &Package, asked: &[String]) -> Result<Chosen> {
        let defaulted: Vec<String>;
        let asked = match (asked, &package.default_top) {
            ([], Some(top)) => std::slice::from_ref(top),
            ([], None) => {
                // Of several tables, a run would be guessing.
                defaulted = match package.tops.len() {
                    0 | 1 => package.tops.keys().cloned().collect(),
                    _ => return Err(several(package)),
                };
                &defaulted
            }
            (asked, _) => asked,
        };
        let mut chosen = Chosen::default();
        let mut defined: Vec<(String, &str, String)> = Vec::new();
        for name in asked {
            chosen.names.push(name.clone());
            let Some(table) = package.tops.get(name) else {
                chosen.modules.push(name.clone());
                chosen.params.push(Vec::new());
                continue;
            };
            let module = table.module(name);
            if module != name {
                chosen.aliases.push((name.clone(), module.to_string()));
            }
            chosen.modules.push(module.to_string());
            chosen
                .params
                .push(table.params.clone().into_iter().collect());
            let defines = (table.defines.iter()).map(|(name, body)| match body {
                Some(body) => format!("{name}={body}"),
                None => name.clone(),
            });
            for define in defines {
                let (macro_name, body) = split_define(&define);
                match defined.iter().find(|(it, _, _)| *it == macro_name) {
                    Some((_, _, before)) if before == body => {}
                    Some((_, by, before)) => {
                        return Err(term::error(format!(
                            "{} is defined as {} by {} and as {} by {}",
                            mark::name(macro_name),
                            mark::code(before),
                            mark::code(format!("top.{by}")),
                            mark::code(body),
                            mark::code(format!("top.{name}"))
                        ))
                        .help("a define is the same for every file, so the two cannot be built together")
                        .into());
                    }
                    None => {
                        defined.push((macro_name.to_string(), name, body.to_string()));
                        chosen.defines.push(define);
                    }
                }
            }
        }
        Ok(chosen)
    }

    /// Sets each parameter `flags` name, as `-G` takes it, on every top,
    /// over what a table sets it to.
    pub fn set(&mut self, flags: &[String]) -> Result {
        if let Some(flag) = flags.first()
            && self.modules.is_empty()
        {
            return Err(term::error(format!(
                "{} sets a parameter of a top, and the run builds none",
                mark::code(format!("-G {flag}"))
            ))
            .help(format!("name one with {}", mark::code("--top")))
            .into());
        }
        for flag in flags {
            let Some((name, value)) = flag.split_once('=') else {
                return Err(term::error(format!(
                    "{} sets no value",
                    mark::code(format!("-G {flag}"))
                ))
                .help(format!(
                    "write it as {}",
                    mark::code(format!("-G {flag}=VALUE"))
                ))
                .into());
            };
            for params in &mut self.params {
                params.retain(|(it, _)| it != name);
                params.push((name.to_string(), value.to_string()));
            }
        }
        Ok(())
    }

    /// Refuses a table chosen whose name `declared` says is also a module's
    /// other than the one it builds, since which of the two `--top` means is
    /// a guess.
    pub fn unambiguous(&self, declared: impl Fn(&str) -> bool) -> Result {
        let Some((name, module)) = self.aliases.iter().find(|(name, _)| declared(name)) else {
            return Ok(());
        };
        Err(term::error(format!(
            "{} is a module, and the name of {}, which builds {}",
            mark::name(name),
            mark::code(format!("top.{name}")),
            mark::name(module)
        ))
        .help(format!(
            "rename {}, so that {} means one of them",
            mark::code(format!("top.{name}")),
            mark::code(format!("--top {name}"))
        ))
        .into())
    }
}

/// Why a run of `package`, which has several tables and no default, cannot
/// tell which to build.
fn several(package: &Package) -> crate::error::Error {
    let names: Vec<String> = package.tops.keys().map(mark::name).collect();
    term::error(format!(
        "no top named, and the manifest has {}: {}",
        package.tops.len(),
        names.join(", ")
    ))
    .help(format!(
        "name one with {}, or set {} in {}",
        mark::code("--top"),
        mark::code("default-top"),
        mark::code("[package]")
    ))
    .into()
}

/// The body of `module` as a top, with `params` set, by name, to the value
/// each spells.
pub fn body(
    elaboration: &Elaboration,
    module: &str,
    params: &[(String, String)],
) -> Result<BodyId> {
    let of = |name: &str| format!("{} of {}", mark::name(name), mark::name(module));
    let err = match elaboration.top_with(module, params) {
        Ok(body) => return Ok(body),
        Err(TopError::NoTop) => term::error(format!(
            "no module, interface or program is called {}",
            mark::name(module)
        )),
        Err(TopError::NoParameter(name)) => term::error(format!(
            "{} has no parameter called {}",
            mark::name(module),
            mark::name(&name)
        )),
        Err(TopError::Local(name)) => term::error(format!(
            "{} is a {}, which only its declaration sets",
            of(&name),
            mark::code("localparam")
        )),
        Err(TopError::Type(name)) => term::error(format!(
            "{} is a type parameter, which a value cannot set",
            of(&name)
        )),
        Err(TopError::NotAValue(name, text)) => {
            let err = term::error(format!(
                "{} is set to {}, which is no number or string",
                of(&name),
                mark::code(&text)
            ));
            // A shell takes the quotes of `-G Name="text"` for its own.
            match text.contains('"') {
                true => err,
                false => err.help(format!(
                    "a string is quoted, as in {}",
                    mark::code(format!("-G '{name}=\"{text}\"'"))
                )),
            }
        }
    };
    Err(err.into())
}
