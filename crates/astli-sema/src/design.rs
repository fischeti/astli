//! The files of a design, and the names they declare where other files can
//! reach them.

use std::ops::Index;

use astli_syntax::SyntaxKind::PACKAGE_KW;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::hir::{Hir, Member, ScopeId, Symbol, SymbolId, SymbolKind};
use crate::resolve::{Names, Resolution, Resolver};

/// How deep `export`s may lead from one package to another before the
/// answer is unknown, which a cycle of them would otherwise not give.
const EXPORT_DEPTH: usize = 16;

/// A file of a [`Design`], by its position among the files it was made from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(pub(crate) u32);

impl FileId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A symbol of some file of a design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolRef {
    pub file: FileId,
    pub symbol: SymbolId,
}

/// The HIRs of the files a design is made of, each its own compilation unit,
/// and the two namespaces they share: design elements (modules, interfaces,
/// programs) and packages.
///
/// A name declared twice in one of them is the last file's, as a filelist
/// compiled in order has it. Which one a compiler would keep is a guess,
/// though, so a name a package declared twice lacks is unknown rather than
/// undeclared: the other one may declare it.
#[derive(Debug)]
pub struct Design {
    files: Vec<Hir>,
    /// For each file, for each scope, the names declared there.
    tables: Vec<Vec<FxHashMap<Box<str>, SymbolId>>>,
    definitions: FxHashMap<Box<str>, SymbolRef>,
    packages: FxHashMap<Box<str>, SymbolRef>,
    /// The packages declared more than once.
    ambiguous: FxHashSet<Box<str>>,
    /// The names regions left opaque at the top of a file spell.
    opaque: FxHashSet<Box<str>>,
}

impl Design {
    /// A design of `files`, in filelist order.
    pub fn new(files: Vec<Hir>) -> Design {
        let mut definitions = FxHashMap::default();
        let mut packages = FxHashMap::default();
        let mut ambiguous = FxHashSet::default();
        let mut opaque = FxHashSet::default();
        let mut tables = Vec::with_capacity(files.len());
        for (index, hir) in files.iter().enumerate() {
            let file = FileId(index as u32);
            let mut scopes = Vec::with_capacity(hir.scopes.len());
            for (_, scope) in hir.scopes() {
                let mut table = FxHashMap::default();
                for member in &scope.members {
                    if let Member::Declare(symbol) = member {
                        // The first declaration of a name stands; a second
                        // is an error that is not this table's to report.
                        let name = hir[*symbol].name.text.clone();
                        table.entry(name).or_insert(*symbol);
                    }
                }
                scopes.push(table);
            }
            tables.push(scopes);

            for member in &hir[hir.root()].members {
                if let Member::Opaque(body) = member {
                    opaque.extend(body.names.iter().map(|name| name.text.clone()));
                }
                let Member::Declare(symbol) = member else {
                    continue;
                };
                let SymbolKind::Definition { keyword, .. } = hir[*symbol].kind else {
                    continue;
                };
                let name = hir[*symbol].name.text.clone();
                let at = SymbolRef {
                    file,
                    symbol: *symbol,
                };
                match keyword {
                    PACKAGE_KW => {
                        if packages.insert(name.clone(), at).is_some() {
                            ambiguous.insert(name);
                        }
                    }
                    _ => {
                        definitions.insert(name, at);
                    }
                }
            }
        }
        Design {
            files,
            tables,
            definitions,
            packages,
            ambiguous,
            opaque,
        }
    }

    pub fn files(&self) -> impl Iterator<Item = (FileId, &Hir)> {
        (self.files.iter().enumerate()).map(|(i, hir)| (FileId(i as u32), hir))
    }

    pub fn symbol(&self, at: SymbolRef) -> &Symbol {
        &self[at.file][at.symbol]
    }

    /// The module, interface or program called `name`.
    pub fn definition(&self, name: &str) -> Option<SymbolRef> {
        self.definitions.get(name).copied()
    }

    pub fn package(&self, name: &str) -> Option<SymbolRef> {
        self.packages.get(name).copied()
    }

    /// What `scope` of `file` imports as `name`: by name first, then by
    /// wildcard.
    pub(crate) fn imported(
        &self,
        file: FileId,
        scope: ScopeId,
        name: &str,
        depth: usize,
    ) -> Resolution {
        let imports = || {
            self[file][scope]
                .members
                .iter()
                .filter_map(|member| match member {
                    Member::Import(import) if !import.export => Some(import),
                    _ => None,
                })
        };
        let by_name =
            imports().find(|import| import.item.as_ref().is_some_and(|it| &*it.text == name));
        if let Some(import) = by_name {
            // An item its package lacks is the import's error, not the use's.
            return match self.member_of(&import.package.text, name, depth) {
                Resolution::Undeclared => Resolution::Unknown,
                found => found,
            };
        }
        let mut unknown = false;
        for import in imports().filter(|import| import.item.is_none()) {
            match self.member_of(&import.package.text, name, depth) {
                Resolution::Undeclared => {}
                Resolution::Unknown => unknown = true,
                found => return found,
            }
        }
        match unknown {
            true => Resolution::Unknown,
            false => Resolution::Undeclared,
        }
    }

    /// What `package::name` refers to: what the package declares, or
    /// exports.
    pub(crate) fn member_of(&self, package: &str, name: &str, depth: usize) -> Resolution {
        let Some(at) = self.package(package) else {
            return Resolution::Unknown;
        };
        let hir = &self[at.file];
        let SymbolKind::Definition { scope, .. } = hir[at.symbol].kind else {
            return Resolution::Unknown;
        };
        if let Some(symbol) = self.declared(at.file, scope, name) {
            return Resolution::Declared(SymbolRef {
                file: at.file,
                symbol,
            });
        }
        if depth == EXPORT_DEPTH {
            return Resolution::Unknown;
        }
        let mut unknown = self.is_ambiguous(package);
        for member in &hir[scope].members {
            match member {
                Member::Opaque(_) => unknown = true,
                // `export *::*` exports whatever the package imports; `export
                // p::*` and `export p::name` what it imports from `p`.
                Member::Import(export) if export.export => {
                    let found = match (&*export.package.text, &export.item) {
                        ("*", _) => self.imported(at.file, scope, name, depth + 1),
                        (_, Some(item)) if &*item.text != name => continue,
                        (from, _) => self.member_of(from, name, depth + 1),
                    };
                    match found {
                        Resolution::Undeclared => {}
                        Resolution::Unknown => unknown = true,
                        found => return found,
                    }
                }
                _ => {}
            }
        }
        match unknown {
            true => Resolution::Unknown,
            false => Resolution::Undeclared,
        }
    }

    /// Whether a region some file leaves opaque at its top level spells
    /// `name`, which may be a module or package it declares.
    pub(crate) fn spelled_opaque(&self, name: &str) -> bool {
        self.opaque.contains(name)
    }

    /// Whether more than one file declares the package `name`.
    pub(crate) fn is_ambiguous(&self, package: &str) -> bool {
        self.ambiguous.contains(package)
    }

    /// What each name `file` uses refers to.
    pub fn resolve(&self, file: FileId) -> Names {
        Resolver::new(self, file).run()
    }

    /// The symbol `name` declares directly in `scope` of `file`.
    pub(crate) fn declared(&self, file: FileId, scope: ScopeId, name: &str) -> Option<SymbolId> {
        self.tables[file.index()][scope.index()].get(name).copied()
    }
}

impl Index<FileId> for Design {
    type Output = Hir;

    fn index(&self, file: FileId) -> &Hir {
        &self.files[file.index()]
    }
}
