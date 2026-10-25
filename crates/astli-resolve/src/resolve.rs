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
//! A package the root patches is read from the directory the patch names,
//! which meets every requirement of it, since a directory has no version to
//! meet one with. Each run says so, as it does a pin's replaced
//! requirements.
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

use astli_package::mark;
use astli_package::{
    At, BENDER_MANIFEST, MANIFEST, Manifest, Package, Requirement, Source, manifest,
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
    /// meet, and each patch, said as a warning, [marked](astli_package::mark).
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
    /// The packages it depends on directly, by name.
    pub needs: Vec<String>,
    /// The `rev`s requirements name its commit by, other than the commit's
    /// own id.
    pub revs: Vec<String>,
}

/// Where a resolved package's files are.
#[derive(Debug, Clone)]
pub struct Located {
    /// The directory it is read from: a checkout, a directory in one, or a
    /// directory on disk.
    pub dir: PathBuf,
    /// For a checkout, every file it holds, absolute: anything else in it
    /// was put there by hand.
    pub held: Option<BTreeSet<PathBuf>>,
}

impl Resolved {
    /// Where its files are, checked out into `store` first if it comes from
    /// a repository. Resolution checks nothing out, so that a run writes
    /// only the checkouts it reads.
    pub fn checkout(&self, store: &Store) -> Result<Located, Error> {
        let (url, subdir, commit) = match (&self.origin, self.commit) {
            (Origin::Dir(dir) | Origin::Patched(dir), _) => {
                return Ok(Located {
                    dir: dir.clone(),
                    held: None,
                });
            }
            (Origin::Git { url }, Some(commit)) => (url, None, commit),
            (Origin::Within { url, subdir }, Some(commit)) => (url, Some(subdir), commit),
            (_, None) => unreachable!("a repository's package is at a commit"),
        };
        let named = Some((self.name.as_str(), self.version.as_ref()));
        let checkout = store.mirror(&self.name, url)?.checkout_as(commit, named)?;
        let dir = match subdir {
            Some(subdir) => checkout.dir.join(subdir),
            None => checkout.dir,
        };
        Ok(Located {
            dir,
            held: Some(checkout.files),
        })
    }
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
    /// A directory on disk, absolute, that the root patches the package to,
    /// whatever its requirements say.
    Patched(PathBuf),
}

impl Origin {
    /// Whether two requirements name one source, URLs compared as
    /// [`canonical`] makes them.
    fn same(&self, other: &Origin) -> bool {
        match (self, other) {
            (Origin::Git { url: a }, Origin::Git { url: b }) => canonical(a) == canonical(b),
            (Origin::Dir(a), Origin::Dir(b)) | (Origin::Patched(a), Origin::Patched(b)) => a == b,
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
            Origin::Patched(dir) => write!(f, "{}, patched", dir.display()),
            Origin::Within { url, subdir } => write!(f, "{} in {url}", subdir.display()),
        }
    }
}

/// Resolves what `root` depends on, directly or not, over the mirrors in
/// `store`, preferring what `lock` holds. The root's requirement of each
/// package in `pinned` is the only one: what anything else asks of it is
/// replaced. A package the root patches is read from the patch's directory
/// instead, whatever is asked of it.
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
            "{} names {}, which the root does not depend on",
            mark::code("[resolve] pinned"),
            mark::name(name)
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

/// What PubGrub chooses a version of: a package, or one of its features.
///
/// A feature is a package of its own at the same versions, which depends
/// on its package at exactly its own version and on what the feature brings
/// in. So choosing a version of a package chooses its features too, and a
/// dependency a feature brings in is resolved only where that feature is
/// enabled. `default` stands for the package's default features.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Node {
    Root,
    Package(String),
    Feature(String, String),
}

/// The feature that stands for a package's default features.
const DEFAULT: &str = "default";

/// What the root is called as an asker.
const ROOT: &str = "the root package";

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Node::Root => write!(f, "{ROOT}"),
            Node::Package(name) => write!(f, "{}", mark::name(name)),
            Node::Feature(name, feature) if feature == DEFAULT => {
                write!(f, "{}'s default features", mark::name(name))
            }
            Node::Feature(name, feature) => {
                write!(f, "{} with {}", mark::name(name), mark::name(feature))
            }
        }
    }
}

/// What a node depends on, each node once with every range asked of it
/// intersected.
type Constraints = HashMap<Node, Candidates>;

