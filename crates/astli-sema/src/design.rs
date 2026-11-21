//! The files of a design, and the names they declare where other files can
//! reach them.

use std::ops::Index;
use std::sync::{Arc, OnceLock};

use astli_syntax::SyntaxKind::PACKAGE_KW;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::hir::{
    Bind, BindId, Class, Data, ExprId, ExprKind, Hir, Member, Port, ScopeId, ScopeKind, StmtId,
    StmtKind, Symbol, SymbolId, SymbolKind, Type, TypeKind,
};
use crate::resolve::{CLASS_METHODS, Names, Object, Resolution, Resolver, STD, Searched};

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
    /// For each file, for each scope, whether it holds a region not
    /// lowered, among its members or its statements.
    hidden: Vec<Vec<bool>>,
    definitions: FxHashMap<Box<str>, SymbolRef>,
    /// The names of the definitions declared inside another, a package
    /// among them, which an instance looks for around itself, and in what
    /// is imported there, before among `definitions`.
    nested: FxHashSet<Box<str>>,
    /// For each file, what each of its `bind`s binds into.
    targets: Vec<Vec<BindTarget>>,
    /// The `bind` adding each instance one adds.
    bound: FxHashMap<SymbolRef, BindId>,
    packages: FxHashMap<Box<str>, SymbolRef>,
    /// The packages declared more than once.
    ambiguous: FxHashSet<Box<str>>,
    /// The modules, interfaces and programs declared more than once.
    twice: FxHashSet<Box<str>>,
    /// The names regions left opaque at the top of a file spell.
    opaque: FxHashSet<Box<str>>,
    /// The instances bound into every instance of each definition.
    binds: FxHashMap<SymbolRef, Vec<Bound>>,
}

/// What a `bind` adds its instances to. A name alone is a definition, or
/// where no definition is so named, an instance, which takes every file's
/// definitions to tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindTarget {
    /// Every instance of the definition a name alone names.
    Definition(SymbolRef),
    /// The instances its paths name: a path, a name alone that is no
    /// definition, or a list after `:`, which binds into those of the
    /// `definition` before it alone. The definition of an instance a path
    /// names is the path's resolution.
    Instances { definition: Option<SymbolRef> },
    /// A list after `:` of instances of a definition no file declares,
    /// which binds into none.
    Undeclared,
}

