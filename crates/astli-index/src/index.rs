//! Names across files: which file declares each, and which files a design
//! needs.

use std::collections::VecDeque;
use std::fmt;

use rustc_hash::{FxHashMap, FxHashSet};

use super::summary::{Declaration, Declares, Reference, Summary};

/// The summaries of a set of files, and each top-level name resolved to the
/// file that declares it.
///
/// A file is indexed by its position in the summaries it was built from, and
/// depends on every file that declares a name it references. A name declared
/// twice resolves to the later declaration, and both are
/// [`redeclared`](Index::redeclared).
#[derive(Debug, Clone)]
pub struct Index {
    summaries: Vec<Summary>,
    /// Each name's declaration: its file, and its place in that file's.
    declared: FxHashMap<String, (usize, usize)>,
    /// Each file's dependencies, with the first reference to each.
    dependencies: Vec<Vec<(usize, usize)>>,
    redeclared: Vec<((usize, usize), (usize, usize))>,
}

/// One file on a chain from a top to a file it needs, and the reference in
/// the file before it that led here.
#[derive(Debug, Clone, Copy)]
pub struct Step<'a> {
    pub file: usize,
    pub via: Option<&'a Reference>,
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
        let mut redeclared = Vec::new();
        for (file, summary) in summaries.iter().enumerate() {
            for (at, declaration) in summary.declarations.iter().enumerate() {
                if let Some(earlier) = declared.insert(declaration.name.clone(), (file, at)) {
                    redeclared.push((earlier, (file, at)));
                }
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

        Index {
            summaries,
            declared,
            dependencies,
            redeclared,
        }
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
    /// order indexed.
    pub fn reachable(&self, tops: &[impl AsRef<str>]) -> Result<Vec<usize>, UnknownTop> {
        let mut reached = vec![false; self.summaries.len()];
        let mut open = self.resolve(tops)?;
        while let Some(file) = open.pop() {
            if !std::mem::replace(&mut reached[file], true) {
                open.extend(self.dependencies(file));
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
        let mut parent: Vec<Option<(usize, usize)>> = vec![None; self.summaries.len()];
        let mut open = VecDeque::new();
        for top in self.resolve(tops)? {
            if !std::mem::replace(&mut reached[top], true) {
                open.push_back(top);
            }
        }
        while let Some(at) = open.pop_front() {
            for &(next, reference) in &self.dependencies[at] {
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
        while let Some((before, reference)) = parent[at] {
            let via = Some(&self.summaries[before].references[reference]);
            chain.push(Step { file: at, via });
            at = before;
        }
        chain.push(Step {
            file: at,
            via: None,
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

    /// Each name declared again, the earlier declaration first; the later is
    /// the one the name resolves to.
    pub fn redeclared(&self) -> Vec<(&Declaration, &Declaration)> {
        let declaration = |(file, at): (usize, usize)| &self.summaries[file].declarations[at];
        self.redeclared
            .iter()
            .map(|&(earlier, later)| (declaration(earlier), declaration(later)))
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
