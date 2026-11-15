//! Which tops a run elaborates from, what they define and what their
//! parameters are set to: those `--top` names, each a `[top.NAME]` of the
//! root or else a module, or without any, the root's default, or for
//! `check` and `lint` every table, one run per set of defines.

use std::collections::BTreeMap;

use astli_package::{Package, mark};
use astli_sema::{BodyId, Elaboration, TopError};

use crate::error::Result;
use crate::term;

/// One top a run elaborates from: a `[top.NAME]` table, or a module no
/// table names.
#[derive(Clone)]
pub struct Top {
    /// What it is called: the table's name, or the module's.
    pub name: String,
    /// What is elaborated from.
    pub module: String,
    /// What its parameters are set to, by name, each value as `-G` takes
    /// it.
    pub params: Vec<(String, String)>,
    /// What its table defines, each macro's body by name; `None` for one
    /// defined as `-D NAME` defines it.
    pub defines: BTreeMap<String, Option<String>>,
}

impl Top {
    /// The top `name` names among `package`'s: the table of that name, or
    /// else the module, which then needs nothing defined.
    pub fn of(package: &Package, name: &str) -> Top {
        let Some(table) = package.tops.get(name) else {
            return Top {
                name: name.to_string(),
                module: name.to_string(),
                params: Vec::new(),
                defines: BTreeMap::new(),
            };
        };
        Top {
            name: name.to_string(),
            module: table.module(name).to_string(),
            params: table.params.clone().into_iter().collect(),
            defines: table.defines.clone(),
        }
    }
}

/// The tops of a run.
#[derive(Default, Clone)]
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
        Chosen::joined(asked.iter().map(|name| Top::of(package, name)).collect())
    }

    /// Every table of `package`'s, as [`grouped`] groups them; none if it
    /// has no table.
    pub fn every(package: &Package) -> Vec<Chosen> {
        let tops = package.tops.keys().map(|name| Top::of(package, name));
        grouped(tops.collect())
    }

    /// `tops`, built together: each defines its macros for every file, so
    /// one macro defined two ways is an error.
    pub fn joined(tops: Vec<Top>) -> Result<Chosen> {
        let mut chosen = Chosen::default();
        let mut defined: Vec<(&str, &str, &str)> = Vec::new();
        for top in &tops {
            for (macro_name, body) in &top.defines {
                // `NAME` alone defines it as `1`, as `-D NAME` does.
                let text = body.as_deref().unwrap_or("1");
                match defined.iter().find(|(it, _, _)| it == macro_name) {
                    Some((_, _, before)) if *before == text => {}
                    Some((_, by, before)) => {
                        return Err(term::error(format!(
                            "{} is defined as {} by {} and as {} by {}",
                            mark::name(macro_name),
                            mark::code(before),
                            mark::code(format!("top.{by}")),
                            mark::code(text),
                            mark::code(format!("top.{}", top.name))
                        ))
                        .help("a define is the same for every file, so the two cannot be built together")
                        .into());
                    }
                    None => {
                        defined.push((macro_name, &top.name, text));
                        chosen.defines.push(match body {
                            Some(body) => format!("{macro_name}={body}"),
                            None => macro_name.clone(),
                        });
                    }
                }
            }
        }
        for top in tops {
            if top.module != top.name {
                chosen.aliases.push((top.name.clone(), top.module.clone()));
            }
            chosen.names.push(top.name);
            chosen.modules.push(top.module);
            chosen.params.push(top.params);
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

/// `tops` as one run for each set of defines they give, in the order of
/// the first top of each: those that define the same are built together,
/// as a filelist of them would be, and those that define one macro two
/// ways are built apart.
pub fn grouped(tops: Vec<Top>) -> Vec<Chosen> {
    let mut groups: Vec<Vec<Top>> = Vec::new();
    for top in tops {
        match groups
            .iter_mut()
            .find(|group| group[0].defines == top.defines)
        {
            Some(group) => group.push(top),
            None => groups.push(vec![top]),
        }
    }
    (groups.into_iter())
        .map(|group| Chosen::joined(group).expect("one set of defines joins"))
        .collect()
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
