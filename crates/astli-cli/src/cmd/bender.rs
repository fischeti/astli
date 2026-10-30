//! `astli bender` subcommand.
//!
//! A package that bender still builds keeps a `Bender.yml` beside its
//! `astli.toml`. `import` copies the dependencies of the `Bender.yml` into
//! the `astli.toml`, once, and `export` writes the `Bender.yml` from the
//! `astli.toml` from then on, so that the `astli.toml` is the one that is
//! edited and the two cannot drift apart.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use astli_index::{Index, Summary};
use astli_package::{At, BENDER_MANIFEST, MANIFEST, Package, Requirement, Source, mark};
use semver::{Op, VersionReq};
use toml_edit::{DocumentMut, InlineTable, Item, Table, Value};
use usage_rs::{Args, RunWith, Subcommands};

use crate::cli::{BuildArgs, FeatureArgs, RunArgs, Sources};
use crate::cmd::add::{relative, written};
use crate::cmd::files::one;
use crate::cmd::{self, Ctx, init};
use crate::discovery::{Plan, kept_only};
use crate::error::{Error, Result};
use crate::lockfile;
use crate::term::{self, status};

/// Keep a Bender.yml for bender alongside astli.toml
#[derive(Args)]
#[usage(run_with)]
pub struct Bender {
    #[usage(subcommand)]
    pub command: BenderCommand,
}

#[derive(Subcommands)]
#[usage(run_with)]
pub enum BenderCommand {
    Import(Import),
    Export(Export),
}

/// Copy the dependencies of Bender.yml into astli.toml, creating it if needed
#[derive(Args)]
pub struct Import {}

/// Write Bender.yml from astli.toml, for bender to build the package with
#[derive(Args)]
pub struct Export {
    /// Fail if Bender.yml is not what would be written, without writing it
    #[usage(long)]
    pub check: bool,
}

/// The first line of every `Bender.yml` `export` writes.
const HEADER: &str = "# Written by `astli bender export` from astli.toml: edit that instead.\n";

impl RunWith<Ctx<'_>> for Import {
    type Output = Result;

    fn run_with(self, _: Ctx<'_>) -> Result {
        let here = std::env::current_dir().map_err(|err| Error::io(".", err))?;
        let Some(text) = init::read_bender(&here)? else {
            return Err(Error::failed(format!(
                "no {BENDER_MANIFEST} here to copy dependencies from"
            )));
        };
        let dependencies = astli_package::bender_dependencies(&text).map_err(|err| {
            Error::failed(format!("{}: {err}", here.join(BENDER_MANIFEST).display()))
        })?;
        let mut table = Table::new();
        for (name, requirement) in &dependencies {
            table.insert(name, Item::Value(Value::InlineTable(entry(requirement))));
        }
        let count = match dependencies.len() {
            1 => "1 dependency".to_string(),
            count => format!("{count} dependencies"),
        };

        let path = here.join(MANIFEST);
        if !path.exists() {
            let (name, mut document) = init::named(&here, Some(&text))?;
            document.insert("dependencies", Item::Table(table));
            init::create(&path, &document)?;
            status!(
                "Created",
                "{MANIFEST} for {}, with the {count} of {BENDER_MANIFEST}",
                mark::name(&name)
            );
            return Ok(());
        }
        let before = std::fs::read_to_string(&path).map_err(|err| Error::io(&path, err))?;
        let mut document: DocumentMut =
            (before.parse()).map_err(|err| Error::failed(format!("{}: {err}", path.display())))?;
        if document.contains_key("dependencies") {
            return Err(term::error(format!(
                "{MANIFEST} already lists its dependencies, so {BENDER_MANIFEST}'s are not read"
            ))
            .help(format!(
                "{} writes {BENDER_MANIFEST} from them instead",
                mark::code("astli bender export")
            ))
            .into());
        }
        document.insert("dependencies", Item::Table(table));
        std::fs::write(&path, document.to_string()).map_err(|err| Error::io(&path, err))?;
        status!("Copied", "the {count} of {BENDER_MANIFEST} into {MANIFEST}");
        Ok(())
    }
}

