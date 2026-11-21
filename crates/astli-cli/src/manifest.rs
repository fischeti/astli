//! Changing what a package's `astli.toml` requires, for `add`, `remove`,
//! `snapshot`, `init` and `bender import`. The manifest is changed in place,
//! so that its comments and layout stay, and written back; once the design
//! is resolved again over it, or put back as it was if that fails, so that
//! the design still builds.

use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

use astli_package::{BENDER_MANIFEST, ListedBy, MANIFEST, Package, Patches, key, mark};
use astli_resolve::Lock;
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, TableLike, Value};

use crate::error::{Error, Result};
use crate::run::Run;
use crate::settle::{self, Tell, Write};
use crate::target::{Here, Target};
use crate::term::{self, Problem};

/// The keys of an entry that say where it comes from and which commit, in
/// the order an entry is written with.
const SOURCE: [&str; 4] = [key::GIT, key::PATH, key::VERSION, key::REV];

/// A manifest read to be changed.
pub struct Editable {
    path: PathBuf,
    /// The text as read, to put back; `None` for one not written yet.
    text: Option<String>,
    doc: DocumentMut,
}

impl Editable {
    /// `package`'s manifest. A package without one is refused.
    pub fn open(package: &Package) -> Result<Editable> {
        Editable::read(path(package)?)
    }

    /// The manifest at `path`, read as text, without the package around it.
    pub fn read(path: &Path) -> Result<Editable> {
        let text = std::fs::read_to_string(path).map_err(|err| Error::io(path, err))?;
        let doc =
            (text.parse()).map_err(|err| Error::failed(format!("{}: {err}", path.display())))?;
        Ok(Editable {
            path: path.to_path_buf(),
            text: Some(text),
            doc,
        })
    }

    /// A manifest not yet written for the package in `dir`, holding
    /// `[package] name` alone: the name `bender`, its `Bender.yml`, gives,
    /// lowercased, or else the directory's. Returns the name too.
    pub fn named(dir: &Path, bender: Option<&str>) -> Result<(String, Editable)> {
        let name = match bender.map(astli_package::bender_name) {
            Some(named) => named.map_err(|err| {
                Error::failed(format!("{}: {err}", dir.join(BENDER_MANIFEST).display()))
            })?,
            None => None,
        };
        let name = match name {
            Some(name) => name,
            None => (dir.file_name())
                .map(|it| it.to_string_lossy().into_owned())
                .ok_or_else(|| Error::failed("the current directory has no name to give"))?,
        };
        let mut doc = DocumentMut::new();
        let mut package = Table::new();
        package.insert("name", toml_edit::value(&name));
        doc.insert("package", Item::Table(package));
        let manifest = Editable {
            path: dir.join(MANIFEST),
            text: None,
            doc,
        };
        Ok((name, manifest))
    }

    /// Where the manifest is.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the manifest has the table `table`.
    pub fn lists(&self, table: &str) -> bool {
        self.doc.contains_key(table)
    }

    /// Whether `table` requires `name`.
    pub fn holds(&self, table: &str, name: &str) -> bool {
        self.doc.get(table).and_then(|it| it.get(name)).is_some()
    }

    /// Starts `table`, empty, if the manifest has none.
    pub fn start(&mut self, table: &str) {
        entries(&mut self.doc, table);
    }

