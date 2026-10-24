//! Choosing one version of each package a design needs.
//!
//! PubGrub chooses, over the candidates [`crate::version`] describes: the
//! tagged commits of each repository, and the commits `rev`s name. A
//! package is required from one source by everything that requires it; two
//! sources of one name are two packages, which one design cannot hold.
//!
//! Requirements are strict unless the root pins a package: then its own
//! requirement of it is the only one, what anything else asks is replaced,
//! and each replaced requirement the choice does not meet is reported.
//!
//! A [`Lock`] says what the last resolution chose. Its commit for a package
//! is chosen while every requirement still admits it, and a repository
//! whose locked commits are all in its mirror is not fetched. If the lock
//! no longer holds, resolution runs again with every mirror fetched, since
//! what it needs instead may be newer than the mirror.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::path::{Component, Path, PathBuf};

use astli_package::{
    At, BENDER_MANIFEST, MANIFEST, Package, Requirement, Source, manifest_dependencies,
};
use gix::ObjectId;
use pubgrub::{
    DefaultStringReporter, Dependencies, DependencyConstraints, DependencyProvider,
    PackageResolutionStatistics, PubGrubError, Reporter, VersionSet,
};
use semver::Version;

use crate::store::key;
use crate::version::{Candidate, Candidates};
use crate::{Error, Lock, Locked, Mirror, Store, canonical};

/// One version of each package a design needs, and what the choice
/// overrode.
#[derive(Debug)]
pub struct Resolution {
    /// By name.
    pub packages: Vec<Resolved>,
    /// Each requirement a pin replaced that the chosen version does not
    /// meet, said as a warning.
    pub warnings: Vec<String>,
}

/// A package the design holds, at the version chosen.
#[derive(Debug)]
pub struct Resolved {
    pub name: String,
    pub origin: Origin,
    /// The commit chosen, for a package from a repository.
    pub commit: Option<ObjectId>,
    /// The highest version a tag on that commit gives it, if any.
    pub version: Option<Version>,
    /// Where its files are: a checkout, a directory in one, or a directory
    /// on disk.
    pub dir: PathBuf,
    /// The packages it depends on directly, by name.
    pub needs: Vec<String>,
    /// The `rev`s requirements name its commit by, other than the commit's
    /// own id.
    pub revs: Vec<String>,
}

/// Where a package comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A repository, at the URL first written for it.
    Git { url: String },
    /// A directory on disk, absolute.
    Dir(PathBuf),
    /// A directory of a repository, at the commit of the package whose
    /// manifest names it by `path`: the same repository, pinned by that
    /// commit.
    Within { url: String, subdir: PathBuf },
}

impl Origin {
    /// Whether two requirements name one source, URLs compared as
    /// [`canonical`] makes them.
    fn same(&self, other: &Origin) -> bool {
        match (self, other) {
            (Origin::Git { url: a }, Origin::Git { url: b }) => canonical(a) == canonical(b),
            (Origin::Dir(a), Origin::Dir(b)) => a == b,
            (Origin::Within { url: a, subdir: x }, Origin::Within { url: b, subdir: y }) => {
                canonical(a) == canonical(b) && x == y
            }
            _ => false,
        }
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Origin::Git { url } => write!(f, "{url}"),
            Origin::Dir(dir) => write!(f, "{}", dir.display()),
            Origin::Within { url, subdir } => write!(f, "{} in {url}", subdir.display()),
        }
    }
}