/// What a lookup through scopes skips, or adds, for the one asking.
#[derive(Clone, Copy, Default)]
pub(crate) struct Filter<'a> {
    /// An instance's definition: only a module, interface, program or
    /// checker is one, and what a file declares at its top level is left to
    /// the definitions every file shares, which keep the last file's where
    /// two declare one.
    pub definition: bool,
    /// Resolving a file in order: whether what a scope of the file declares
    /// stands above the use, or the name is skipped there, so that what is
    /// declared around the scope comes first. `None` where every declaration
    /// does.
    pub declared: Option<&'a dyn Fn(ScopeId, SymbolId) -> bool>,
    /// The object an object's `randomize` randomizes, whose members its
    /// arguments and constraints name before what the call's scope
    /// declares.
    pub object: Option<Object<'a>>,
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
        let mut nested = FxHashSet::default();
        let mut bound = FxHashMap::default();
        let mut packages = FxHashMap::default();
        let mut ambiguous = FxHashSet::default();
        let mut twice = FxHashSet::default();
        let mut opaque = FxHashSet::default();
        let mut binds: FxHashMap<SymbolRef, Vec<Bound>> = FxHashMap::default();
        // Each `bind`, in the order of the scopes holding them, for once
        // every definition is known.
        let mut held = Vec::new();
        let mut tables = Vec::with_capacity(files.len());
        let mut hidden = Vec::with_capacity(files.len());
        for (index, hir) in files.iter().enumerate() {
            let file = FileId(index as u32);
            let mut scopes = Vec::with_capacity(hir.scopes.len());
            for (id, scope) in hir.scopes() {
                let mut table = FxHashMap::default();
                for member in &scope.members {
                    match member {
                        Member::Declare(symbol) => {
                            // The first declaration of a name stands, but a
                            // forward typedef or an `extern` definition gives
                            // way to what it announces; a second is an error
                            // that is not this table's to report.
                            let name = hir[*symbol].name.text.clone();
                            if id != hir.root() && is_definition(&hir[*symbol].kind) {
                                nested.insert(name.clone());
                            }
                            let first = table.entry(name).or_insert(*symbol);
                            if announces(&hir[*first].kind) && !announces(&hir[*symbol].kind) {
                                *first = *symbol;
                            }
                        }
                        Member::Bind(bind) => {
                            for &symbol in &hir[*bind].instances {
                                bound.insert(SymbolRef { file, symbol }, *bind);
                            }
                            held.push((file, *bind));
                        }
                        _ => {}
                    }
                }
                scopes.push(table);
            }
            tables.push(scopes);
            hidden.push(hidden_scopes(hir));

            for member in &hir[hir.root()].members {
                if let Member::Opaque(body) = member {
                    opaque.extend(body.names.iter().map(|name| name.text.clone()));
                }
                let Member::Declare(symbol) = member else {
                    continue;
                };
                let SymbolKind::Definition {
                    keyword, prototype, ..
                } = hir[*symbol].kind
                else {
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
                    // A body stands in for an `extern` declaration of it,
                    // wherever each is, and externs agree or are wrong.
                    _ => {
                        let earlier = definitions.get(&name).map(|it: &SymbolRef| {
                            announces(&files[it.file.index()][it.symbol].kind)
                        });
                        match earlier {
                            Some(false) if prototype => {}
                            Some(false) => {
                                definitions.insert(name.clone(), at);
                                twice.insert(name);
                            }
                            _ => {
                                definitions.insert(name, at);
                            }
                        }
                    }
                }
            }
        }
        let targets: Vec<Vec<BindTarget>> = (files.iter())
            .map(|hir| {
                (hir.binds.iter())
                    .map(|it| bind_target(&definitions, it))
                    .collect()
            })
            .collect();
        for (file, id) in held {
            let BindTarget::Definition(definition) = targets[file.index()][id.index()] else {
                continue;
            };
            let hir = &files[file.index()];
            let into = binds.entry(definition).or_default();
            for &symbol in &hir[id].instances {
                let holder = (hir.enclosing(hir[symbol].scope, ScopeKind::Definition))
                    .and_then(|it| hir[it].owner)
                    .map(|symbol| SymbolRef { file, symbol });
                let instance = SymbolRef { file, symbol };
                into.push(Bound { instance, holder });
            }
        }
        Design {
            names: files.iter().map(|_| OnceLock::new()).collect(),
            targets,
            files,
            tables,
            hidden,
            definitions,
            nested,
            bound,
            packages,
            ambiguous,
            twice,
            opaque,
            binds,
        }
    }

    /// What `bind` of `file` binds into.
    pub(crate) fn target(&self, file: FileId, bind: BindId) -> BindTarget {
        self.targets[file.index()][bind.index()]
    }

    /// The instances bound into every instance of `definition`.
    pub(crate) fn binds(&self, definition: SymbolRef) -> &[Bound] {
        self.binds.get(&definition).map_or(&[], Vec::as_slice)
    }

    /// The definitions `tops` reach: those they instantiate, in any
    /// generate arm whatever its condition, and those the `bind`s that apply
    /// add, one at a file's top level into a definition reached, or one a
    /// definition reached holds.
    pub(crate) fn reached(&self, tops: &[impl AsRef<str>]) -> FxHashSet<SymbolRef> {
        // The instances each definition holds and binds.
        let mut uses: FxHashMap<SymbolRef, Vec<SymbolRef>> = FxHashMap::default();
        for (file, hir) in self.files() {
            for (id, scope) in hir.scopes() {
                let definition = hir.enclosing(id, ScopeKind::Definition);
                let Some(owner) = definition.and_then(|it| hir[it].owner) else {
                    continue;
                };
                for member in &scope.members {
                    let symbols = match member {
                        Member::Declare(symbol) => std::slice::from_ref(symbol),
                        Member::Bind(bind) => &hir[*bind].instances,
                        _ => &[],
                    };
                    for &symbol in symbols {
                        if matches!(hir[symbol].kind, SymbolKind::Instance(_)) {
                            let at = SymbolRef {
                                file,
                                symbol: owner,
                            };
                            uses.entry(at).or_default().push(SymbolRef { file, symbol });
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
                let instances = uses.get(&at).map_or(&[][..], Vec::as_slice);
                for definition in instances.iter().filter_map(|&it| self.instantiated(it)) {
                    if reached.insert(definition) {
                        open.push(definition);
                    }
                }
            }
            for (target, bound) in &self.binds {
                if !reached.contains(target) {
                    continue;
                }
                for bound in bound.iter().filter(|it| it.holder.is_none()) {
                    if let Some(definition) = self.instantiated(bound.instance)
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

    /// The module, interface or program called `name`.
    pub fn definition(&self, name: &str) -> Option<SymbolRef> {
        self.definitions.get(name).copied()
    }

    /// The definition the instance `at` is of: a module, interface, program
    /// or checker declared in a definition around it, or a checker imported
    /// there, else the one a file declares at its top level. A `bind`'s
    /// instance is looked for where it is bound: in the definition it names,
    /// or where no definition is so named, around the `bind`.
    pub fn instantiated(&self, at: SymbolRef) -> Option<SymbolRef> {
        let SymbolKind::Instance(instance) = &self.symbol(at).kind else {
            return None;
        };
        let name = &*instance.definition.text;
        if !self.nested.contains(name) {
            return self.definition(name);
        }
        let into = self.bound.get(&at).map(|&bind| self.target(at.file, bind));
        let body = match into {
            Some(
                BindTarget::Definition(it)
                | BindTarget::Instances {
                    definition: Some(it),
                },
            ) => match self.symbol(it).kind {
                SymbolKind::Definition { scope, .. } => Some((it.file, scope)),
                _ => None,
            },
            _ => None,
        };
        let (file, scope) = body.unwrap_or((at.file, self.symbol(at).scope));
        let filter = Filter {
            definition: true,
            ..Filter::default()
        };
        match self.lookup(file, scope, name, filter) {
            Resolution::Declared(found) => Some(found),
            _ => None,
        }
    }

    /// What `name`, used in `scope` of `file`, refers to, as `filter` has
    /// it: what the scope declares, then what it imports by name and by
    /// wildcard, then in a class what it inherits, then the same in each
    /// scope around it, up to the file's top level, and past the file a
    /// module, interface or program of the design. Where nothing declares
    /// it but what sema cannot see may, it is unknown: an import of a
    /// package no file declares, a class with a base not known, a region
    /// not lowered, a name of `std`, or one a file's opaque top level
    /// spells.
    pub(crate) fn lookup(
        &self,
        file: FileId,
        scope: ScopeId,
        name: &str,
        filter: Filter,
    ) -> Resolution {
        self.lookup_at(file, scope, name, filter, 0)
    }

    /// [`Design::lookup`], `depth` classes and typedefs into finding a
    /// class's base.
    fn lookup_at(
        &self,
        file: FileId,
        scope: ScopeId,
        name: &str,
        filter: Filter,
        depth: usize,
    ) -> Resolution {
        let hir = &self[file];
        let admits = |at: SymbolRef| !filter.definition || is_definition(&self.symbol(at).kind);
        let mut unknown = false;
        let mut at = Some(scope);
        while let Some(scope) = at {
            if let Some(object) = filter.object
                && object.call == scope
                && (object.only).is_none_or(|only| only.iter().any(|it| &*it.text == name))
            {
                match object.class.map(|class| self.class_member(class, name)) {
                    Some(Resolution::Undeclared(_)) if !CLASS_METHODS.contains(&name) => {}
                    Some(found @ Resolution::Declared(_)) => return found,
                    _ => unknown = true,
                }
            }
            if let Some(symbol) = self.declared(file, scope, name)
                && !(filter.definition && scope == hir.root())
                && (filter.declared).is_none_or(|declared| declared(scope, symbol))
                && admits(SymbolRef { file, symbol })
            {
                return Resolution::Declared(SymbolRef { file, symbol });
            }
            match self.imported(file, scope, name, 0) {
                found @ Resolution::Declared(at) if admits(at) => return found,
                Resolution::Unknown => unknown = true,
                _ => {}
            }
            if hir[scope].kind == ScopeKind::Class
                && let Some(owner) = hir[scope].owner
            {
                let class = SymbolRef {
                    file,
                    symbol: owner,
                };
                let found = match depth < BASE_DEPTH {
                    true => self.inherited_at(class, name, depth + 1),
                    false => Resolution::Unknown,
                };
                match found {
                    found @ Resolution::Declared(at) if admits(at) => return found,
                    Resolution::Unknown => unknown = true,
                    _ => {}
                }
            }
            unknown |= self.is_opaque(file, scope);
            at = hir[scope].parent;
        }
        if let Some(definition) = self.definition(name) {
            return Resolution::Declared(definition);
        }
        match unknown || STD.contains(&name) || self.spelled_opaque(name) {
            true => Resolution::Unknown,
            false => Resolution::Undeclared(Searched::Scopes),
        }
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
                Resolution::Undeclared(_) => Resolution::Unknown,
                found => found,
            };
        }
        let mut unknown = false;
        for import in imports().filter(|import| import.item.is_none()) {
            match self.member_of(&import.package.text, name, depth) {
                Resolution::Undeclared(_) => {}
                Resolution::Unknown => unknown = true,
                found => return found,
            }
        }
        match unknown {
            true => Resolution::Unknown,
            false => Resolution::Undeclared(Searched::Scopes),
        }
    }

    /// What `package::name` refers to: what the package declares, or
    /// exports. The built-in `std`, which no file declares, has the members
    /// the standard gives it, none of them modelled.
    pub(crate) fn member_of(&self, package: &str, name: &str, depth: usize) -> Resolution {
        let Some(at) = self.package(package) else {
            return match package {
                "std" if !STD.contains(&name) => Resolution::Undeclared(Searched::Package),
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
                        Resolution::Undeclared(_) => {}
                        Resolution::Unknown => unknown = true,
                        found => return found,
                    }
                }
                _ => {}
            }
        }
        match unknown {
            true => Resolution::Unknown,
            false => Resolution::Undeclared(Searched::Package),
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
        self.inherited_at(class, name, 0)
    }

    /// [`Design::inherited`], `depth` classes and typedefs into finding
    /// another's base.
    fn inherited_at(&self, class: SymbolRef, name: &str, depth: usize) -> Resolution {
        let searched = Searched::Class(class);
        let mut class = class;
        for _ in 0..BASE_DEPTH {
            let base = match self.base(class, depth) {
                Base::None => return Resolution::Undeclared(searched),
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
    /// declared, `depth` classes and typedefs in.
    fn base(&self, class: SymbolRef, depth: usize) -> Base {
        let SymbolKind::Class(Class {
            extends: Some((base, _)),
            ..
        }) = &self.symbol(class).kind
        else {
            return Base::None;
        };
        match base.kind {
            TypeKind::Named(name) => {
                self.class_named(class.file, self.symbol(class).scope, name, depth, &mut 0)
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
                match self.class_named(at.file, self.symbol(at).scope, *name, 0, &mut 0) {
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
        match self.class_named(
            handle.file,
            self.symbol(handle).scope,
            name,
            0,
            &mut unpacked,
        ) {
            Base::Class(class) if selects == Some(unpacked) => Some(class),
            _ => None,
        }
    }

    /// The class `expr`, a type's name in `file`, names from `scope`;
    /// `unpacked` counts the unpacked dimensions of the typedefs it goes
    /// through. `depth` counts those so far, and the classes whose base was
    /// looked for to get here, which a class's base named by what an outer
    /// class inherits takes.
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
            ExprKind::Name(name) => self.lookup_at(file, scope, name, Filter::default(), depth),
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
                self.class_named(at.file, self.symbol(at).scope, *name, depth + 1, unpacked)
            }
            _ => Base::Unknown,
        }
    }

    /// Whether `scope` of `file` holds a region not lowered, which may
    /// declare anything.
    pub(crate) fn is_opaque(&self, file: FileId, scope: ScopeId) -> bool {
        self.hidden[file.index()][scope.index()]
    }

    /// The symbol `name` declares directly in `scope` of `file`.
    pub(crate) fn declared(&self, file: FileId, scope: ScopeId, name: &str) -> Option<SymbolId> {
        self.tables[file.index()][scope.index()].get(name).copied()
    }

    /// The name `expr`, written in `file`, ends with as a scope of the
    /// hierarchy, once the selects after it and a modport of what they
    /// select are taken off: `bus` of `bus[1].mp`, `u.bus` of `u.bus[0]`.
    /// With it, the selects taken off, the one right after the name first.
    pub(crate) fn head(&self, file: FileId, expr: ExprId) -> (ExprId, Vec<ExprId>) {
        let hir = &self[file];
        let modport = |expr: ExprId| match self.names(file).expr(expr) {
            Some(Resolution::Declared(at)) => {
                matches!(self.symbol(at).kind, SymbolKind::Modport(_))
            }
            _ => false,
        };
        let mut head = expr;
        let mut selects = Vec::new();
        loop {
            head = match hir[head].kind {
                ExprKind::Select { base, .. } => {
                    selects.push(head);
                    base
                }
                ExprKind::Member { base, .. } if modport(head) => base,
                _ => break,
            };
        }
        selects.reverse();
        (head, selects)
    }
}

/// For each scope of `hir`, whether it holds a region not lowered: among its
/// members, or the statements of a block or a subroutine's body.
fn hidden_scopes(hir: &Hir) -> Vec<bool> {
    let mut hidden: Vec<bool> = (hir.scopes())
        .map(|(_, scope)| (scope.members.iter()).any(|it| matches!(it, Member::Opaque(_))))
        .collect();
    let is_opaque = |stmt: &StmtId| matches!(hir[*stmt].kind, StmtKind::Opaque(_));
    for (_, stmt) in hir.stmts() {
        if let StmtKind::Block { scope, stmts, .. } = &stmt.kind {
            hidden[scope.index()] |= stmts.iter().any(is_opaque);
        }
    }
    for (_, symbol) in hir.symbols() {
        if let SymbolKind::Subroutine { scope, body, .. } = &symbol.kind {
            hidden[scope.index()] |= body.iter().any(is_opaque);
        }
    }
    hidden
}

/// What `bind` binds into, given the design's `definitions`.
fn bind_target(definitions: &FxHashMap<Box<str>, SymbolRef>, bind: &Bind) -> BindTarget {
    let Some(target) = &bind.target else {
        return BindTarget::Instances { definition: None };
    };
    match (definitions.get(&target.text), bind.chosen) {
        (Some(&definition), false) => BindTarget::Definition(definition),
        (Some(&definition), true) => BindTarget::Instances {
            definition: Some(definition),
        },
        (None, false) => BindTarget::Instances { definition: None },
        (None, true) => BindTarget::Undeclared,
    }
}

/// Whether a declaration of `kind` announces another that stands in for it:
/// a forward typedef, or an `extern` definition.
fn announces(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Forward
            | SymbolKind::Definition {
                prototype: true,
                ..
            }
    )
}

/// Whether a declaration of `kind` is one an instance can be of.
fn is_definition(kind: &SymbolKind) -> bool {
    matches!(kind, SymbolKind::Definition { keyword, .. } if *keyword != PACKAGE_KW)
}

impl Index<FileId> for Design {
    type Output = Hir;

    fn index(&self, file: FileId) -> &Hir {
        &self.files[file.index()]
    }
}