    /// Requires `name` in `table`, writing the source keys `keys` over what
    /// its entry says, if it has one, adding `features` to those it enables,
    /// and turning its default features off if `no_default_features`. A new
    /// repository or directory replaces every source key; a version or a
    /// rev alone replaces only which commit. A new entry keeps a sorted
    /// table sorted. Returns the entry written.
    pub fn require(
        &mut self,
        table: &str,
        name: &str,
        keys: &[(&'static str, String)],
        features: &[String],
        no_default_features: bool,
    ) -> InlineTable {
        let entries = entries(&mut self.doc, table);
        let old = entries.get(name).and_then(inline);
        let entry = entry(old.as_ref(), keys, features, no_default_features);
        let sorted = (entries.iter().map(|(key, _)| key)).is_sorted();
        let new = entries.get(name).is_none();
        entries.insert(name, Item::Value(Value::InlineTable(entry.clone())));
        if new && sorted {
            entries.sort_values();
        }
        entry
    }

    /// Requires `name` from `url` at `commit`, keeping the rest of what it
    /// asks, such as features: in `[dev-dependencies]` if that names it,
    /// and else in `[dependencies]`, made if the manifest has none.
    pub fn require_at(&mut self, name: &str, url: &str, commit: &str) {
        let table = match self.holds("dev-dependencies", name) {
            true => "dev-dependencies",
            false => "dependencies",
        };
        let entries = entries(&mut self.doc, table);
        let mut entry = (entries.get(name).and_then(inline)).unwrap_or_default();
        for key in [key::PATH, key::VERSION, key::REV] {
            entry.remove(key);
        }
        if !entry.contains_key(key::GIT) {
            entry.insert(key::GIT, url.into());
        }
        entry.insert(key::REV, commit.into());
        *entries.entry(name).or_insert(Item::None) = Item::Value(Value::InlineTable(entry));
    }

    /// Drops `name` from `table`, and from what the root's features bring
    /// in and `[resolve] pinned`, since both would then name what the root
    /// does not require. Returns whether `table` held it.
    pub fn unrequire(&mut self, table: &str, name: &str) -> bool {
        let removed = (self.doc.get_mut(table))
            .and_then(Item::as_table_like_mut)
            .and_then(|it| it.remove(name));
        if removed.is_none() {
            return false;
        }
        let named = |value: &Value| {
            let asked = value.as_str().unwrap_or_default();
            asked.split_once('/').map_or(asked, |(package, _)| package) == name
        };
        if let Some(features) = (self.doc.get_mut("features")).and_then(Item::as_table_like_mut) {
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
        let pinned = (self.doc.get_mut("resolve"))
            .and_then(Item::as_table_like_mut)
            .and_then(|it| it.get_mut("pinned"))
            .and_then(Item::as_array_mut);
        if let Some(pinned) = pinned {
            pinned.retain(|it| it.as_str() != Some(name));
            pinned.fmt();
        }
        true
    }

    /// Adds `name` to `[resolve] pinned`.
    pub fn pin(&mut self, name: &str) {
        let doc = &mut self.doc;
        if doc.get("resolve").is_none() {
            doc["resolve"] = Item::Table(Table::new());
        }
        let resolve = &mut doc["resolve"];
        if resolve.get("pinned").is_none() {
            resolve["pinned"] = toml_edit::value(Array::new());
        }
        if let Some(pinned) = resolve["pinned"].as_array_mut() {
            pinned.push(name);
        }
    }

    /// Writes a manifest not written yet; one there already is refused.
    pub fn create(&self) -> Result {
        let path = &self.path;
        let mut file = (std::fs::File::create_new(path)).map_err(|err| Error::io(path, err))?;
        (file.write_all(self.doc.to_string().as_bytes())).map_err(|err| Error::io(path, err))
    }

    /// Writes the manifest over what was read, without resolving again.
    pub fn save(&self) -> Result {
        std::fs::write(&self.path, self.doc.to_string()).map_err(|err| Error::io(&self.path, err))
    }
}

/// Why `package`'s dependencies are not astli's to change, if its
/// `Bender.yml` lists them: a `[dependencies]` table would replace that
/// list, which is the bender build's to edit. `here` says what to call it.
pub fn from_bender(here: &Here, package: &Package) -> Option<Problem> {
    if package.manifest.listed_by != ListedBy::Bender {
        return None;
    }
    let since = match package.manifest_path {
        Some(_) => format!(
            "its {MANIFEST} has no {} table",
            mark::code("[dependencies]")
        ),
        None => format!("it has no {MANIFEST}"),
    };
    let problem = term::error(format!(
        "{}'s dependencies come from its {BENDER_MANIFEST}, since {since}",
        here.called(package)
    ));
    Some(problem.help(format!(
        "{} copies them into {MANIFEST}, which then lists them",
        mark::code("astli bender import")
    )))
}

/// Where `package`'s manifest is, for a command that needs one: a package
/// without one is refused.
pub fn path(package: &Package) -> Result<&Path> {
    package.manifest_path.as_deref().ok_or_else(|| {
        term::error(format!("the package has no {MANIFEST}"))
            .help(format!("{} writes one", mark::code("astli init")))
            .into()
    })
}

/// Writes each of `edits`, and `patches`, if given, as the patches of
/// `here`'s workspace, then resolves the design again as `here`'s package
/// now reads, writing the lock, and telling what `tell` and `locked` ask, as
/// [`settle::settle`] does. If any of it fails, every file written is put
/// back as it was, so that the design still builds. The lock needs none:
/// writing it is the last a resolution does.
pub fn commit(
    run: &Run,
    here: &Here,
    edits: &[Editable],
    patches: Option<&Patches>,
    tell: Tell,
    locked: bool,
) -> Result {
    let root = &here.workspace.root.package.root;
    let mut saved: Vec<(PathBuf, Option<String>)> = (edits.iter())
        .map(|it| (it.path.clone(), it.text.clone()))
        .collect();
    if patches.is_some() {
        let path = Patches::path(root);
        saved.push((path.clone(), read_if_any(&path)?));
    }
    let done = (|| {
        for edit in edits {
            edit.save()?;
        }
        if let Some(patches) = patches {
            patches.write(root)?;
        }
        // Found again, workspace and all, as the manifests now read.
        let here = Target::new(here.package.package.root.clone()).here(run)?;
        settle::settle(
            run,
            &here,
            &here.overrides(),
            Lock::clone,
            Write::Manifest,
            tell,
            locked,
        )
    })();
    if let Err(err) = done {
        for (path, text) in &saved {
            restore(path, text.as_deref())?;
        }
        return Err(err);
    }
    Ok(())
}

/// The text of the file at `path`, or `None` without one.
fn read_if_any(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(Error::io(path, err)),
    }
}

/// Puts the file at `path` back as `text`, or removes it if `None`; one
/// already so is left alone, as one whose write failed is.
fn restore(path: &Path, text: Option<&str>) -> Result {
    if read_if_any(path)?.as_deref() == text {
        return Ok(());
    }
    match text {
        Some(text) => std::fs::write(path, text).map_err(|err| Error::io(path, err)),
        None => std::fs::remove_file(path).map_err(|err| Error::io(path, err)),
    }
}

/// The text of the `Bender.yml` in `dir`, if there is one.
pub fn bender_text(dir: &Path) -> Result<Option<String>> {
    let path = dir.join(BENDER_MANIFEST);
    match path.is_file() {
        true => (std::fs::read_to_string(&path).map(Some)).map_err(|err| Error::io(&path, err)),
        false => Ok(None),
    }
}

/// The table `--dev` names, and the other one.
pub fn tables(dev: bool) -> (&'static str, &'static str) {
    match dev {
        true => ("dev-dependencies", "dependencies"),
        false => ("dependencies", "dev-dependencies"),
    }
}

/// `to` relative to `from`, both absolute and clean. On Windows, a path on
/// another drive stays absolute.
pub fn relative(from: &Path, to: &Path) -> PathBuf {
    let from: Vec<Component> = from.components().collect();
    let to: Vec<Component> = to.components().collect();
    let common = (from.iter().zip(&to)).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return to.iter().collect();
    }
    let up = (common..from.len()).map(|_| Component::ParentDir);
    let path: PathBuf = up.chain(to[common..].iter().copied()).collect();
    match path.as_os_str().is_empty() {
        true => PathBuf::from("."),
        false => path,
    }
}

/// A path as a manifest writes it, with `/` on every platform.
pub fn written(path: &Path) -> String {
    let parts = path.components().map(|it| it.as_os_str().to_string_lossy());
    parts.collect::<Vec<_>>().join("/")
}

/// The document's table `table`, made if it has none.
fn entries<'a>(doc: &'a mut DocumentMut, table: &str) -> &'a mut dyn TableLike {
    (doc.entry(table).or_insert(Item::Table(Table::new())))
        .as_table_like_mut()
        .expect("a dependency table")
}