impl RunWith<Ctx<'_>> for Export {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        let root = lockfile::here()?;
        let (_, _, document) = lockfile::manifest(&root)?;
        let path = root.root.join(BENDER_MANIFEST);
        if !document.contains_key("dependencies") && path.is_file() {
            return Err(term::error(format!(
                "the root's dependencies come from its {BENDER_MANIFEST}, since its {MANIFEST} \
                 has no {} table, so there is nothing to write it from",
                mark::code("[dependencies]")
            ))
            .help(format!(
                "{} copies them in",
                mark::code("astli bender import")
            ))
            .into());
        }

        // Every feature on, as the package's own runs read it, so that every
        // file is ordered among the others.
        let features = FeatureArgs {
            features: Vec::new(),
            all_features: true,
        };
        let mut plan = Plan::new(
            &Sources::default(),
            &features,
            &BuildArgs::default(),
            None,
            run,
        )?;
        lockfile::described(&plan.described);
        let silent = RunArgs {
            quiet: true,
            jobs: run.jobs,
            diagnostics: run.diagnostics,
            color: run.color,
            locked: run.locked,
        };
        let outcome = cmd::each(out, &plan.units, &silent, "", |sink, file| {
            let unit = plan.unit(file);
            one(sink, file, plan.build(unit), unit.reported)
        })?;
        // A region that did not parse may hold an instance, so the order
        // could be wrong, as `files` refuses to write a list then.
        if outcome.failed > 0 || outcome.wrong > 0 {
            return outcome.finish();
        }
        let units: Vec<(PathBuf, bool)> = (plan.units.iter())
            .map(|unit| (unit.path.clone(), unit.reported))
            .collect();
        let kept = plan.replace(&outcome.values)?;
        let overriding = overriding(&root.root, &units, &outcome.values, &kept)?;
        let index = Index::new(kept_only(outcome.values, &kept));
        let own: Vec<usize> = (plan.units.iter().enumerate())
            .filter(|(_, unit)| unit.reported)
            .map(|(at, _)| at)
            .collect();
        let ordered = index.ordered(&own);

        let Some((package, _)) = plan.packages() else {
            unreachable!("a run naming no file discovers the package");
        };
        let text = manifest(&plan, package, &index, &ordered, &overriding)?;

        let current = match path.is_file() {
            true => Some(std::fs::read_to_string(&path).map_err(|err| Error::io(&path, err))?),
            false => None,
        };
        if self.check {
            return match current.as_deref() == Some(text.as_str()) {
                true => Ok(()),
                false => {
                    let what = match current {
                        Some(_) => format!("{BENDER_MANIFEST} is not what {MANIFEST} exports"),
                        None => format!("no {BENDER_MANIFEST} here, though {MANIFEST} exports one"),
                    };
                    Err(term::error(what)
                        .help(format!("{} writes it", mark::code("astli bender export")))
                        .into())
                }
            };
        }
        if current.as_deref() != Some(text.as_str()) {
            std::fs::write(&path, &text).map_err(|err| Error::io(&path, err))?;
        }
        let name = package.name.as_deref().unwrap_or_default();
        status!("Exported", "{BENDER_MANIFEST} for {}", mark::name(name));
        Ok(())
    }
}

