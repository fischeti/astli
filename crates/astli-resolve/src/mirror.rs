//! A bare mirror of one repository: what resolution reads of it, and the
//! checkouts made from it.

use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use astli_package::{Submodules, lfs_object, mark};
use gix::ObjectId;
use gix::filter::plumbing::eol::{AutoCrlf, Mode};
use gix::index::entry::Mode as Kind;
use gix::progress::Discard;
use gix::progress::prodash::tree::Root;
use gix::remote::Direction;
use semver::Version;

use crate::report::step;
use crate::store::{hash, key};
use crate::{Error, Report, Step, Store, canonical};

/// Every branch and tag, each under its own name, as a bare clone with
/// `--mirror` keeps them, so that a `rev` names what it names upstream.
const REFSPECS: [&str; 2] = ["+refs/heads/*:refs/heads/*", "+refs/tags/*:refs/tags/*"];

/// A bare mirror of the repository at a URL, and where its checkouts go.
pub struct Mirror {
    pub url: String,
    /// The dependency it was opened for, which its fetches are reported as.
    pub(crate) name: String,
    /// What the store keys it by.
    key: String,
    repo: gix::Repository,
    checkouts: PathBuf,
    /// Whether it was cloned or fetched through the store just now, so that
    /// fetching again gains nothing.
    pub fresh: bool,
    /// The store it came from, which a submodule's mirror comes from too.
    store: Store,
    /// Taken for each write, a clone, a fetch or a checkout, so that two
    /// runs never write to one at once, and let go after it. A run holds
    /// one mirror's lock at a time, never waiting for another's with it, so
    /// runs that open the same mirrors in another order never wait on each
    /// other for ever. Reading needs no lock: git replaces a ref or adds a
    /// pack whole.
    lock: PathBuf,
}

impl Mirror {
    /// The mirror of `url` in `store`, cloned if there is none. A clone is
    /// made beside its place and renamed into it once fetched, so that one
    /// cut short is never taken for a mirror.
    pub(crate) fn open(store: &Store, name: &str, url: &str) -> Result<Mirror, Error> {
        let key = key(url)?;
        let db = store.root.join("git").join("db");
        std::fs::create_dir_all(&db).map_err(|err| Error::Io(db.clone(), err))?;
        let lock = db.join(format!("{key}.lock"));
        let dir = db.join(&key);
        if store.offline && !dir.is_dir() {
            return Err(Error::Git(format!(
                "{} comes from {}, which is not in the cache, and an offline run clones nothing",
                mark::name(name),
                mark::path(url)
            )));
        }
        // Checked again under the lock, since another run may have cloned
        // it while this one waited.
        let held = (!dir.is_dir()).then(|| locked(&lock)).transpose()?;
        let cloned = held.is_some() && !dir.is_dir();
        if cloned {
            let partial = db.join(format!("{key}.partial"));
            remove(&partial)?;
            let repo = gix::init_bare(&partial).map_err(failed(url, "make a mirror of"))?;
            fetch(
                &repo,
                url,
                &REFSPECS,
                &*store.report,
                Step::Fetching { name, url },
            )?;
            drop(repo);
            std::fs::rename(&partial, &dir).map_err(|err| Error::Io(dir.clone(), err))?;
        }
        drop(held);
        let repo = gix::open(&dir).map_err(failed(url, "open the mirror of"))?;
        let checkouts = store.root.join("git").join("checkouts").join(&key);
        let mut fetched = store.fetched.lock().unwrap_or_else(|err| err.into_inner());
        if cloned {
            fetched.insert(key.clone());
        }
        let fresh = fetched.contains(&key);
        drop(fetched);
        Ok(Mirror {
            url: url.to_string(),
            name: name.to_string(),
            key,
            repo,
            checkouts,
            fresh,
            store: store.clone(),
            lock,
        })
    }

    /// The mirror's directory, a bare repository.
    pub fn dir(&self) -> &Path {
        self.repo.git_dir()
    }

