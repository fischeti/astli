//! Names across files: which file declares each, and which files a design
//! needs.

use std::collections::VecDeque;
use std::fmt;

use rustc_hash::{FxHashMap, FxHashSet};

use super::summary::{Declaration, Declares, Reference, Summary, Uses};

/// The summaries of a set of files, and each top-level name resolved to the
/// file that declares it.
///
/// A file is indexed by its position in the summaries it was built from, and
/// depends on every file that declares a name it references. A file that
/// binds into what another declares, at its top level, is needed wherever
/// that one is, since the `bind` applies wherever the file is compiled; one
/// a module holds applies only once the module is elaborated, and its file
/// is needed as the module's is. A name declared more than once resolves
/// to the last declaration, and all of them are
/// [`redeclared`](Index::redeclared).
#[derive(Debug, Clone)]
pub struct Index {
    summaries: Vec<Summary>,
    /// Each name's declaration: its file, and its place in that file's.
    declared: FxHashMap<String, (usize, usize)>,
    /// Each file's dependencies, with the first reference to each.
    dependencies: Vec<Vec<(usize, usize)>>,
    /// The files that bind into what each file declares, with the first
    /// such `bind` in each.
    bound: Vec<Vec<(usize, usize)>>,
    /// Every declaration of each name declared more than once, by name in
    /// the order first declared.
    redeclared: Vec<Vec<(usize, usize)>>,
}

/// A name more than one declaration declares.
#[derive(Debug, Clone)]
pub struct Redeclared<'a> {
    pub name: &'a str,
    /// In the order indexed; the name resolves to the last.
    pub declarations: Vec<&'a Declaration>,
    /// Whether one of the files asked about uses the name, which makes which
    /// declaration it means matter.
    pub referenced: bool,
}

/// One file on a chain from a top to a file it needs, and the reference that
/// led here: the file before's use of what this one declares, or this one's
/// `bind` into what the file before declares.
#[derive(Debug, Clone, Copy)]
pub struct Step<'a> {
    pub file: usize,
    pub via: Option<&'a Reference>,
    /// Whether `via` is this file's `bind`.
    pub binds: bool,
}

/// A top named that no file declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownTop(pub String);

impl fmt::Display for UnknownTop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "no file declares the top `{}`", self.0)
    }
}

impl std::error::Error for UnknownTop {}

/// Packages the language provides, which no file declares.
const BUILT_IN: &[&str] = &["std"];

impl Index {
    pub fn new(summaries: Vec<Summary>) -> Index {
        let mut declared = FxHashMap::default();
        let mut redeclared: Vec<Vec<(usize, usize)>> = Vec::new();
        // Each name declared more than once, by its place in `redeclared`.
        let mut again: FxHashMap<&str, usize> = FxHashMap::default();
        for (file, summary) in summaries.iter().enumerate() {
            for (at, declaration) in summary.declarations.iter().enumerate() {
                let name = declaration.name.as_str();
                let Some(earlier) = declared.insert(name.to_string(), (file, at)) else {
                    continue;
                };
                let group = *again.entry(name).or_insert_with(|| {
                    redeclared.push(vec![earlier]);
                    redeclared.len() - 1
                });
                redeclared[group].push((file, at));
            }
        }

        let dependencies = summaries
            .iter()
            .enumerate()
            .map(|(file, summary)| {
                let mut seen = FxHashSet::default();
                summary
                    .references
                    .iter()
                    .enumerate()
                    .filter_map(|(at, reference)| {
                        let &(declaring, _) = declared.get(&reference.name)?;
                        (declaring != file && seen.insert(declaring)).then_some((declaring, at))
                    })
                    .collect()
            })
            .collect();

        let mut bound: Vec<Vec<(usize, usize)>> = vec![Vec::new(); summaries.len()];
        for (file, summary) in summaries.iter().enumerate() {
            for (at, reference) in summary.references.iter().enumerate() {
                let Some(&(target, _)) = declared.get(&reference.name) else {
                    continue;
                };
                let first = !bound[target].iter().any(|&(it, _)| it == file);
                if reference.uses == (Uses::Bind { held: false }) && target != file && first {
                    bound[target].push((file, at));
                }
            }
        }

        Index {
            summaries,
            declared,
            dependencies,
            bound,
            redeclared,
        }
    }