/// Resolves what `root` depends on, directly or not, over the mirrors in
/// `store`, preferring what `lock` holds. The root's requirement of each
/// package in `pinned` is the only one: what anything else asks of it is
/// replaced.
pub fn resolve(
    store: &Store,
    root: &Package,
    pinned: &BTreeSet<String>,
    lock: &Lock,
) -> Result<Resolution, Error> {
    if let Some(name) = pinned
        .iter()
        .find(|it| !root.dependencies.contains_key(*it))
    {
        return Err(Error::Resolve(format!(
            "`[resolve] pinned` names `{name}`, which the root does not depend on"
        )));
    }
    let provider = |fetch_all| Provider {
        store,
        root,
        pinned,
        lock,
        fetch_all,
        state: RefCell::new(State::default()),
    };
    let trusting = provider(false);
    let solved = trusting.solve();
    let state = trusting.state.borrow();
    let stale = !state.trusted.is_empty()
        && (state.missed
            || match &solved {
                Ok(chosen) => trusting.moved(chosen),
                Err(_) => true,
            });
    drop(state);
    if !stale {
        return trusting.finish(solved?);
    }
    // Dropped first, since it holds the mirrors' locks.
    drop(trusting);
    let fetching = provider(true);
    let solved = fetching.solve()?;
    fetching.finish(solved)
}

/// A package as PubGrub sees it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Node {
    Root,
    Package(String),
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Node::Root => write!(f, "the root package"),
            Node::Package(name) => write!(f, "`{name}`"),
        }
    }
}

/// Where the paths of a manifest are relative to.
#[derive(Clone)]
enum Base {
    /// Made absolute already, as a [`Package`] read from disk has them.
    Disk,
    /// A directory of a repository at a commit.
    Repo {
        url: String,
        commit: ObjectId,
        subdir: PathBuf,
    },
}

/// A requirement a pin replaced: who asked, of what, how, and from where
/// its paths are read.
struct Replaced {
    asker: String,
    name: String,
    requirement: Requirement,
    base: Base,
}

struct Provider<'a> {
    store: &'a Store,
    root: &'a Package,
    pinned: &'a BTreeSet<String>,
    lock: &'a Lock,
    /// Whether a mirror is fetched even when it holds every commit the lock
    /// names of it.
    fetch_all: bool,
    state: RefCell<State>,
}

#[derive(Default)]
struct State {
    /// Each repository's mirror, opened and fetched once, by its store key.
    mirrors: HashMap<String, Mirror>,
    /// Each repository's tagged commits, and every version each tag gives
    /// it, by store key.
    tagged: HashMap<String, Vec<(Candidate, Vec<Version>)>>,
    /// Where each package comes from, as first required, and who required
    /// it so.
    origins: HashMap<String, (Origin, String)>,
    /// What each package at each candidate depends on directly.
    needs: HashMap<(String, Candidate), Vec<String>>,
    replaced: Vec<Replaced>,
    /// Each `rev` other than a commit's id that a requirement of a package
    /// named, and the commit it names, by package.
    revs: HashMap<String, BTreeSet<(String, ObjectId)>>,
    /// The mirrors not fetched, trusting the lock, by store key.
    trusted: HashSet<String>,
    /// Whether a `rev` the lock does not hold was read from such a mirror,
    /// which may know an older commit for it than upstream.
    missed: bool,
}