    /// Fetches every branch and tag again. A branch or tag deleted upstream
    /// stays, and a commit no branch or tag reaches is not fetched.
    pub fn fetch(&mut self) -> Result<(), Error> {
        let (name, url) = (&self.name, &self.url);
        if self.store.offline {
            return Err(Error::Git(format!(
                "cannot fetch {} for {}: an offline run fetches nothing",
                mark::path(url),
                mark::name(name)
            )));
        }
        let _held = locked(&self.lock)?;
        fetch(
            &self.repo,
            url,
            &REFSPECS,
            &*self.store.report,
            Step::Fetching { name, url },
        )?;
        let mut fetched = self
            .store
            .fetched
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        fetched.insert(self.key.clone());
        Ok(())
    }

    /// Fetches one commit by its id, which a forge serves even where no
    /// branch or tag reaches it any more, as after a force-push. It is kept
    /// under `refs/astli/`, so that it stays in the mirror.
    pub fn fetch_commit(&mut self, id: ObjectId) -> Result<(), Error> {
        let (name, url) = (&self.name, &self.url);
        if self.store.offline {
            return Err(Error::Git(format!(
                "the cache of {} for {} has no commit {}, and an offline run fetches nothing",
                mark::path(url),
                mark::name(name),
                mark::version(id)
            )));
        }
        let refspecs = [format!("+{id}:refs/astli/{id}")];
        let _held = locked(&self.lock)?;
        fetch(
            &self.repo,
            url,
            &refspecs,
            &*self.store.report,
            Step::Fetching { name, url },
        )
    }

    /// Each version the repository tags, `vX.Y.Z` as semver after its `v`,
    /// and the commit it tags, in ascending order. A tag of another form is
    /// not a version.
    pub fn versions(&self) -> Result<Vec<(Version, ObjectId)>, Error> {
        let read = failed(&self.url, "read the tags of");
        let references = self.repo.references().map_err(&read)?;
        let mut versions = Vec::new();
        for reference in references.tags().map_err(&read)? {
            let mut reference = reference.map_err(&read)?;
            let name = reference.name().shorten().to_string();
            let Some(version) = (name.strip_prefix('v')).and_then(|it| Version::parse(it).ok())
            else {
                continue;
            };
            // A tag of a tree or a blob tags no version.
            if let Ok(commit) = reference.peel_to_commit() {
                versions.push((version, commit.id));
            }
        }
        versions.sort();
        Ok(versions)
    }

    /// The commit `rev` names, as git reads it: a branch, a tag, or a
    /// commit's hash, in full or abbreviated where that is unambiguous.
    /// `None` if the mirror has no such commit.
    pub fn rev(&self, rev: &str) -> Option<ObjectId> {
        let spec = format!("{rev}^{{commit}}");
        let id = self.repo.rev_parse_single(spec.as_str()).ok()?;
        Some(id.detach())
    }

    /// Whether the mirror holds `commit`.
    pub fn has(&self, commit: ObjectId) -> bool {
        self.repo.find_commit(commit).is_ok()
    }

    /// The file at `path` in `commit`, such as its manifest, read from the
    /// mirror without a checkout. `None` if the commit holds no such file.
    pub fn file(&self, commit: ObjectId, path: &str) -> Result<Option<Vec<u8>>, Error> {
        let read = failed(
            &self.url,
            &format!("read {} at {} of", mark::path(path), mark::version(commit)),
        );
        let commit = self.repo.find_commit(commit).map_err(&read)?;
        let tree = commit.tree().map_err(&read)?;
        let Some(entry) = tree.lookup_entry_by_path(path).map_err(&read)? else {
            return Ok(None);
        };
        if !entry.mode().is_blob() {
            return Ok(None);
        }
        let object = entry.object().map_err(&read)?;
        Ok(Some(object.data.clone()))
    }

