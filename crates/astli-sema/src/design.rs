//! The files of a design, and the names they declare where other files can
//! reach them.

use std::ops::Index;
use std::sync::{Arc, OnceLock};

use astli_syntax::SyntaxKind::PACKAGE_KW;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::hir::{
    Bind, Class, Data, ExprId, ExprKind, Hir, Member, Port, ScopeId, ScopeKind, Symbol, SymbolId,
    SymbolKind, Type, TypeKind,
};
use crate::resolve::{Names, Resolution, Resolver, STD};

/// How deep `export`s may lead from one package to another before the
/// answer is unknown, which a cycle of them would otherwise not give.
const EXPORT_DEPTH: usize = 16;

/// How many classes up from one its members are looked for, past which they
/// are unknown, as a cycle of classes extending each other would be.
const BASE_DEPTH: usize = 64;

/// What a class extends.
enum Base {
    None,
    Class(SymbolRef),
    /// A class no file declares, a type parameter, or what sema cannot
    /// tell.
    Unknown,
}

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
    files: Vec<Arc<Hir>>,
    /// What the names each file uses refer to, resolved when first asked.
    names: Vec<OnceLock<Names>>,
    /// For each file, for each scope, the names declared there.
    tables: Vec<Vec<FxHashMap<Box<str>, SymbolId>>>,
    /// For each file, the scope each symbol is declared in.
    homes: Vec<Vec<ScopeId>>,
    definitions: FxHashMap<Box<str>, SymbolRef>,
    packages: FxHashMap<Box<str>, SymbolRef>,
    /// The packages declared more than once.
    ambiguous: FxHashSet<Box<str>>,
    /// The modules, interfaces and programs declared more than once.
    twice: FxHashSet<Box<str>>,
    /// The names regions left opaque at the top of a file spell.
    opaque: FxHashSet<Box<str>>,
    /// The instances bound into every instance of each definition.
    binds: FxHashMap<Box<str>, Vec<Bound>>,
}

/// An instance a `bind` adds, and the definition holding the `bind`: `None`
/// for one at a file's top level, which applies wherever the file is
/// compiled. One a definition holds applies once that one is elaborated.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Bound {
    pub instance: SymbolRef,
    pub holder: Option<SymbolRef>,
}

