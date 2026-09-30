//! The files of a design, and the names they declare where other files can
//! reach them.

use std::ops::Index;

use astli_syntax::SyntaxKind::PACKAGE_KW;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::hir::{Hir, Member, ScopeId, Symbol, SymbolId, SymbolKind};
use crate::resolve::{Names, Resolver};

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
}

impl Design {
    /// A design of `files`, in filelist order.
    pub fn new(files: Vec<Hir>) -> Design {
        let mut definitions = FxHashMap::default();
        let mut packages = FxHashMap::default();
        let mut ambiguous = FxHashSet::default();
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