    /// `commit`'s files, written out the first time any design asks for
    /// them. The checkout has no `.git`, and a submodule is an empty
    /// directory: a dependency is the files its own repository holds.
    /// Written beside its place and renamed into it, so that one cut short
    /// is never taken for a checkout.
    ///
    /// Every design reads a checkout, so it must hold what the commit does
    /// and nothing else: its files are made read-only, and the list of them
    /// is kept beside it, for a walk to refuse a file put there by hand. The
    /// list is written last, so that a checkout without one, cut short or
    /// older than lists, is sealed again.
    pub fn checkout(&self, commit: ObjectId) -> Result<Checkout, Error> {
        self.checkout_as(commit, &Takes::default(), None)
    }

    /// [`checkout`](Mirror::checkout), with what `taken` asks for besides.
    /// The files of each submodule it takes are written where the commit
    /// records it. Each comes from a mirror of its own repository in the
    /// store, at the commit the gitlink names, from the URL `.gitmodules`
    /// gives at `commit`. With `lfs`, `git-lfs` writes each file stored
    /// with Git LFS, fetching what its mirror's `lfs/` lacks. A checkout
    /// taking either is kept apart from the plain one, so that a design
    /// that takes neither reads the commit's own files.
    pub fn checkout_with(&self, commit: ObjectId, taken: &Takes) -> Result<Checkout, Error> {
        self.checkout_as(commit, taken, None)
    }

    /// [`checkout_with`](Mirror::checkout_with), reported as the commit of
    /// the package `named` at `version`, if it has to be written.
    pub(crate) fn checkout_as(
        &self,
        commit: ObjectId,
        taken: &Takes,
        named: Option<(&str, Option<&Version>)>,
    ) -> Result<Checkout, Error> {
        let mut at = match &taken.submodules {
            Submodules::Paths(paths) if paths.is_empty() => commit.to_string(),
            Submodules::All => format!("{commit}-{}", hash("*")?),
            Submodules::Paths(paths) => {
                let listed: String = paths.iter().map(|it| format!("{it}\n")).collect();
                format!("{commit}-{}", hash(&listed)?)
            }
        };
        if taken.lfs {
            at += "-lfs";
        }
        let dir = self.checkouts.join(&at);
        let list = self.checkouts.join(format!("{at}.files"));
        if dir.is_dir()
            && let Ok(text) = std::fs::read_to_string(&list)
        {
            let files = text.lines().map(|it| dir.join(it)).collect();
            return Ok(Checkout { dir, files });
        }
        // Fetched before this mirror is locked, since a run never waits for
        // one mirror's lock holding another's.
        let lfs = taken.lfs;
        let taken = self.submodules(commit, &taken.submodules, &mut Vec::new())?;
        if lfs {
            self.lfs_ready(commit, &taken)?;
        }
        // Checked again under the lock, since another run may have written
        // it while this one waited.
        let _held = locked(&self.lock)?;
        if !dir.is_dir() {
            let write = || self.write(commit, &at, &dir, &taken, lfs);
            match named {
                Some((name, version)) => {
                    let checking_out = Step::CheckingOut {
                        name,
                        version,
                        commit,
                    };
                    step(&*self.store.report, checking_out, write)?;
                }
                None => write()?,
            }
        }
        let files = match std::fs::read_to_string(&list) {
            Ok(text) => text.lines().map(PathBuf::from).collect(),
            Err(_) => {
                let mut files = self.listed(commit)?;
                for it in &taken {
                    let within = it.mirror.listed(it.commit)?.into_iter();
                    files.extend(within.map(|file| Path::new(&it.path).join(file)));
                }
                sealed(&dir, &files)?;
                let text: String = files
                    .iter()
                    .map(|it| format!("{}\n", it.to_string_lossy().replace('\\', "/")))
                    .collect();
                // Renamed into place, since a run reads it without the lock.
                let partial = self.checkouts.join(format!("{at}.files.partial"));
                std::fs::write(&partial, text).map_err(|err| Error::Io(partial.clone(), err))?;
                std::fs::rename(&partial, &list).map_err(|err| Error::Io(list.clone(), err))?;
                files
            }
        };
        let files = files.iter().map(|it| dir.join(it)).collect();
        Ok(Checkout { dir, files })
    }

