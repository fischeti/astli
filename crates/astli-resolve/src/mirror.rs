//! A bare mirror of one repository: what resolution reads of it, and the
//! checkouts made from it.

use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use astli_package::mark;
use gix::ObjectId;
use gix::filter::plumbing::eol::{AutoCrlf, Mode};
use gix::progress::Discard;
use gix::progress::prodash::tree::Root;
use gix::remote::Direction;
use semver::Version;

use crate::report::step;
use crate::store::key;
use crate::{Error, Report, Step, Store};

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
    report: Arc<dyn Report>,
    fetched: Arc<Mutex<BTreeSet<String>>>,
    /// Held for as long as the mirror is, so that two runs never write to
    /// one at once.
    _lock: File,
}

impl Mirror {
    /// The mirror of `url` in `store`, cloned if there is none. A clone is
    /// made beside its place and renamed into it once fetched, so that one
    /// cut short is never taken for a mirror.
    pub(crate) fn open(store: &Store, name: &str, url: &str) -> Result<Mirror, Error> {
        let key = key(url)?;
        let db = store.root.join("git").join("db");
        std::fs::create_dir_all(&db).map_err(|err| Error::Io(db.clone(), err))?;
        let lock_path = db.join(format!("{key}.lock"));
        let lock = File::create(&lock_path).map_err(|err| Error::Io(lock_path.clone(), err))?;
        lock.lock().map_err(|err| Error::Io(lock_path, err))?;

        let dir = db.join(&key);
        let cloned = !dir.is_dir();
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
            report: store.report.clone(),
            fetched: store.fetched.clone(),
            _lock: lock,
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
        fetch(
            &self.repo,
            url,
            &REFSPECS,
            &*self.report,
            Step::Fetching { name, url },
        )?;
        let mut fetched = self.fetched.lock().unwrap_or_else(|err| err.into_inner());
        fetched.insert(self.key.clone());
        Ok(())
    }

    /// Fetches one commit by its id, which a forge serves even where no
    /// branch or tag reaches it any more, as after a force-push. It is kept
    /// under `refs/astli/`, so that it stays in the mirror.
    pub fn fetch_commit(&mut self, id: ObjectId) -> Result<(), Error> {
        let (name, url) = (&self.name, &self.url);
        let refspecs = [format!("+{id}:refs/astli/{id}")];
        fetch(
            &self.repo,
            url,
            &refspecs,
            &*self.report,
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
        self.checkout_as(commit, None)
    }

    /// [`checkout`](Mirror::checkout), reported as the commit of the package
    /// `named` at `version`, if it has to be written.
    pub(crate) fn checkout_as(
        &self,
        commit: ObjectId,
        named: Option<(&str, Option<&Version>)>,
    ) -> Result<Checkout, Error> {
        let dir = self.checkouts.join(commit.to_string());
        let list = self.checkouts.join(format!("{commit}.files"));
        if !dir.is_dir() {
            match named {
                Some((name, version)) => {
                    let checking_out = Step::CheckingOut {
                        name,
                        version,
                        commit,
                    };
                    step(&*self.report, checking_out, || self.write(commit, &dir))?;
                }
                None => self.write(commit, &dir)?,
            }
        }
        let files = match std::fs::read_to_string(&list) {
            Ok(text) => text.lines().map(PathBuf::from).collect(),
            Err(_) => {
                let files = self.listed(commit)?;
                sealed(&dir, &files)?;
                let text: String = files
                    .iter()
                    .map(|it| format!("{}\n", it.display()))
                    .collect();
                std::fs::write(&list, text).map_err(|err| Error::Io(list.clone(), err))?;
                files
            }
        };
        let files = files.iter().map(|it| dir.join(it)).collect();
        Ok(Checkout { dir, files })
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

    /// Writes `commit`'s files to `dir`.
    fn write(&self, commit: ObjectId, dir: &Path) -> Result<(), Error> {
        let partial = self.checkouts.join(format!("{commit}.partial"));
        remove(&partial)?;
        std::fs::create_dir_all(&partial).map_err(|err| Error::Io(partial.clone(), err))?;

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
        let objects = (self.repo.objects.clone().into_arc()).map_err(failed(&self.url, &doing))?;
        let outcome = gix::worktree::state::checkout(
            &mut index,
            &partial,
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
        std::fs::rename(&partial, dir).map_err(|err| Error::Io(dir.to_path_buf(), err))
    }
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

/// Removes what an earlier run cut short left at `path`, if anything.
fn remove(path: &Path) -> Result<(), Error> {
    match std::fs::remove_dir_all(path) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            Err(Error::Io(path.to_path_buf(), err))
        }
        _ => Ok(()),
    }
}