/// The root's files that replace a dependency's, given each unit's path
/// and whether it is the root's, its summary, and whether it was kept.
///
/// Bender puts each in place of every file of the same name in the design,
/// and compiles it there, with that package's include directories. So a
/// replacement is refused where bender would build something else: named
/// otherwise than what it replaces, which bender would build beside it;
/// named as a file astli keeps, which bender would replace too; or reading
/// a header of the root, which the package it replaces into cannot find. A
/// file another dependency replaces is that dependency's to say.
fn overriding(
    root: &Path,
    units: &[(PathBuf, bool)],
    summaries: &[Summary],
    kept: &[bool],
) -> Result<BTreeSet<PathBuf>> {
    let declares = |summary: &Summary, name: &str| {
        (summary.declarations.iter()).any(|declaration| declaration.name == name)
    };
    let mut overriding = BTreeSet::new();
    for ((path, _), summary) in (units.iter().zip(summaries).zip(kept))
        .filter(|(_, kept)| !**kept)
        .map(|(it, _)| it)
    {
        let replacing: Vec<(&Path, &Summary)> = (units.iter().zip(summaries))
            .filter(|((_, own), _)| *own)
            .filter(|(_, by)| {
                (summary.declarations.iter()).any(|declaration| declares(by, &declaration.name))
            })
            .map(|((path, _), summary)| (path.as_path(), summary))
            .collect();
        let Some(&(first, _)) = replacing.first() else {
            continue;
        };
        if !replacing
            .iter()
            .any(|(by, _)| by.file_name() == path.file_name())
        {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            return Err(term::error(format!(
                "{} replaces {}, but bender replaces a file only by one of the same name",
                mark::path(first.display()),
                mark::path(path.display())
            ))
            .help(format!("name the replacement {}", mark::path(name)))
            .into());
        }
        for (by, summary) in replacing {
            if let Some(header) = (summary.includes.iter()).find(|it| it.starts_with(root)) {
                return Err(term::error(format!(
                    "{} replaces {} and includes {}, which bender would not find there",
                    mark::path(by.display()),
                    mark::path(path.display()),
                    mark::path(header.display())
                ))
                .help(
                    "bender compiles a replacement with the include directories of the package \
                     it replaces into",
                )
                .into());
            }
            overriding.insert(by.to_path_buf());
        }
    }
    for by in &overriding {
        let same = (units.iter().zip(kept))
            .find(|((other, own), kept)| **kept && !own && other.file_name() == by.file_name());
        if let Some(((other, _), _)) = same {
            return Err(term::error(format!(
                "{} replaces a dependency's file, and bender would replace {} too, as it \
                 replaces every file of that name",
                mark::path(by.display()),
                mark::path(other.display())
            ))
            .into());
        }
    }
    Ok(overriding)
}