    /// The submodules of `commit` that `wanted` takes, each in a mirror that
    /// holds the commit its gitlink names, outer before inner. `within` holds
    /// the repositories, by canonical URL, and commits this one is nested
    /// in.
    fn submodules(
        &self,
        commit: ObjectId,
        wanted: &Submodules,
        within: &mut Vec<(String, ObjectId)>,
    ) -> Result<Vec<Taken>, Error> {
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        let declared = self.declared(commit)?;
        if let Submodules::Paths(paths) = wanted {
            for path in paths {
                let holder = (declared.iter())
                    .find(|(at, ..)| path == at || path.starts_with(&format!("{at}/")));
                let why = match holder {
                    Some((at, ..)) if at == path || paths.contains(at) => continue,
                    Some((at, ..)) => format!(
                        "it is in the submodule {}, which is not listed",
                        mark::path(at)
                    ),
                    None => "it is not a submodule there".to_string(),
                };
                return Err(Error::Git(format!(
                    "{} takes the submodule {} at {} of {}, but {why}",
                    mark::name(&self.name),
                    mark::path(path),
                    mark::version(commit),
                    mark::path(&self.url)
                )));
            }
        }
        within.push((canonical(&self.url), commit));
        let mut taken = Vec::new();
        for (path, url, id) in declared {
            let nested = match wanted {
                Submodules::All => Submodules::All,
                Submodules::Paths(paths) if paths.contains(&path) => {
                    let inside = format!("{path}/");
                    Submodules::Paths(
                        (paths.iter())
                            .filter_map(|it| it.strip_prefix(&inside))
                            .map(str::to_string)
                            .collect(),
                    )
                }
                Submodules::Paths(_) => continue,
            };
            let url = match url.starts_with("./") || url.starts_with("../") {
                true => joined(&self.url, &url),
                false => url,
            };
            let failed = |why: String| {
                Error::Git(format!(
                    "cannot check out the submodule {} of {} from {}: {why}",
                    mark::path(&path),
                    mark::name(&self.name),
                    mark::path(&url)
                ))
            };
            if within.contains(&(canonical(&url), id)) {
                return Err(failed("it holds itself".to_string()));
            }
            let name = format!("{}/{path}", self.name);
            let mut mirror =
                (self.store.mirror(&name, &url)).map_err(|err| failed(err.to_string()))?;
            if !mirror.has(id) {
                (mirror.fetch_commit(id)).map_err(|err| failed(err.to_string()))?;
            }
            if !mirror.has(id) {
                return Err(failed(format!("it has no commit {}", mark::version(id))));
            }
            let inner = mirror.submodules(id, &nested, within)?;
            taken.push(Taken {
                path: path.clone(),
                mirror,
                commit: id,
            });
            taken.extend(inner.into_iter().map(|it| Taken {
                path: format!("{path}/{}", it.path),
                ..it
            }));
        }
        within.pop();
        Ok(taken)
    }

    /// The submodules `commit` records: each one's path, its URL as
    /// `.gitmodules` writes it, and the commit its gitlink names. One
    /// without a gitlink, or a gitlink without a URL, is left out, as git
    /// leaves it.
    fn declared(&self, commit: ObjectId) -> Result<Vec<(String, String, ObjectId)>, Error> {
        let Some(bytes) = self.file(commit, ".gitmodules")? else {
            return Ok(Vec::new());
        };
        let read = failed(
            &self.url,
            &format!(
                "read {} at {} of",
                mark::path(".gitmodules"),
                mark::version(commit)
            ),
        );
        let modules =
            gix::submodule::File::from_bytes(&bytes, None, &Default::default()).map_err(&read)?;
        let tree = (self.repo.find_commit(commit).map_err(&read)?)
            .tree()
            .map_err(&read)?;
        let mut declared = Vec::new();
        for name in modules.names() {
            let path = modules.path(name).map_err(&read)?.to_string();
            let Some(url) = modules.config().string_by("submodule", Some(name), "url") else {
                continue;
            };
            let Some(entry) = tree.lookup_entry_by_path(&path).map_err(&read)? else {
                continue;
            };
            if entry.mode().is_commit() {
                declared.push((path, url.to_string(), entry.object_id()));
            }
        }
        Ok(declared)
    }