impl Design {
    /// A design of `files`, in filelist order. A file is shared, so a design
    /// made again after one file changes keeps the others as they are.
    pub fn new(files: Vec<Arc<Hir>>) -> Design {
        let mut definitions = FxHashMap::default();
        let mut packages = FxHashMap::default();
        let mut ambiguous = FxHashSet::default();
        let mut twice = FxHashSet::default();
        let mut opaque = FxHashSet::default();
        let mut binds: FxHashMap<Box<str>, Vec<Bound>> = FxHashMap::default();
        let mut tables = Vec::with_capacity(files.len());
        let mut homes = Vec::with_capacity(files.len());
        for (index, hir) in files.iter().enumerate() {
            let file = FileId(index as u32);
            let mut scopes = Vec::with_capacity(hir.scopes.len());
            let mut home = vec![hir.root(); hir.symbols.len()];
            for (id, scope) in hir.scopes() {
                let mut table = FxHashMap::default();
                for member in &scope.members {
                    match member {
                        Member::Declare(symbol) => {
                            home[symbol.index()] = id;
                            // The first declaration of a name stands, but a
                            // forward typedef gives way to what it announces;
                            // a second is an error that is not this table's
                            // to report.
                            let name = hir[*symbol].name.text.clone();
                            let first = table.entry(name).or_insert(*symbol);
                            if let SymbolKind::Forward = hir[*first].kind {
                                *first = *symbol;
                            }
                        }
                        Member::Bind(Bind {
                            target: Some(target),
                            chosen: false,
                            instances,
                        }) => {
                            let holder =
                                definition_of(hir, id).map(|symbol| SymbolRef { file, symbol });
                            let bound = binds.entry(target.text.clone()).or_default();
                            bound.extend(instances.iter().map(|&symbol| Bound {
                                instance: SymbolRef { file, symbol },
                                holder,
                            }));
                        }
                        _ => {}
                    }
                }
                scopes.push(table);
            }
            tables.push(scopes);
            homes.push(home);

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
                        if definitions.insert(name.clone(), at).is_some() {
                            twice.insert(name);
                        }
                    }
                }
            }
        }
        Design {
            names: files.iter().map(|_| OnceLock::new()).collect(),
            files,
            tables,
            homes,
            definitions,
            packages,
            ambiguous,
            twice,
            opaque,
            binds,
        }
    }

    /// The instances bound into every instance of the definition `name`.
    pub(crate) fn binds(&self, name: &str) -> &[Bound] {
        self.binds.get(name).map_or(&[], Vec::as_slice)
    }

    /// The definitions `tops` reach: those they instantiate, in any
    /// generate arm whatever its condition, and those the `bind`s that apply
    /// add, one at a file's top level into a definition reached, or one a
    /// definition reached holds.
    pub(crate) fn reached(&self, tops: &[impl AsRef<str>]) -> FxHashSet<SymbolRef> {
        // What each definition instantiates and binds, by name.
        let mut uses: FxHashMap<SymbolRef, Vec<&str>> = FxHashMap::default();
        for (file, hir) in self.files() {
            for (id, scope) in hir.scopes() {
                let Some(owner) = definition_of(hir, id) else {
                    continue;
                };
                for member in &scope.members {
                    let symbols = match member {
                        Member::Declare(symbol) => std::slice::from_ref(symbol),
                        Member::Bind(bind) => &bind.instances,
                        _ => &[],
                    };
                    for &symbol in symbols {
                        if let SymbolKind::Instance(instance) = &hir[symbol].kind {
                            let at = SymbolRef {
                                file,
                                symbol: owner,
                            };
                            uses.entry(at).or_default().push(&instance.definition.text);
                        }
                    }
                }
            }
        }
        let mut open: Vec<SymbolRef> = (tops.iter())
            .filter_map(|top| self.definition(top.as_ref()))
            .collect();
        let mut reached: FxHashSet<SymbolRef> = open.iter().copied().collect();
        loop {
            while let Some(at) = open.pop() {
                let names = uses.get(&at).map_or(&[][..], Vec::as_slice);
                for definition in names.iter().filter_map(|name| self.definition(name)) {
                    if reached.insert(definition) {
                        open.push(definition);
                    }
                }
            }
            for (target, bound) in &self.binds {
                if !self
                    .definition(target)
                    .is_some_and(|it| reached.contains(&it))
                {
                    continue;
                }
                for bound in bound.iter().filter(|it| it.holder.is_none()) {
                    let SymbolKind::Instance(instance) = &self.symbol(bound.instance).kind else {
                        continue;
                    };
                    if let Some(definition) = self.definition(&instance.definition.text)
                        && reached.insert(definition)
                    {
                        open.push(definition);
                    }
                }
            }
            if open.is_empty() {
                return reached;
            }
        }
    }

    pub fn files(&self) -> impl Iterator<Item = (FileId, &Hir)> {
        (self.files.iter().enumerate()).map(|(i, hir)| (FileId(i as u32), &**hir))
    }

    pub fn symbol(&self, at: SymbolRef) -> &Symbol {
        &self[at.file][at.symbol]
    }

    /// The scope `at` is declared in.
    pub fn home(&self, at: SymbolRef) -> ScopeId {
        self.homes[at.file.index()][at.symbol.index()]
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
    /// exports. The built-in `std`, which no file declares, has the members
    /// the standard gives it, none of them modelled.
    pub(crate) fn member_of(&self, package: &str, name: &str, depth: usize) -> Resolution {
        let Some(at) = self.package(package) else {
            return match package {
                "std" if !STD.contains(&name) => Resolution::Undeclared,
                _ => Resolution::Unknown,
            };
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

    /// Whether more than one file declares the module, interface or program
    /// `name`, which one a compiler keeps being a guess.
    pub fn is_declared_twice(&self, name: &str) -> bool {
        self.twice.contains(name)
    }

    /// Whether more than one file declares the package `name`.
    pub fn is_ambiguous(&self, package: &str) -> bool {
        self.ambiguous.contains(package)
    }

    /// What each name `file` uses refers to, resolved once however often
    /// it is asked, from any thread.
    pub fn names(&self, file: FileId) -> &Names {
        self.names[file.index()].get_or_init(|| Resolver::new(self, file).run())
    }

    /// `name` as a member of `class`: one it declares, or inherits.
    pub(crate) fn class_member(&self, class: SymbolRef, name: &str) -> Resolution {
        let SymbolKind::Class(Class { scope, .. }) = self.symbol(class).kind else {
            return Resolution::Unknown;
        };
        if let Some(symbol) = self.declared(class.file, scope, name) {
            return Resolution::Declared(SymbolRef {
                file: class.file,
                symbol,
            });
        }
        if self.is_opaque(class.file, scope) {
            return Resolution::Unknown;
        }
        self.inherited(class, name)
    }

    /// `name` as a member `class` inherits from the classes it extends;
    /// unknown where one of them is not known, or holds what is not lowered.
    pub(crate) fn inherited(&self, class: SymbolRef, name: &str) -> Resolution {
        let mut class = class;
        for _ in 0..BASE_DEPTH {
            let base = match self.base(class) {
                Base::None => return Resolution::Undeclared,
                Base::Unknown => return Resolution::Unknown,
                Base::Class(base) => base,
            };
            let SymbolKind::Class(Class { scope, .. }) = self.symbol(base).kind else {
                return Resolution::Unknown;
            };
            if let Some(symbol) = self.declared(base.file, scope, name) {
                return Resolution::Declared(SymbolRef {
                    file: base.file,
                    symbol,
                });
            }
            if self.is_opaque(base.file, scope) {
                return Resolution::Unknown;
            }
            class = base;
        }
        Resolution::Unknown
    }

    /// The class `class` extends, its name found from where `class` is
    /// declared.
    fn base(&self, class: SymbolRef) -> Base {
        let SymbolKind::Class(Class {
            extends: Some((base, _)),
            ..
        }) = &self.symbol(class).kind
        else {
            return Base::None;
        };
        match base.kind {
            TypeKind::Named(name) => {
                self.class_named(class.file, self.home(class), name, 0, &mut 0)
            }
            _ => Base::Unknown,
        }
    }

    /// The class `at` is, or a typedef of names, as `type_id` in
    /// `T::type_id::create()`.
    pub(crate) fn class_at(&self, at: SymbolRef) -> Option<SymbolRef> {
        match &self.symbol(at).kind {
            SymbolKind::Class(_) => Some(at),
            SymbolKind::Typedef(Type {
                kind: TypeKind::Named(name),
                dims,
                packed,
            }) if dims.len() == *packed => {
                match self.class_named(at.file, self.home(at), *name, 0, &mut 0) {
                    Base::Class(class) => Some(class),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The class a handle holds, `selects` of its elements in: a port, net
    /// or variable whose type names one, directly or through typedefs, and
    /// with as many unpacked dimensions. With fewer selects, it is an array
    /// of handles, whose members are its methods.
    pub(crate) fn class_of(&self, handle: SymbolRef, selects: Option<usize>) -> Option<SymbolRef> {
        let (SymbolKind::Port(Port { ty, .. })
        | SymbolKind::Variable(Data { ty, .. })
        | SymbolKind::Net(Data { ty, .. })) = &self.symbol(handle).kind
        else {
            return None;
        };
        let TypeKind::Named(name) = ty.kind else {
            return None;
        };
        let mut unpacked = ty.dims.len() - ty.packed;
        match self.class_named(handle.file, self.home(handle), name, 0, &mut unpacked) {
            Base::Class(class) if selects == Some(unpacked) => Some(class),
            _ => None,
        }
    }

    /// The class `expr`, a type's name in `file`, names from `scope`;
    /// `unpacked` counts the unpacked dimensions of the typedefs it goes
    /// through, `depth` of them so far.
    fn class_named(
        &self,
        file: FileId,
        scope: ScopeId,
        expr: ExprId,
        depth: usize,
        unpacked: &mut usize,
    ) -> Base {
        let hir = &self[file];
        let found = match &hir[expr].kind {
            ExprKind::Name(name) => self.visible(file, scope, name),
            ExprKind::Scoped { base, name } => match &hir[*base].kind {
                ExprKind::Name(package) => self.member_of(package, &name.text, 0),
                _ => Resolution::Unknown,
            },
            // `B #(T)`.
            ExprKind::Call { callee, .. } => {
                return self.class_named(file, scope, *callee, depth, unpacked);
            }
            _ => Resolution::Unknown,
        };
        let Resolution::Declared(at) = found else {
            return Base::Unknown;
        };
        match &self.symbol(at).kind {
            SymbolKind::Class(_) => Base::Class(at),
            SymbolKind::Typedef(Type {
                kind: TypeKind::Named(name),
                dims,
                packed,
            }) if depth < BASE_DEPTH => {
                *unpacked += dims.len() - packed;
                self.class_named(at.file, self.home(at), *name, depth + 1, unpacked)
            }
            _ => Base::Unknown,
        }
    }

    /// What `name` is declared or imported as in `scope` of `file`, or in
    /// one around it.
    fn visible(&self, file: FileId, scope: ScopeId, name: &str) -> Resolution {
        let mut at = Some(scope);
        while let Some(scope) = at {
            if let Some(symbol) = self.declared(file, scope, name) {
                return Resolution::Declared(SymbolRef { file, symbol });
            }
            if let found @ Resolution::Declared(_) = self.imported(file, scope, name, 0) {
                return found;
            }
            at = self[file][scope].parent;
        }
        Resolution::Unknown
    }

    /// Whether `scope` of `file` holds a region not lowered, which may
    /// declare anything.
    fn is_opaque(&self, file: FileId, scope: ScopeId) -> bool {
        (self[file][scope].members.iter()).any(|member| matches!(member, Member::Opaque(_)))
    }

    /// The symbol `name` declares directly in `scope` of `file`.
    pub(crate) fn declared(&self, file: FileId, scope: ScopeId, name: &str) -> Option<SymbolId> {
        self.tables[file.index()][scope.index()].get(name).copied()
    }
}

/// The definition whose scope `scope` is or is inside, if any.
fn definition_of(hir: &Hir, scope: ScopeId) -> Option<SymbolId> {
    let mut at = Some(scope);
    while let Some(scope) = at {
        if hir[scope].kind == ScopeKind::Definition {
            return hir[scope].owner;
        }
        at = hir[scope].parent;
    }
    None
}

impl Index<FileId> for Design {
    type Output = Hir;

    fn index(&self, file: FileId) -> &Hir {
        &self.files[file.index()]
    }
}