/// The `Bender.yml` for `package`, whose units `ordered` lists in order
/// from `index`, each in `overriding` replacing a dependency's.
fn manifest(
    plan: &Plan,
    package: &Package,
    index: &Index,
    ordered: &[usize],
    overriding: &BTreeSet<PathBuf>,
) -> Result<String> {
    let Some(name) = package.name.as_deref() else {
        return Err(term::error(format!(
            "the package has no name, which {BENDER_MANIFEST} needs"
        ))
        .help(format!("give it one in {}", mark::code("[package] name")))
        .into());
    };
    let dev = format!("{name}_dev");
    let targets = Targets::new(package, &dev)?;
    let root = &package.root;
    let mut text = String::from(HEADER);
    text += &format!("package:\n  name: {}\n", scalar(name));

    let mut dependencies = String::new();
    let required = (package
        .dependencies
        .iter()
        .map(|(name, it)| (name, it, false)))
    .chain(
        package
            .dev_dependencies
            .iter()
            .map(|(name, it)| (name, it, true)),
    );
    for (dependency, requirement, is_dev) in required {
        let mut keys: Vec<(&str, String)> = Vec::new();
        match &requirement.source {
            Some(Source::Git { url, at }) => {
                keys.push(("git", quoted(url)));
                match at {
                    At::Version(required) => keys.push(("version", quoted(&version(required)))),
                    At::Rev(rev) => keys.push(("rev", quoted(rev))),
                }
            }
            Some(Source::Path(dir)) => {
                keys.push(("path", quoted(&written(&relative(root, dir)))));
            }
            None => {
                return Err(Error::failed(format!(
                    "{} says nowhere where it comes from",
                    mark::name(dependency)
                )));
            }
        }
        let target = match is_dev {
            true => Some(dev.clone()),
            false => targets.optional(dependency),
        };
        if let Some(target) = target {
            keys.push(("target", quoted(&target)));
        }
        if !requirement.default_features
            && let Some(of) = plan.dependency(root, dependency)
            && !of.default_features.is_empty()
        {
            return Err(term::error(format!(
                "{} turns off the default features of {}, which bender cannot",
                mark::code("default-features = false"),
                mark::name(dependency)
            ))
            .help("bender builds a package's default features whatever its dependents ask")
            .into());
        }
        let passed = targets.passed(dependency, requirement);
        if !passed.is_empty() {
            keys.push(("pass_targets", format!("[{}]", passed.join(", "))));
        }
        let keys: Vec<String> = (keys.into_iter())
            .map(|(key, value)| format!("{key}: {value}"))
            .collect();
        dependencies += &format!("  {}: {{ {} }}\n", scalar(dependency), keys.join(", "));
    }
    if !dependencies.is_empty() {
        text += &format!("\ndependencies:\n{dependencies}");
    }

    let mut exported: Vec<String> = Vec::new();
    for dir in &package.include_dirs {
        exported.push(scalar(&written(&relative(root, dir))));
    }
    for (feature, declared) in &package.features {
        for dir in &declared.include_dirs {
            let dir = scalar(&written(&relative(root, dir)));
            exported.push(match targets.of_feature(feature) {
                Some(target) => format!("{{ target: {}, dir: {dir} }}", quoted(&target)),
                None => dir,
            });
        }
    }
    let mut seen = BTreeSet::new();
    exported.retain(|dir| seen.insert(dir.clone()));
    if !exported.is_empty() {
        text.push_str("\nexport_include_dirs:\n");
        for dir in &exported {
            text += &format!("  - {dir}\n");
        }
    }

    // Each file in the first group of what builds it that comes no earlier
    // than a group holding a file it uses, so that bender, which compiles
    // the groups in order, compiles it after them.
    let naming = package.naming()?;
    let mut groups: Vec<(Group, Vec<usize>)> = Vec::new();
    let mut placed: HashMap<usize, usize> = HashMap::new();
    for &file in ordered {
        let path = &index.summaries()[file].path;
        let group = Group {
            target: targets.of_file(naming.features(path), naming.dev(path)),
            overrides: overriding.contains(path),
        };
        let after = (index.dependencies(file))
            .filter_map(|needed| placed.get(&needed).copied())
            .max()
            .unwrap_or_default();
        let at = match (groups.iter().enumerate().skip(after)).find(|(_, (it, _))| *it == group) {
            Some((at, _)) => at,
            None => {
                groups.push((group, Vec::new()));
                groups.len() - 1
            }
        };
        groups[at].1.push(file);
        placed.insert(file, at);
    }

    let private: Vec<String> = (plan.suffix_dirs.iter())
        .map(|dir| scalar(&written(&relative(root, dir))))
        .collect();
    if !groups.is_empty() {
        text.push_str("\nsources:\n");
    }
    for (group, files) in &groups {
        let paths = files.iter().map(|&file| {
            let path = &index.summaries()[file].path;
            scalar(&written(&relative(root, path)))
        });
        let plain = group.target.is_none() && !group.overrides && private.is_empty();
        if plain {
            for path in paths {
                text += &format!("  - {path}\n");
            }
            continue;
        }
        let mut keys = Vec::new();
        if let Some(target) = &group.target {
            keys.push(format!("target: {}", scalar(target)));
        }
        if group.overrides {
            keys.push("override_files: true".to_string());
        }
        // Bender ignores an override group's include directories, since its
        // files are compiled where the files they replace are.
        if !private.is_empty() && !group.overrides {
            keys.push(format!("include_dirs: [{}]", private.join(", ")));
        }
        keys.push("files:".to_string());
        text += &format!("  - {}\n", keys.join("\n    "));
        for path in paths {
            text += &format!("      - {path}\n");
        }
    }
    Ok(text)
}

/// The files of a `sources` group share what builds them, and whether they
/// replace a dependency's.
#[derive(PartialEq, Eq)]
struct Group {
    target: Option<String>,
    overrides: bool,
}

/// The bender targets the package's features and `dev` become.
struct Targets<'a> {
    package: &'a Package,
    dev: &'a str,
}