    /// The paths of the files `commit` holds, relative and `/`-separated, a
    /// submodule left out.
    fn listed(&self, commit: ObjectId) -> Result<Vec<PathBuf>, Error> {
        let read = failed(
            &self.url,
            &format!("list the files of {} of", mark::version(commit)),
        );
        let tree = (self.repo.find_commit(commit).map_err(&read)?)
            .tree_id()
            .map_err(&read)?;
        let index = self.repo.index_from_tree(&tree).map_err(&read)?;
        Ok((index.entries().iter())
            .filter(|entry| !entry.mode.is_submodule())
            .map(|entry| PathBuf::from(entry.path(&index).to_string()))
            .collect())
    }

    /// Whether a checkout of `commit` and of the submodules `taken` with
    /// their LFS files can be written: `git-lfs` runs, and an offline run
    /// finds every one of the files in its mirror's `lfs/`, so that
    /// `git-lfs` never goes to the network for one.
    fn lfs_ready(&self, commit: ObjectId, taken: &[Taken]) -> Result<(), Error> {
        let asked = format!(
            "{} asks for the files its repository stores with Git LFS",
            mark::name(&self.name)
        );
        let ran = std::process::Command::new(LFS)
            .arg("version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match ran {
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::Git(format!(
                    "{asked}, which takes {}, and it is not installed",
                    mark::code(LFS)
                )));
            }
            Err(err) => {
                return Err(Error::Git(format!(
                    "{asked}, but {} does not run: {err}",
                    mark::code(LFS)
                )));
            }
        }
        if !self.store.offline {
            return Ok(());
        }
        let mut missing = self.lfs_missing(commit)?;
        for it in taken {
            let within = it.mirror.lfs_missing(it.commit)?.into_iter();
            missing.extend(within.map(|file| format!("{}/{file}", it.path)));
        }
        let Some(first) = missing.first() else {
            return Ok(());
        };
        let more = match missing.len() {
            1 => String::new(),
            2 => format!(" and {}", mark::path(&missing[1])),
            n => format!(" and {} more", n - 1),
        };
        Err(Error::Git(format!(
            "{asked}, and the cache lacks {}{more}, which an offline run does not fetch",
            mark::path(first)
        )))
    }

    /// The paths of the files of `commit` that are Git LFS pointers whose
    /// object the mirror's `lfs/` lacks. A pointer is a small file, so a
    /// larger one is never read.
    fn lfs_missing(&self, commit: ObjectId) -> Result<Vec<String>, Error> {
        let read = failed(
            &self.url,
            &format!("read the files of {} of", mark::version(commit)),
        );
        let tree = (self.repo.find_commit(commit).map_err(&read)?)
            .tree_id()
            .map_err(&read)?;
        let index = self.repo.index_from_tree(&tree).map_err(&read)?;
        let objects = self.dir().join("lfs").join("objects");
        let mut missing = Vec::new();
        for entry in index.entries() {
            if !matches!(entry.mode, Kind::FILE | Kind::FILE_EXECUTABLE) {
                continue;
            }
            let header = self.repo.find_header(entry.id).map_err(&read)?;
            if header.size() > POINTER_SIZE {
                continue;
            }
            let blob = self.repo.find_object(entry.id).map_err(&read)?;
            let Some(oid) = lfs_object(&blob.data) else {
                continue;
            };
            if !objects.join(&oid[..2]).join(&oid[2..4]).join(oid).is_file() {
                missing.push(entry.path(&index).to_string());
            }
        }
        Ok(missing)
    }

    /// Writes `commit`'s files to `dir`, and those of each of `taken`, by
    /// way of a directory named for `at` beside it, with their LFS files if
    /// `lfs`.
    fn write(
        &self,
        commit: ObjectId,
        at: &str,
        dir: &Path,
        taken: &[Taken],
        lfs: bool,
    ) -> Result<(), Error> {
        let partial = self.checkouts.join(format!("{at}.partial"));
        remove(&partial)?;
        std::fs::create_dir_all(&partial).map_err(|err| Error::Io(partial.clone(), err))?;
        self.write_tree(commit, &partial, lfs)?;
        for it in taken {
            let into = partial.join(&it.path);
            std::fs::create_dir_all(&into).map_err(|err| Error::Io(into.clone(), err))?;
            it.mirror.write_tree(it.commit, &into, lfs)?;
        }
        std::fs::rename(&partial, dir).map_err(|err| Error::Io(dir.to_path_buf(), err))
    }

    /// Writes `commit`'s files into `dir`, which is empty, and with `lfs`
    /// what each file stored with Git LFS holds rather than its pointer.
    fn write_tree(&self, commit: ObjectId, dir: &Path, lfs: bool) -> Result<(), Error> {
        let doing = format!("check out {} of", mark::version(commit));
        let write = failed(&self.url, &doing);
        let tree = (self.repo.find_commit(commit).map_err(&write)?)
            .tree_id()
            .map_err(&write)?;
        let mut index = self.repo.index_from_tree(&tree).map_err(&write)?;
        let source = gix::worktree::stack::state::attributes::Source::IdMapping;
        let mut options = self.repo.checkout_options(source).map_err(&write)?;
        options.destination_is_initially_empty = true;
        // Line endings as the repository holds them, whatever this machine's
        // `core.autocrlf` or `core.eol` say, and no filter program it
        // configures: a checkout is shared by every design and must read the
        // same on every machine. Only the repository's own attributes apply.
        let filters = options.filters.options_mut();
        filters.eol_config.auto_crlf = AutoCrlf::Disabled;
        filters.eol_config.eol = Some(Mode::Lf);
        filters.drivers.clear();
        // `git-lfs` alone, as git runs it once `git lfs install` has, and
        // only where a requirement asks: what it writes is the file the
        // pointer names, the same on every machine that has the program.
        if lfs {
            self.serve_lfs()?;
            filters.drivers.push(gix::filter::plumbing::Driver {
                name: "lfs".into(),
                clean: None,
                smudge: None,
                process: Some(format!("{LFS} filter-process").into()),
                required: true,
            });
        }
        let objects = (self.repo.objects.clone().into_arc()).map_err(failed(&self.url, &doing))?;
        let outcome = gix::worktree::state::checkout(
            &mut index,
            dir,
            objects,
            &Discard,
            &Discard,
            &AtomicBool::new(false),
            options,
        )
        .map_err(&write)?;
        if let Some(error) = outcome.errors.first() {
            return Err(Error::Git(format!(
                "cannot {doing} {}: {}: {}",
                mark::path(&self.url),
                mark::path(&error.path),
                error.error
            )));
        }
        Ok(())
    }

    /// Points `git-lfs` at the server it fetches from: the mirror's
    /// `remote.origin.url`, which `git-lfs` derives its server's URL from as
    /// for a clone, over HTTPS, SSH or a local path alike. A mirror is
    /// fetched from a URL rather than a remote, and HTTPS and SSH share one,
    /// so it is set to the URL this mirror was opened by, each time. The
    /// configuration is written beside its place and renamed into it,
    /// since a run opens a mirror without its lock.
    fn serve_lfs(&self) -> Result<(), Error> {
        let path = self.dir().join("config");
        let configure = "configure the mirror of";
        let mut config =
            gix::config::File::from_path_no_includes(path.clone(), gix::config::Source::Local)
                .map_err(failed(&self.url, configure))?;
        let url = self.url.as_str();
        if config
            .string("remote.origin.url")
            .is_some_and(|it| it.as_slice() == url.as_bytes())
        {
            return Ok(());
        }
        config
            .set_raw_value("remote.origin.url", url)
            .map_err(failed(&self.url, configure))?;
        let partial = self.dir().join("config.partial");
        let mut text = Vec::new();
        (config.write_to(&mut text)).map_err(failed(&self.url, configure))?;
        std::fs::write(&partial, text).map_err(|err| Error::Io(partial.clone(), err))?;
        std::fs::rename(&partial, &path).map_err(|err| Error::Io(path.clone(), err))
    }
}

