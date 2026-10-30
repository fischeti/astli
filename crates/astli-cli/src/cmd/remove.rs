//! `astli remove` subcommand.
//!
//! Drops dependencies from the root's `astli.toml` and resolves again, so
//! that the lock no longer holds what nothing needs. A feature bringing a
//! removed dependency in and `[resolve] pinned` no longer mention it
//! either, since both would then name what the root does not require. A
//! description, an override or a patch of it stays: another dependency may
//! still bring it into the design.

use astli_package::{BENDER_MANIFEST, mark};
use toml_edit::{DocumentMut, Item, Value};
use usage_rs::{Args, RunWith};

use crate::cmd::Ctx;
use crate::cmd::add::{from_bender, tables};
use crate::error::Result;
use crate::lockfile;
use crate::term::{self, status};

/// Remove dependencies from astli.toml and the lock
#[derive(Args)]
pub struct Remove {
    /// Names of the dependencies to remove
    #[usage(arg, value_name = "NAME", required)]
    pub names: Vec<String>,
    /// Remove from [dev-dependencies] instead of [dependencies]
    #[usage(long)]
    pub dev: bool,
}

impl RunWith<Ctx<'_>> for Remove {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let root = lockfile::here()?;
        let (manifest, text, mut document) = lockfile::manifest(&root)?;
        let (table, other) = tables(self.dev);
        if !self.dev
            && document.get("dependencies").is_none()
            && root.root.join(BENDER_MANIFEST).is_file()
        {
            return Err(from_bender());
        }
        for name in &self.names {
            let removed = (document.get_mut(table))
                .and_then(Item::as_table_like_mut)
                .and_then(|it| it.remove(name));
            if removed.is_none() {
                let problem = term::error(format!(
                    "{} is not in {}",
                    mark::name(name),
                    mark::code(format!("[{table}]"))
                ));
                let elsewhere = document.get(other).and_then(|it| it.get(name)).is_some();
                let flag = match self.dev {
                    true => "",
                    false => " --dev",
                };
                return Err(match elsewhere {
                    true => problem.help(format!(
                        "it is in {}: {} removes it",
                        mark::code(format!("[{other}]")),
                        mark::code(format!("astli remove{flag} {name}"))
                    )),
                    false => problem,
                }
                .into());
            }
            unmentioned(&mut document, name);
        }
        lockfile::rewrite(&manifest, &text, &document, ctx.run.locked)?;
        for name in &self.names {
            status!("Removed", "{name} from [{table}]");
        }
        Ok(())
    }
}

/// Drops `name`, and each of its features, from what the root's features
/// bring in, and `name` from `[resolve] pinned`.
fn unmentioned(document: &mut DocumentMut, name: &str) {
    let named = |value: &Value| {
        let asked = value.as_str().unwrap_or_default();
        asked.split_once('/').map_or(asked, |(package, _)| package) == name
    };
    if let Some(features) = document
        .get_mut("features")
        .and_then(Item::as_table_like_mut)
    {
        for (_, feature) in features.iter_mut() {
            let brought = (feature.as_table_like_mut())
                .and_then(|it| it.get_mut("dependencies"))
                .and_then(Item::as_array_mut);
            if let Some(brought) = brought {
                brought.retain(|it| !named(it));
                brought.fmt();
            }
        }
    }
    let pinned = (document.get_mut("resolve"))
        .and_then(Item::as_table_like_mut)
        .and_then(|it| it.get_mut("pinned"))
        .and_then(Item::as_array_mut);
    if let Some(pinned) = pinned {
        pinned.retain(|it| it.as_str() != Some(name));
        pinned.fmt();
    }
}