impl<'a> Targets<'a> {
    /// Refuses a name a target cannot have.
    fn new(package: &'a Package, dev: &'a str) -> Result<Targets<'a>> {
        let names = (package.features.keys().map(String::as_str)).chain([dev]);
        for name in names {
            let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
            if name.starts_with('-') || !name.chars().all(allowed) {
                return Err(Error::failed(format!(
                    "{} cannot be a bender target, which holds letters, digits, `.`, `_` and \
                     `-`, and starts with no `-`",
                    mark::name(name)
                )));
            }
        }
        Ok(Targets { package, dev })
    }

    /// The target under which `feature` builds: none for a default one,
    /// since bender has no defaults and so builds them always.
    fn of_feature(&self, feature: &str) -> Option<String> {
        (!self.package.default_features.iter().any(|it| it == feature)).then(|| feature.to_string())
    }

    /// The target under which any of `features` builds, or none if one of
    /// them always builds.
    fn any(&self, features: &[&str]) -> Option<String> {
        let targets: Option<Vec<String>> =
            (features.iter()).map(|it| self.of_feature(it)).collect();
        match targets?.as_slice() {
            [] => None,
            [one] => Some(one.clone()),
            many => Some(format!("any({})", many.join(", "))),
        }
    }

    /// The target of a file the features `features` name, and `dev` too if
    /// `dev`.
    fn of_file(&self, features: Vec<&str>, dev: bool) -> Option<String> {
        let features = match features.is_empty() {
            true => None,
            false => self.any(&features),
        };
        match (dev, features) {
            (false, features) => features,
            (true, None) => Some(self.dev.to_string()),
            (true, Some(features)) => Some(format!("all({}, {features})", self.dev)),
        }
    }

    /// The target of an optional dependency, the features naming it, or
    /// none for one every build has.
    fn optional(&self, dependency: &str) -> Option<String> {
        if !self.package.manifest().optional(dependency) {
            return None;
        }
        let naming: Vec<&str> = (self.package.features.iter())
            .filter(|(_, feature)| feature.dependencies.iter().any(|(it, _)| it == dependency))
            .map(|(name, _)| name.as_str())
            .collect();
        self.any(&naming)
    }

    /// What `pass_targets` lists for `dependency`: the features its
    /// requirement enables there always, and those a feature here enables
    /// under that feature's target.
    fn passed(&self, dependency: &str, requirement: &Requirement) -> Vec<String> {
        let mut passed: Vec<String> = requirement.features.iter().map(|it| quoted(it)).collect();
        for (feature, declared) in &self.package.features {
            for (_, enabled) in (declared.dependencies.iter()).filter(|(it, _)| it == dependency) {
                let Some(enabled) = enabled else {
                    continue;
                };
                passed.push(match self.of_feature(feature) {
                    Some(target) => format!(
                        "{{ target: {}, pass: {} }}",
                        quoted(&target),
                        quoted(enabled)
                    ),
                    None => quoted(enabled),
                });
            }
        }
        let mut seen = BTreeSet::new();
        passed.retain(|it| seen.insert(it.clone()));
        passed
    }
}

/// `text` as a YAML scalar: as written where YAML reads it back as that
/// string, and quoted otherwise.
fn scalar(text: &str) -> String {
    let plain = (text.chars().next()).is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | '+'))
        && text.parse::<f64>().is_err()
        && !matches!(
            text.to_ascii_lowercase().as_str(),
            "true" | "false" | "yes" | "no" | "on" | "off" | "null" | "y" | "n"
        );
    match plain {
        true => text.to_string(),
        false => quoted(text),
    }
}

/// `text` as a double-quoted YAML scalar.
fn quoted(text: &str) -> String {
    let mut quoted = String::from('"');
    for c in text.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\t' => quoted.push_str("\\t"),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// `requirement` as an entry of `[dependencies]`. A `Bender.yml` enables no
/// features, so only where it comes from is written.
fn entry(requirement: &Requirement) -> InlineTable {
    let mut entry = InlineTable::new();
    match &requirement.source {
        Some(Source::Git { url, at }) => {
            entry.insert("git", url.as_str().into());
            match at {
                At::Version(required) => entry.insert("version", version(required).into()),
                At::Rev(rev) => entry.insert("rev", rev.as_str().into()),
            };
        }
        Some(Source::Path(path)) => {
            entry.insert("path", path.to_string_lossy().as_ref().into());
        }
        None => {}
    }
    entry
}

/// `required` as a person writes it: a lone caret requirement without its
/// `^`, which a bare version means already, in either manifest.
fn version(required: &VersionReq) -> String {
    let written = required.to_string();
    match &required.comparators[..] {
        [only] if only.op == Op::Caret => written.trim_start_matches('^').to_string(),
        _ => written,
    }
}