/// The program that writes a file stored with Git LFS.
const LFS: &str = "git-lfs";

/// The largest a Git LFS pointer is, as its specification bounds it.
const POINTER_SIZE: u64 = 1024;

/// What a checkout holds besides its commit's own files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Takes {
    /// The submodules it writes in.
    pub submodules: Submodules,
    /// Whether it writes what each file stored with Git LFS holds, in its
    /// submodules too, rather than the pointer the commit holds.
    pub lfs: bool,
}

impl Takes {
    /// Adds what `other` takes.
    pub fn add(&mut self, other: &Takes) {
        self.submodules.add(&other.submodules);
        self.lfs |= other.lfs;
    }
}

/// A submodule a checkout takes, at the commit its gitlink names.
struct Taken {
    /// Where it goes, relative to the checkout and `/`-separated.
    path: String,
    mirror: Mirror,
    commit: ObjectId,
}

/// The URL `relative`, which starts with `./` or `../`, names beside the
/// repository at `base`: each `../` drops a component of `base`'s path, as
/// git reads a submodule's URL against its superproject's. A Windows path's
/// components end at a `\`.
fn joined(base: &str, relative: &str) -> String {
    let mut base = base.trim_end_matches('/');
    let mut rest = relative;
    // What the last component dropped followed, so that one dropped back to
    // the host of `host:path` stays after its `:`.
    let mut after = "/";
    loop {
        if let Some(more) = rest.strip_prefix("./") {
            rest = more;
        } else if let Some(more) = rest.strip_prefix("../") {
            rest = more;
            (base, after) = match base.rfind(['/', '\\', ':']) {
                Some(at) => (&base[..at], &base[at..=at]),
                None => ("", ""),
            };
        } else {
            break;
        }
    }
    format!("{base}{after}{rest}")
}