impl Provider<'_> {
    /// One candidate for each package, as PubGrub chooses them.
    fn solve(&self) -> Result<BTreeMap<String, Candidate>, Error> {
        let chosen = match pubgrub::resolve(self, Node::Root, Candidate::Local) {
            Ok(chosen) => chosen,
            Err(PubGrubError::NoSolution(mut tree)) => {
                tree.collapse_no_versions();
                let hint = match self.pinned.is_empty() {
                    true => "\nTo let the root's requirement of a package it depends on decide, \
                             name the package in `[resolve] pinned`."
                        .to_string(),
                    false => String::new(),
                };
                // What is on disk shows as no version, which leaves two
                // spaces where the report puts one around it.
                let report = DefaultStringReporter::report(&tree).replace("  ", " ");
                return Err(Error::Resolve(format!(
                    "no version of each package meets every requirement:\n{report}{hint}"
                )));
            }
            Err(PubGrubError::ErrorRetrievingDependencies { source, .. })
            | Err(PubGrubError::ErrorChoosingVersion { source, .. })
            | Err(PubGrubError::ErrorInShouldCancel(source)) => return Err(source),
        };
        Ok((chosen.into_iter())
            .filter_map(|(node, candidate)| match node {
                Node::Root => None,
                Node::Package(name) => Some((name, candidate)),
            })
            .collect())
    }

    /// Whether `chosen` moves a package off the commit the lock holds.
    fn moved(&self, chosen: &BTreeMap<String, Candidate>) -> bool {
        let state = self.state.borrow();
        chosen.iter().any(|(name, candidate)| {
            let (Origin::Git { url }, Candidate::Commit { id, .. }) =
                (&state.origins[name].0, candidate)
            else {
                return false;
            };
            self.locked(name, url).is_some_and(|it| it.commit != *id)
        })
    }

    /// The packages `chosen` holds, checked out, and what the choice
    /// overrode.
    fn finish(&self, chosen: BTreeMap<String, Candidate>) -> Result<Resolution, Error> {
        let state = &mut *self.state.borrow_mut();
        let warnings = self.replaced(state, &chosen)?;
        let mut packages = Vec::new();
        for (name, candidate) in chosen {
            let origin = state.origins[&name].0.clone();
            let (commit, version) = match &candidate {
                Candidate::Commit { version, id } => (Some(*id), version.clone()),
                Candidate::Local => (None, None),
            };
            let dir = match (&origin, commit) {
                (Origin::Dir(dir), _) => dir.clone(),
                (Origin::Git { url }, Some(id)) => self.mirror(state, url)?.checkout(id)?,
                (Origin::Within { url, subdir }, Some(id)) => {
                    self.mirror(state, url)?.checkout(id)?.join(subdir)
                }
                (_, None) => unreachable!("a repository's package is at a commit"),
            };
            let needs = state
                .needs
                .remove(&(name.clone(), candidate))
                .unwrap_or_default();
            let revs = (state.revs.remove(&name).unwrap_or_default().into_iter())
                .filter(|(_, id)| Some(*id) == commit)
                .map(|(rev, _)| rev)
                .collect();
            packages.push(Resolved {
                name,
                origin,
                commit,
                version,
                dir,
                needs,
                revs,
            });
        }
        Ok(Resolution { packages, warnings })
    }

    /// What the lock holds of the package `name`, if it locked it from
    /// `url`.
    fn locked(&self, name: &str, url: &str) -> Option<&Locked> {
        (self.lock.get(name)).filter(|it| canonical(&it.source) == canonical(url))
    }

    /// The mirror of `url`, fetched once in this run unless it was cloned
    /// just now or holds every commit the lock names of it.
    fn mirror<'s>(&self, state: &'s mut State, url: &str) -> Result<&'s Mirror, Error> {
        let key = key(url)?;
        if !state.mirrors.contains_key(&key) {
            let mut mirror = self.store.mirror(url)?;
            let locked: Vec<ObjectId> = (self.lock.packages.iter())
                .filter(|it| canonical(&it.source) == canonical(url))
                .map(|it| it.commit)
                .collect();
            let trusted =
                !self.fetch_all && !locked.is_empty() && locked.iter().all(|id| mirror.has(*id));
            if trusted {
                state.trusted.insert(key.clone());
            } else if !mirror.fresh {
                mirror.fetch()?;
            }
            for id in locked {
                // A locked commit no branch or tag reaches any more is
                // asked for by its id. One the forge no longer serves is
                // not chosen, and the package resolves again.
                if !mirror.has(id) {
                    let _ = mirror.fetch_commit(id);
                }
            }
            state.mirrors.insert(key.clone(), mirror);
        }
        Ok(&state.mirrors[&key])
    }

    /// The mirror of `url`, to fetch more into.
    fn mirror_mut<'s>(&self, state: &'s mut State, url: &str) -> Result<&'s mut Mirror, Error> {
        self.mirror(state, url)?;
        Ok(state.mirrors.get_mut(&key(url)?).expect("opened just now"))
    }

    /// The tagged commits of `url`, each with every version its tags give.
    fn tagged<'s>(
        &self,
        state: &'s mut State,
        url: &str,
    ) -> Result<&'s [(Candidate, Vec<Version>)], Error> {
        let key = key(url)?;
        if !state.tagged.contains_key(&key) {
            let mut by_commit: BTreeMap<ObjectId, Vec<Version>> = BTreeMap::new();
            for (version, id) in self.mirror(state, url)?.versions()? {
                by_commit.entry(id).or_default().push(version);
            }
            let tagged = (by_commit.into_iter())
                .map(|(id, versions)| {
                    let version = versions.iter().max().cloned();
                    (Candidate::Commit { version, id }, versions)
                })
                .collect();
            state.tagged.insert(key.clone(), tagged);
        }
        Ok(&state.tagged[&key])
    }

    /// `id` of `url` as a candidate, with the version its tags give it.
    fn at(&self, state: &mut State, url: &str, id: ObjectId) -> Result<Candidate, Error> {
        let tagged = self.tagged(state, url)?;
        let found = tagged.iter().find(
            |(candidate, _)| matches!(candidate, Candidate::Commit { id: at, .. } if *at == id),
        );
        Ok(match found {
            Some((candidate, _)) => candidate.clone(),
            None => Candidate::Commit { version: None, id },
        })
    }

    /// Where `requirement` names `name`, and the candidates it admits.
    fn admitted(
        &self,
        state: &mut State,
        asker: &str,
        name: &str,
        requirement: &Requirement,
        base: &Base,
    ) -> Result<(Origin, Candidates), Error> {
        let origin = origin(asker, name, requirement, base)?;
        let admitted = match (&origin, &requirement.source) {
            (Origin::Git { url }, Some(Source::Git { at, .. })) => match at {
                At::Version(required) => Candidates::of(
                    (self.tagged(state, url)?.iter())
                        .filter(|(_, versions)| versions.iter().any(|it| required.matches(it)))
                        .map(|(candidate, _)| candidate.clone()),
                ),
                At::Rev(rev) => {
                    // A branch moves on upstream, but the commit the lock
                    // holds for it stays chosen.
                    let locked = (self.locked(name, url))
                        .filter(|it| it.revs.contains(rev))
                        .map(|it| it.commit);
                    let mirror = self.mirror(state, url)?;
                    let mut found = match locked {
                        Some(id) if mirror.has(id) => Some(id),
                        _ => mirror.rev(rev),
                    };
                    if locked.is_none()
                        && ObjectId::from_hex(rev.as_bytes()).is_err()
                        && state.trusted.contains(&key(url)?)
                    {
                        state.missed = true;
                    }
                    if found.is_none()
                        && let Ok(id) = ObjectId::from_hex(rev.as_bytes())
                    {
                        // A commit no branch or tag reaches any more, as
                        // after a force-push, is asked for by its id.
                        self.mirror_mut(state, url)?.fetch_commit(id)?;
                        found = self.mirror(state, url)?.rev(rev);
                    }
                    let Some(id) = found else {
                        return Err(Error::Resolve(format!(
                            "{asker} depends on `{name}` at `{rev}`, which {url} has no \
                             branch, tag or commit named"
                        )));
                    };
                    if ObjectId::from_hex(rev.as_bytes()).is_err() {
                        (state.revs.entry(name.to_string()).or_default()).insert((rev.clone(), id));
                    }
                    Candidates::of([self.at(state, url, id)?])
                }
            },
            (Origin::Within { .. }, _) => {
                let Base::Repo { commit, .. } = base else {
                    unreachable!("only a repository's manifest names a directory within it");
                };
                Candidates::of([Candidate::Commit {
                    version: None,
                    id: *commit,
                }])
            }
            _ => Candidates::of([Candidate::Local]),
        };
        Ok((origin, admitted))
    }

    /// Records that `asker` requires `name` from `origin`, refusing a second
    /// source for it.
    fn require_from(
        &self,
        state: &mut State,
        asker: &str,
        name: &str,
        origin: Origin,
    ) -> Result<(), Error> {
        if let Some((first, by)) = state.origins.get(name) {
            if !first.same(&origin) {
                return Err(Error::Resolve(format!(
                    "{asker} depends on `{name}` from {origin}, but {by} from {first}: two \
                     packages of one name, which one design cannot hold. If the root depends \
                     on `{name}`, naming it in `[resolve] pinned` lets the root's source decide"
                )));
            }
            return Ok(());
        }
        state
            .origins
            .insert(name.to_string(), (origin, asker.to_string()));
        Ok(())
    }

    /// What the package `name` at `candidate` requires, and where its
    /// manifest's paths are relative to.
    fn requirements(
        &self,
        state: &mut State,
        name: &str,
        candidate: &Candidate,
    ) -> Result<(BTreeMap<String, Requirement>, Base), Error> {
        let origin = state.origins[name].0.clone();
        let (url, subdir) = match (&origin, candidate) {
            (Origin::Dir(dir), _) => {
                let package = Package::at(dir)?;
                return Ok((package.dependencies, Base::Disk));
            }
            (Origin::Git { url }, _) => (url.clone(), PathBuf::new()),
            (Origin::Within { url, subdir }, _) => (url.clone(), subdir.clone()),
        };
        let Candidate::Commit { id, .. } = candidate else {
            unreachable!("a repository's package is at a commit");
        };
        let mirror = self.mirror(state, &url)?;
        let read = |file: &str| -> Result<Option<String>, Error> {
            let path = subdir.join(file);
            let path = path.to_string_lossy().replace('\\', "/");
            match mirror.file(*id, &path)? {
                None => Ok(None),
                Some(bytes) => String::from_utf8(bytes)
                    .map(Some)
                    .map_err(|_| Error::Resolve(format!("{path} at {id} of {url} is not UTF-8"))),
            }
        };
        let (astli, bender) = (read(MANIFEST)?, read(BENDER_MANIFEST)?);
        let requirements = manifest_dependencies(astli.as_deref(), bender.as_deref())
            .map_err(|err| Error::Resolve(format!("`{name}` at {candidate}: {err}")))?;
        let base = Base::Repo {
            url,
            commit: *id,
            subdir,
        };
        Ok((requirements, base))
    }

    /// The warnings for the requirements pins replaced that `chosen` does
    /// not meet.
    fn replaced(
        &self,
        state: &mut State,
        chosen: &BTreeMap<String, Candidate>,
    ) -> Result<Vec<String>, Error> {
        let mut warnings = Vec::new();
        for replaced in std::mem::take(&mut state.replaced) {
            let Replaced {
                asker,
                name,
                requirement,
                base,
            } = replaced;
            let (Some(candidate), Some((origin, _))) =
                (chosen.get(&name), state.origins.get(&name))
            else {
                continue;
            };
            let origin = origin.clone();
            // Another source is reported without fetching it, and a
            // requirement that names nothing there does not stop the run.
            let met = match self::origin(&asker, &name, &requirement, &base) {
                Ok(asked) if asked.same(&origin) => {
                    match self.admitted(state, &asker, &name, &requirement, &base) {
                        Ok((_, admitted)) => admitted.contains(candidate),
                        Err(_) => false,
                    }
                }
                _ => false,
            };
            if !met {
                warnings.push(format!(
                    "{asker} depends on `{name}` {}; the root pins it to {candidate} of {origin}",
                    described(&requirement)
                ));
            }
        }
        warnings.sort();
        warnings.dedup();
        Ok(warnings)
    }
}