    /// The files reaching `file` reaches next, each with the file holding the
    /// reference that leads there and its place: one `file` depends on, its
    /// own reference; one that binds into it, that one's `bind`.
    fn next(&self, file: usize) -> impl Iterator<Item = (usize, (usize, usize))> + '_ {
        let dependencies = (self.dependencies[file].iter()).map(move |&(it, at)| (it, (file, at)));
        let bound = self.bound[file].iter().map(|&(it, at)| (it, (it, at)));
        dependencies.chain(bound)
    }

    pub fn summaries(&self) -> &[Summary] {
        &self.summaries
    }

    /// The file that declares `name`, and the declaration.
    pub fn declaration(&self, name: &str) -> Option<(usize, &Declaration)> {
        let &(file, at) = self.declared.get(name)?;
        Some((file, &self.summaries[file].declarations[at]))
    }

    /// The files `file` references a name in, in the order first referenced.
    pub fn dependencies(&self, file: usize) -> impl Iterator<Item = usize> + '_ {
        self.dependencies[file].iter().map(|&(file, _)| file)
    }

    /// Every file the files declaring `tops` need, those included, in the
    /// order indexed: what they depend on, and what binds into them.
    ///
    /// An encrypted file is reached whatever the tops, since what it declares
    /// is hidden, and so is every file it needs.
    pub fn reachable(&self, tops: &[impl AsRef<str>]) -> Result<Vec<usize>, UnknownTop> {
        let mut reached = vec![false; self.summaries.len()];
        let mut open = self.resolve(tops)?;
        let encrypted = self.summaries.iter().enumerate();
        open.extend(
            encrypted
                .filter(|(_, summary)| summary.encrypted)
                .map(|(file, _)| file),
        );
        while let Some(file) = open.pop() {
            if !std::mem::replace(&mut reached[file], true) {
                open.extend(self.next(file).map(|(next, _)| next));
            }
        }
        Ok((0..reached.len()).filter(|&file| reached[file]).collect())
    }

    /// `files` with each after the files it depends on, and otherwise in the
    /// order given. Files that depend on each other stay in the order given.
    pub fn ordered(&self, files: &[usize]) -> Vec<usize> {
        let wanted: FxHashSet<usize> = files.iter().copied().collect();
        let mut seen = FxHashSet::default();
        let mut out = Vec::with_capacity(files.len());
        for &file in files {
            self.visit(file, &wanted, &mut seen, &mut out);
        }
        out
    }

    fn visit(
        &self,
        file: usize,
        wanted: &FxHashSet<usize>,
        seen: &mut FxHashSet<usize>,
        out: &mut Vec<usize>,
    ) {
        if !seen.insert(file) {
            return;
        }
        for dependency in self.dependencies(file) {
            if wanted.contains(&dependency) {
                self.visit(dependency, wanted, seen, out);
            }
        }
        out.push(file);
    }

    /// The modules and programs no file references: what could be a top.
    ///
    /// Many of these do not compile on their own, such as a block meant to be
    /// instantiated elsewhere, so they are candidates to choose from rather
    /// than tops to use.
    pub fn tops(&self) -> Vec<(usize, &Declaration)> {
        let referenced: FxHashSet<&str> = self
            .summaries
            .iter()
            .flat_map(|summary| &summary.references)
            .map(|reference| reference.name.as_str())
            .collect();
        self.summaries
            .iter()
            .enumerate()
            .flat_map(|(file, summary)| summary.declarations.iter().map(move |d| (file, d)))
            .filter(|(_, declaration)| {
                matches!(declaration.declares, Declares::Module | Declares::Program)
                    && !referenced.contains(declaration.name.as_str())
            })
            .collect()
    }

    /// The shortest chain from a file declaring one of `tops` to `file`, or
    /// `None` if `file` is not needed.
    pub fn why(
        &self,
        tops: &[impl AsRef<str>],
        file: usize,
    ) -> Result<Option<Vec<Step<'_>>>, UnknownTop> {
        let mut reached = vec![false; self.summaries.len()];
        // The file before each, and where the reference leading to it is.
        let mut parent: Vec<Option<(usize, (usize, usize))>> = vec![None; self.summaries.len()];
        let mut open = VecDeque::new();
        for top in self.resolve(tops)? {
            if !std::mem::replace(&mut reached[top], true) {
                open.push_back(top);
            }
        }
        while let Some(at) = open.pop_front() {
            for (next, reference) in self.next(at) {
                if !std::mem::replace(&mut reached[next], true) {
                    parent[next] = Some((at, reference));
                    open.push_back(next);
                }
            }
        }
        if !reached[file] {
            return Ok(None);
        }

        let mut chain = Vec::new();
        let mut at = file;
        while let Some((before, (holder, reference))) = parent[at] {
            let via = Some(&self.summaries[holder].references[reference]);
            chain.push(Step {
                file: at,
                via,
                binds: holder == at,
            });
            at = before;
        }
        chain.push(Step {
            file: at,
            via: None,
            binds: false,
        });
        chain.reverse();
        Ok(Some(chain))
    }

    /// The first reference in `files` to each name that must be declared at
    /// the top level and is declared in no indexed file.
    pub fn undeclared(&self, files: &[usize]) -> Vec<(usize, &Reference)> {
        let mut seen = FxHashSet::default();
        files
            .iter()
            .flat_map(|&file| {
                self.summaries[file]
                    .references
                    .iter()
                    .map(move |r| (file, r))
            })
            .filter(|(_, reference)| {
                reference.uses.is_global()
                    && !self.declared.contains_key(&reference.name)
                    && !BUILT_IN.contains(&reference.name.as_str())
                    && seen.insert(reference.name.as_str())
            })
            .collect()
    }

    /// Each name declared more than once that one of `files` declares or
    /// uses, in the order first declared.
    pub fn redeclared(&self, files: &[usize]) -> Vec<Redeclared<'_>> {
        let wanted: FxHashSet<usize> = files.iter().copied().collect();
        let used: FxHashSet<&str> = (files.iter())
            .flat_map(|&file| &self.summaries[file].references)
            .map(|reference| reference.name.as_str())
            .collect();
        let declaration = |&(file, at): &(usize, usize)| &self.summaries[file].declarations[at];
        (self.redeclared.iter())
            .filter_map(|group| {
                let name = declaration(&group[0]).name.as_str();
                let referenced = used.contains(name);
                let declared = group.iter().any(|(file, _)| wanted.contains(file));
                (referenced || declared).then(|| Redeclared {
                    name,
                    declarations: group.iter().map(declaration).collect(),
                    referenced,
                })
            })
            .collect()
    }

    fn resolve(&self, tops: &[impl AsRef<str>]) -> Result<Vec<usize>, UnknownTop> {
        tops.iter()
            .map(|top| {
                let top = top.as_ref();
                let found = self.declared.get(top).map(|&(file, _)| file);
                found.ok_or_else(|| UnknownTop(top.to_string()))
            })
            .collect()
    }
}