/// A commit's files, checked out.
#[derive(Debug, Clone)]
pub struct Checkout {
    pub dir: PathBuf,
    /// Every file the commit holds, absolute: anything else in `dir` was
    /// put there by hand.
    pub files: BTreeSet<PathBuf>,
}

/// Makes each of `files` under `dir` read-only. A directory stays writable,
/// so that the store can be deleted as any directory is, and a link is left
/// alone, since what it points at may be outside.
fn sealed(dir: &Path, files: &[PathBuf]) -> Result<(), Error> {
    for file in files {
        let path = dir.join(file);
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_file() {
            let mut permissions = metadata.permissions();
            permissions.set_readonly(true);
            std::fs::set_permissions(&path, permissions).map_err(|err| Error::Io(path, err))?;
        }
    }
    Ok(())
}

/// Fetches what `refspecs` name of `url` into `repo`, as `step`, telling
/// `report` how far the transfer has come while it runs.
fn fetch(
    repo: &gix::Repository,
    url: &str,
    refspecs: &[impl AsRef<str>],
    report: &dyn Report,
    as_step: Step<'_>,
) -> Result<(), Error> {
    let progress = Root::new();
    step(report, as_step, || {
        let (stop, stopped) = mpsc::channel::<()>();
        std::thread::scope(|scope| {
            scope.spawn(|| watch(&progress, report, &as_step, stopped));
            let fetched = transfer(repo, url, refspecs, &progress);
            drop(stop);
            fetched
        })
    })
}