/// Where `requirement` names `name`, from a manifest whose paths are
/// relative to `base`.
fn origin(
    asker: &str,
    name: &str,
    requirement: &Requirement,
    base: &Base,
) -> Result<Origin, Error> {
    match (&requirement.source, base) {
        (None, _) => Err(Error::Resolve(format!(
            "{asker} depends on `{name}` without saying where it is: give it `git` or `path`"
        ))),
        (Some(Source::Git { url, .. }), _) => Ok(Origin::Git { url: url.clone() }),
        (Some(Source::Path(dir)), Base::Disk) => Ok(Origin::Dir(dir.clone())),
        (Some(Source::Path(path)), Base::Repo { url, subdir, .. }) => {
            match within(&subdir.join(path)) {
                Some(subdir) => Ok(Origin::Within {
                    url: url.clone(),
                    subdir,
                }),
                None => Err(Error::Resolve(format!(
                    "{asker} depends on `{name}` at `{}`, outside its repository",
                    path.display()
                ))),
            }
        }
    }
}

/// What a requirement asks, said for a warning.
fn described(requirement: &Requirement) -> String {
    match &requirement.source {
        Some(Source::Git {
            url,
            at: At::Version(required),
        }) => format!("{required} of {url}"),
        Some(Source::Git {
            url,
            at: At::Rev(rev),
        }) => format!("at `{rev}` of {url}"),
        Some(Source::Path(path)) => format!("at `{}`", path.display()),
        None => "from nowhere said".to_string(),
    }
}