/// Adds `range` to what `constraints` asks of `node`.
fn constrain(constraints: &mut Constraints, node: Node, range: Candidates) {
    let range = match constraints.remove(&node) {
        Some(asked) => asked.intersection(&range),
        None => range,
    };
    constraints.insert(node, range);
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
    /// Each package's manifest at each candidate, and where its paths are
    /// relative to, read once.
    manifests: HashMap<(String, Candidate), (Manifest, Base)>,
    /// The candidate last chosen for each package, which its features take
    /// too.
    decided: HashMap<String, Candidate>,
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
                    true => format!(
                        "\nTo let the root's requirement of a package it depends on decide, \
                         name the package in {}.",
                        mark::code("[resolve] pinned")
                    ),
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
                Node::Root | Node::Feature(..) => None,
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
        let mut warnings = self.replaced(state, &chosen)?;
        for (name, dir) in &self.root.patches.0 {
            warnings.push(match chosen.contains_key(name) {
                true => format!(
                    "{} is patched: read from {}, whatever is required of it",
                    mark::name(name),
                    mark::path(dir.display())
                ),
                false => format!(
                    "{} is patched, but the design does not depend on it",
                    mark::name(name)
                ),
            });
        }
        // A dependency no enabled feature brought in is not a need.
        let chosen_names: BTreeSet<String> = chosen.keys().cloned().collect();
        let mut packages = Vec::new();
        for (name, candidate) in chosen {
            let origin = state.origins[&name].0.clone();
            let (commit, version) = match &candidate {
                Candidate::Commit { version, id } => (Some(*id), version.clone()),
                Candidate::Local => (None, None),
            };
            let needs = (state
                .needs
                .remove(&(name.clone(), candidate))
                .unwrap_or_default())
            .into_iter()
            .filter(|it| chosen_names.contains(it))
            .collect();
            let revs = (state.revs.remove(&name).unwrap_or_default().into_iter())
                .filter(|(_, id)| Some(*id) == commit)
                .map(|(rev, _)| rev)
                .collect();
            packages.push(Resolved {
                name,
                origin,
                commit,
                version,
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

    /// The mirror of `url`, fetched once in this run, for `name`, unless it was cloned
    /// just now or holds every commit the lock names of it.
    fn mirror<'s>(&self, state: &'s mut State, name: &str, url: &str) -> Result<&'s Mirror, Error> {
        let key = key(url)?;
        if !state.mirrors.contains_key(&key) {
            let mut mirror = self.store.mirror(name, url)?;
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
    fn mirror_mut<'s>(
        &self,
        state: &'s mut State,
        name: &str,
        url: &str,
    ) -> Result<&'s mut Mirror, Error> {
        self.mirror(state, name, url)?;
        Ok(state.mirrors.get_mut(&key(url)?).expect("opened just now"))
    }

    /// The tagged commits of `url`, each with every version its tags give.
    fn tagged<'s>(
        &self,
        state: &'s mut State,
        name: &str,
        url: &str,
    ) -> Result<&'s [(Candidate, Vec<Version>)], Error> {
        let key = key(url)?;
        if !state.tagged.contains_key(&key) {
            let mut by_commit: BTreeMap<ObjectId, Vec<Version>> = BTreeMap::new();
            for (version, id) in self.mirror(state, name, url)?.versions()? {
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
    fn at(
        &self,
        state: &mut State,
        name: &str,
        url: &str,
        id: ObjectId,
    ) -> Result<Candidate, Error> {
        let tagged = self.tagged(state, name, url)?;
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
                    (self.tagged(state, name, url)?.iter())
                        .filter(|(_, versions)| versions.iter().any(|it| required.matches(it)))
                        .map(|(candidate, _)| candidate.clone()),
                ),
                At::Rev(rev) => {
                    // A branch moves on upstream, but the commit the lock
                    // holds for it stays chosen.
                    let locked = (self.locked(name, url))
                        .filter(|it| it.revs.contains(rev))
                        .map(|it| it.commit);
                    let mirror = self.mirror(state, name, url)?;
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
                        self.mirror_mut(state, name, url)?.fetch_commit(id)?;
                        found = self.mirror(state, name, url)?.rev(rev);
                    }
                    let Some(id) = found else {
                        return Err(Error::Resolve(format!(
                            "{asker} depends on {} at {}, which {} has no branch, tag or \
                             commit named",
                            mark::name(name),
                            mark::version(rev),
                            mark::path(url)
                        )));
                    };
                    if ObjectId::from_hex(rev.as_bytes()).is_err() {
                        (state.revs.entry(name.to_string()).or_default()).insert((rev.clone(), id));
                    }
                    Candidates::of([self.at(state, name, url, id)?])
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
                let named = mark::name(name);
                return Err(Error::Resolve(format!(
                    "{asker} depends on {named} from {}, but {by} from {}: two packages of one \
                     name, which one design cannot hold. If the root depends on {named}, \
                     naming it in {} lets the root's source decide",
                    mark::path(&origin),
                    mark::path(first),
                    mark::code("[resolve] pinned")
                )));
            }
            return Ok(());
        }
        state
            .origins
            .insert(name.to_string(), (origin, asker.to_string()));
        Ok(())
    }

    /// The manifest of the package `name` at `candidate`, and where its
    /// paths are relative to.
    fn manifest(
        &self,
        state: &mut State,
        name: &str,
        candidate: &Candidate,
    ) -> Result<(Manifest, Base), Error> {
        let at = (name.to_string(), candidate.clone());
        if let Some(read) = state.manifests.get(&at) {
            return Ok(read.clone());
        }
        let origin = state.origins[name].0.clone();
        let (url, subdir) = match (&origin, candidate) {
            (Origin::Dir(dir) | Origin::Patched(dir), _) => {
                let read = (Package::at(dir)?.manifest(), Base::Disk);
                state.manifests.insert(at, read.clone());
                return Ok(read);
            }
            (Origin::Git { url }, _) => (url.clone(), PathBuf::new()),
            (Origin::Within { url, subdir }, _) => (url.clone(), subdir.clone()),
        };
        let Candidate::Commit { id, .. } = candidate else {
            unreachable!("a repository's package is at a commit");
        };
        let mirror = self.mirror(state, name, &url)?;
        let read = |file: &str| -> Result<Option<String>, Error> {
            let path = subdir.join(file);
            let path = path.to_string_lossy().replace('\\', "/");
            match mirror.file(*id, &path)? {
                None => Ok(None),
                Some(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| {
                    Error::Resolve(format!(
                        "{} at {} of {} is not UTF-8",
                        mark::path(&path),
                        mark::version(id),
                        mark::path(&url)
                    ))
                }),
            }
        };
        let (astli, bender) = (read(MANIFEST)?, read(BENDER_MANIFEST)?);
        let manifest = manifest(astli.as_deref(), bender.as_deref())
            .map_err(|err| Error::Resolve(format!("{} at {candidate}: {err}", mark::name(name))))?;
        let base = Base::Repo {
            url,
            commit: *id,
            subdir,
        };
        state.manifests.insert(at, (manifest.clone(), base.clone()));
        Ok((manifest, base))
    }

    /// Adds to `constraints` what `asker` requiring `name` as `requirement`
    /// asks: the package, and the features the requirement enables there,
    /// `also` among them. What a pin replaces asks only for the features,
    /// at whatever version the root's requirement chooses, and what a patch
    /// meets asks for them in its directory.
    #[allow(clippy::too_many_arguments)]
    fn require(
        &self,
        state: &mut State,
        constraints: &mut Constraints,
        asker: &str,
        base: &Base,
        name: &str,
        requirement: &Requirement,
        also: Option<&str>,
    ) -> Result<(), Error> {
        let range = if let Some(dir) = self.root.patches.0.get(name) {
            // Met whatever it asks, so it is not read.
            self.require_from(state, asker, name, Origin::Patched(dir.clone()))?;
            let local = Candidates::of([Candidate::Local]);
            constrain(constraints, Node::Package(name.to_string()), local.clone());
            local
        } else if asker != ROOT && self.pinned.contains(name) {
            state.replaced.push(Replaced {
                asker: asker.to_string(),
                name: name.to_string(),
                requirement: requirement.clone(),
                base: base.clone(),
            });
            Candidates::full()
        } else {
            let (origin, admitted) = self.admitted(state, asker, name, requirement, base)?;
            self.require_from(state, asker, name, origin)?;
            constrain(
                constraints,
                Node::Package(name.to_string()),
                admitted.clone(),
            );
            admitted
        };
        let features = (requirement.features.iter().map(String::as_str))
            .chain(requirement.default_features.then_some(DEFAULT))
            .chain(also);
        for feature in features {
            let node = Node::Feature(name.to_string(), feature.to_string());
            constrain(constraints, node, range.clone());
        }
        Ok(())
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
                    "{asker} depends on {} {}; the root pins it to {candidate} of {}",
                    mark::name(&name),
                    described(&requirement),
                    mark::path(&origin)
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
            "{asker} depends on {} without saying where it is: give it {} or {}",
            mark::name(name),
            mark::code("git"),
            mark::code("path")
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
                    "{asker} depends on {} at {}, outside its repository",
                    mark::name(name),
                    mark::path(path.display())
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
        }) => format!("{} of {}", mark::version(required), mark::path(url)),
        Some(Source::Git {
            url,
            at: At::Rev(rev),
        }) => format!("at {} of {}", mark::version(rev), mark::path(url)),
        Some(Source::Path(path)) => format!("at {}", mark::path(path.display())),
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
    type Priority = (bool, Reverse<usize>);
    type Err = Error;

    /// Packages before features, so that a feature takes the version its
    /// package has; then the package with the fewest candidates first, where
    /// a conflict shows soonest.
    fn prioritize(
        &self,
        node: &Node,
        range: &Candidates,
        _: &PackageResolutionStatistics,
    ) -> (bool, Reverse<usize>) {
        let package = !matches!(node, Node::Feature(..));
        (package, Reverse(range.len().unwrap_or(usize::MAX)))
    }

    fn choose_version(&self, node: &Node, range: &Candidates) -> Result<Option<Candidate>, Error> {
        let state = &mut *self.state.borrow_mut();
        let name = match node {
            Node::Root => return Ok(Some(Candidate::Local)),
            Node::Package(name) => name,
            Node::Feature(name, _) => {
                if let Some(decided) = state.decided.get(name)
                    && range.contains(decided)
                {
                    return Ok(Some(decided.clone()));
                }
                name
            }
        };
        let chosen = self.chosen(state, name, range)?;
        if let (Node::Package(name), Some(chosen)) = (node, &chosen) {
            state.decided.insert(name.clone(), chosen.clone());
        }
        Ok(chosen)
    }

    fn get_dependencies(
        &self,
        node: &Node,
        candidate: &Candidate,
    ) -> Result<Dependencies<Node, Candidates, String>, Error> {
        let state = &mut *self.state.borrow_mut();
        let mut constraints = Constraints::new();
        match node {
            // Every feature of the root's is on, so that the lock holds what
            // any selection of them needs.
            Node::Root => {
                let root = self.root;
                for (name, requirement) in root.dependencies.iter().chain(&root.dev_dependencies) {
                    self.require(
                        state,
                        &mut constraints,
                        ROOT,
                        &Base::Disk,
                        name,
                        requirement,
                        None,
                    )?;
                }
                for feature in root.features.values() {
                    for (name, also) in &feature.dependencies {
                        let requirement = &root.dependencies[name];
                        let also = also.as_deref();
                        self.require(
                            state,
                            &mut constraints,
                            ROOT,
                            &Base::Disk,
                            name,
                            requirement,
                            also,
                        )?;
                    }
                }
            }
            Node::Package(name) => {
                let (manifest, base) = self.manifest(state, name, candidate)?;
                let asker = node.to_string();
                for (dependency, requirement) in &manifest.dependencies {
                    if !manifest.optional(dependency) {
                        self.require(
                            state,
                            &mut constraints,
                            &asker,
                            &base,
                            dependency,
                            requirement,
                            None,
                        )?;
                    }
                }
                let needs = manifest.dependencies.keys().cloned().collect();
                state.needs.insert((name.clone(), candidate.clone()), needs);
            }
            Node::Feature(name, feature) => {
                let (manifest, base) = self.manifest(state, name, candidate)?;
                let at = Candidates::singleton(candidate.clone());
                constrain(&mut constraints, Node::Package(name.clone()), at.clone());
                if feature == DEFAULT {
                    for feature in &manifest.default_features {
                        let node = Node::Feature(name.clone(), feature.clone());
                        constrain(&mut constraints, node, at.clone());
                    }
                } else {
                    let Some(brought) = manifest.features.get(feature) else {
                        return Ok(Dependencies::Unavailable(format!(
                            "it declares no feature {}",
                            mark::name(feature)
                        )));
                    };
                    let asker = mark::name(name);
                    for (dependency, also) in brought {
                        let requirement = &manifest.dependencies[dependency];
                        let also = also.as_deref();
                        self.require(
                            state,
                            &mut constraints,
                            &asker,
                            &base,
                            dependency,
                            requirement,
                            also,
                        )?;
                    }
                }
            }
        }
        Ok(Dependencies::Available(DependencyConstraints::from_iter(
            constraints,
        )))
    }
}

impl Provider<'_> {
    /// The candidate to try for the package `name` among `range`: the
    /// locked commit while the range holds it, or else the highest.
    fn chosen(
        &self,
        state: &mut State,
        name: &str,
        range: &Candidates,
    ) -> Result<Option<Candidate>, Error> {
        let origin = state.origins[name].0.clone();
        Ok(match origin {
            Origin::Dir(_) | Origin::Patched(_) => range
                .contains(&Candidate::Local)
                .then_some(Candidate::Local),
            Origin::Within { .. } => range.highest([]).cloned(),
            Origin::Git { url } => {
                if let Some(locked) = self.locked(name, &url) {
                    let candidate = self.at(state, name, &url, locked.commit)?;
                    if range.contains(&candidate) {
                        return Ok(Some(candidate));
                    }
                }
                let tagged = self.tagged(state, name, &url)?;
                range
                    .highest(tagged.iter().map(|(candidate, _)| candidate))
                    .cloned()
            }
        })
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