/// Fetches what `refspecs` name of `url` into `repo`, counting what it
/// receives under `progress`.
fn transfer(
    repo: &gix::Repository,
    url: &str,
    refspecs: &[impl AsRef<str>],
    progress: &Root,
) -> Result<(), Error> {
    let fetching = failed(url, "fetch");
    // HTTPS through reqwest takes the process's rustls provider, and panics
    // without one. Installing fails only once one is in place, which is fine.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let remote = (repo.remote_at(url).map_err(&fetching)?)
        .with_refspecs(refspecs.iter().map(AsRef::as_ref), Direction::Fetch)
        .map_err(&fetching)?
        .with_fetch_tags(gix::remote::fetch::Tags::None);
    let connection = remote.connect(Direction::Fetch).map_err(&fetching)?;
    let prepared = (connection.prepare_fetch(Discard, Default::default())).map_err(&fetching)?;
    prepared
        .receive(progress.add_child("fetch"), &AtomicBool::new(false))
        .map_err(&fetching)?;
    Ok(())
}

/// Tells `report` of the task deepest in `progress` that counts something,
/// ten times a second, until `stopped` hangs up.
fn watch(progress: &Root, report: &dyn Report, step: &Step<'_>, stopped: Receiver<()>) {
    let mut tasks = Vec::new();
    while stopped.recv_timeout(Duration::from_millis(100)) == Err(RecvTimeoutError::Timeout) {
        progress.sorted_snapshot(&mut tasks);
        // In order of the hierarchy, so the last is the one running deepest.
        let counted = tasks.iter().rev().find_map(|(_, task)| {
            let value = task.progress.as_ref()?;
            let unit = value.unit.as_ref()?;
            let count = value.step.load(Ordering::Relaxed);
            Some(format!(
                "{} {}",
                task.name,
                unit.display(count, value.done_at, None)
            ))
        });
        if let Some(said) = counted {
            report.advance(step, &said);
        }
    }
}

/// What a failure to `doing` the repository at `url` is reported as.
fn failed<E: std::fmt::Display>(url: &str, doing: &str) -> impl Fn(E) -> Error + use<E> {
    let what = format!("cannot {doing} {}", mark::path(url));
    move |err| Error::Git(format!("{what}: {err}"))
}

/// The lock at `path`, taken once no other run holds it, and let go when
/// the file is dropped.
fn locked(path: &Path) -> Result<File, Error> {
    let lock = File::create(path).map_err(|err| Error::Io(path.to_path_buf(), err))?;
    lock.lock()
        .map_err(|err| Error::Io(path.to_path_buf(), err))?;
    Ok(lock)
}

/// Removes what an earlier run cut short left at `path`, if anything.
fn remove(path: &Path) -> Result<(), Error> {
    match std::fs::remove_dir_all(path) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            Err(Error::Io(path.to_path_buf(), err))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::joined;

    #[test]
    fn a_relative_url_is_read_beside_its_superproject() {
        let cases = [
            (
                "https://github.com/org/core.git",
                "../fpu.git",
                "https://github.com/org/fpu.git",
            ),
            (
                "https://github.com/org/core/",
                "./sub",
                "https://github.com/org/core/sub",
            ),
            (
                "git@github.com:org/core.git",
                "../../other/x",
                "git@github.com:other/x",
            ),
            (
                "git@github.com:org/core.git",
                "../fpu",
                "git@github.com:org/fpu",
            ),
            ("/srv/git/core", "../fpu", "/srv/git/fpu"),
            (r"C:\git\core", "../fpu", r"C:\git\fpu"),
        ];
        for (base, relative, expected) in cases {
            assert_eq!(joined(base, relative), expected, "{base} {relative}");
        }
    }
}
