//! Which tops a run elaborates from, and what they define: those `--top`
//! names, each a `[top.NAME]` of the root or else a module, or without any,
//! the root's default.

use astli_package::{Package, mark};

use crate::error::Result;
use crate::sources::split_define;
use crate::term;

/// The tops of a run.
#[derive(Default)]
pub struct Chosen {
    /// What is elaborated from, in the order named.
    pub modules: Vec<String>,
    /// What the tables chosen define, each once, in the order written.
    pub defines: Vec<String>,
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
        let mut defined: Vec<(&str, &str, &str)> = Vec::new();
        for name in asked {
            let Some(table) = package.tops.get(name) else {
                chosen.modules.push(name.clone());
                continue;
            };
            let module = table.module(name);
            if module != name {
                chosen.aliases.push((name.clone(), module.to_string()));
            }
            chosen.modules.push(module.to_string());
            for define in &table.defines {
                let (macro_name, body) = split_define(define);
                match defined.iter().find(|(it, _, _)| *it == macro_name) {
                    Some((_, _, before)) if *before == body => {}
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
                        defined.push((macro_name, name, body));
                        chosen.defines.push(define.clone());
                    }
                }
            }
        }
        Ok(chosen)
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