/// An entry as an inline table, whichever way it is written.
fn inline(item: &Item) -> Option<InlineTable> {
    match item {
        Item::Value(Value::InlineTable(entry)) => Some(entry.clone()),
        Item::Table(entry) => Some(entry.clone().into_inline_table()),
        _ => None,
    }
}

/// The entry `old` with the source keys `keys` written over it, its
/// features with `features` added, and its default features off if
/// `no_default_features`.
fn entry(
    old: Option<&InlineTable>,
    keys: &[(&'static str, String)],
    features: &[String],
    no_default_features: bool,
) -> InlineTable {
    let given = |key: &str| keys.iter().any(|(it, _)| *it == key);
    let moved = given(key::GIT) || given(key::PATH);
    let at = given(key::VERSION) || given(key::REV);
    let mut entry = InlineTable::new();
    for key in SOURCE {
        if let Some((_, value)) = keys.iter().find(|(it, _)| *it == key) {
            entry.insert(key, value.as_str().into());
            continue;
        }
        let replaced = moved || (at && (key == key::VERSION || key == key::REV));
        if let Some(value) = old.and_then(|it| it.get(key))
            && !replaced
        {
            entry.insert(key, value.clone());
        }
    }
    for (key, value) in old.into_iter().flat_map(InlineTable::iter) {
        if !SOURCE.contains(&key) {
            entry.insert(key, value.clone());
        }
    }
    if !features.is_empty() {
        let mut enabled = (entry.get(key::FEATURES).and_then(Value::as_array))
            .cloned()
            .unwrap_or_else(Array::new);
        for feature in features {
            if !enabled.iter().any(|it| it.as_str() == Some(feature)) {
                enabled.push(feature.as_str());
            }
        }
        entry.insert(key::FEATURES, Value::Array(enabled));
    }
    if no_default_features {
        entry.insert(key::DEFAULT_FEATURES, false.into());
    }
    entry.fmt();
    entry
}