/// `path` as a directory inside the repository it is relative to, or `None`
/// if it leaves it.
fn within(path: &Path) -> Option<PathBuf> {
    let mut kept = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !kept.pop() {
                    return None;
                }
            }
            Component::Normal(part) => kept.push(part),
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(kept)
}

impl DependencyProvider for Provider<'_> {
    type P = Node;
    type V = Candidate;
    type VS = Candidates;
    type M = String;
    type Priority = Reverse<usize>;
    type Err = Error;

    /// The package with the fewest candidates first, where a conflict shows
    /// soonest.
    fn prioritize(
        &self,
        _: &Node,
        range: &Candidates,
        _: &PackageResolutionStatistics,
    ) -> Reverse<usize> {
        Reverse(range.len().unwrap_or(usize::MAX))
    }

    fn choose_version(&self, node: &Node, range: &Candidates) -> Result<Option<Candidate>, Error> {
        let Node::Package(name) = node else {
            return Ok(Some(Candidate::Local));
        };
        let state = &mut *self.state.borrow_mut();
        let origin = state.origins[name].0.clone();
        Ok(match origin {
            Origin::Dir(_) => range
                .contains(&Candidate::Local)
                .then_some(Candidate::Local),
            Origin::Within { .. } => range.highest([]).cloned(),
            Origin::Git { url } => {
                if let Some(locked) = self.locked(name, &url) {
                    let candidate = self.at(state, &url, locked.commit)?;
                    if range.contains(&candidate) {
                        return Ok(Some(candidate));
                    }
                }
                let tagged = self.tagged(state, &url)?;
                range
                    .highest(tagged.iter().map(|(candidate, _)| candidate))
                    .cloned()
            }
        })
    }

    fn get_dependencies(
        &self,
        node: &Node,
        candidate: &Candidate,
    ) -> Result<Dependencies<Node, Candidates, String>, Error> {
        let state = &mut *self.state.borrow_mut();
        let (asker, requirements, base) = match node {
            Node::Root => (node.to_string(), self.root.dependencies.clone(), Base::Disk),
            Node::Package(name) => {
                let (requirements, base) = self.requirements(state, name, candidate)?;
                (node.to_string(), requirements, base)
            }
        };
        let mut constraints = Vec::new();
        for (name, requirement) in &requirements {
            if *node != Node::Root && self.pinned.contains(name) {
                state.replaced.push(Replaced {
                    asker: asker.clone(),
                    name: name.clone(),
                    requirement: requirement.clone(),
                    base: base.clone(),
                });
                continue;
            }
            let (origin, admitted) = self.admitted(state, &asker, name, requirement, &base)?;
            self.require_from(state, &asker, name, origin)?;
            constraints.push((Node::Package(name.clone()), admitted));
        }
        if let Node::Package(name) = node {
            let needs = requirements.keys().cloned().collect();
            state.needs.insert((name.clone(), candidate.clone()), needs);
        }
        Ok(Dependencies::Available(DependencyConstraints::from_iter(
            constraints,
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::within;

    #[test]
    fn a_path_stays_inside_its_repository() {
        assert_eq!(within(Path::new("hw/../pd")), Some(PathBuf::from("pd")));
        assert_eq!(
            within(Path::new("./ip/core")),
            Some(PathBuf::from("ip/core"))
        );
        assert_eq!(within(Path::new("../other")), None);
        assert_eq!(within(Path::new("/abs")), None);
    }
}
