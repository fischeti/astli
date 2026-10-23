//! A bare mirror of one repository: what resolution reads of it, and the
//! checkouts made from it.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use gix::ObjectId;
use gix::filter::plumbing::eol::{AutoCrlf, Mode};
use gix::progress::Discard;
use gix::remote::Direction;
use semver::Version;

use crate::store::key;
use crate::{Error, Store};

/// Every branch and tag, each under its own name, as a bare clone with
/// `--mirror` keeps them, so that a `rev` names what it names upstream.
const REFSPECS: [&str; 2] = ["+refs/heads/*:refs/heads/*", "+refs/tags/*:refs/tags/*"];

/// A bare mirror of the repository at a URL, and where its checkouts go.
pub struct Mirror {
    pub url: String,
    repo: gix::Repository,
    checkouts: PathBuf,
    /// Whether it was cloned just now, so that fetching again gains nothing.
    pub fresh: bool,
    /// Held for as long as the mirror is, so that two runs never write to
    /// one at once.
    _lock: File,
}

impl Mirror {
    /// The mirror of `url` in `store`, cloned if there is none. A clone is
    /// made beside its place and renamed into it once fetched, so that one
    /// cut short is never taken for a mirror.
    pub(crate) fn open(store: &Store, url: &str) -> Result<Mirror, Error> {
        let key = key(url)?;
        let db = store.root.join("git").join("db");
        std::fs::create_dir_all(&db).map_err(|err| Error::Io(db.clone(), err))?;
        let lock_path = db.join(format!("{key}.lock"));
        let lock = File::create(&lock_path).map_err(|err| Error::Io(lock_path.clone(), err))?;
        lock.lock().map_err(|err| Error::Io(lock_path, err))?;

        let dir = db.join(&key);
        let fresh = !dir.is_dir();
        if fresh {
            let partial = db.join(format!("{key}.partial"));
            remove(&partial)?;
            let repo = gix::init_bare(&partial).map_err(failed(url, "make a mirror of"))?;
            fetch(&repo, url, &REFSPECS)?;
            drop(repo);
            std::fs::rename(&partial, &dir).map_err(|err| Error::Io(dir.clone(), err))?;
        }
        let repo = gix::open(&dir).map_err(failed(url, "open the mirror of"))?;
        let checkouts = store.root.join("git").join("checkouts").join(&key);
        Ok(Mirror {
            url: url.to_string(),
            repo,
            checkouts,
            fresh,
            _lock: lock,
        })
    }

    /// Fetches every branch and tag again. A branch or tag deleted upstream
    /// stays, and a commit no branch or tag reaches is not fetched.
    pub fn fetch(&mut self) -> Result<(), Error> {
        fetch(&self.repo, &self.url, &REFSPECS)
    }

    /// Fetches one commit by its id, which a forge serves even where no
    /// branch or tag reaches it any more, as after a force-push. It is kept
    /// under `refs/astli/`, so that it stays in the mirror.
    pub fn fetch_commit(&mut self, id: ObjectId) -> Result<(), Error> {
        fetch(&self.repo, &self.url, &[format!("+{id}:refs/astli/{id}")])
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

    /// The file at `path` in `commit`, such as its manifest, read from the
    /// mirror without a checkout. `None` if the commit holds no such file.
    pub fn file(&self, commit: ObjectId, path: &str) -> Result<Option<Vec<u8>>, Error> {
        let read = failed(&self.url, &format!("read `{path}` at {commit} of"));
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

    /// The directory holding `commit`'s files, written out the first time
    /// any design asks for it. It has no `.git`, and a submodule is an
    /// empty directory: a dependency is the files its own repository holds.
    /// Written beside its place and renamed into it, so that one cut short
    /// is never taken for a checkout.
    pub fn checkout(&self, commit: ObjectId) -> Result<PathBuf, Error> {
        let dir = self.checkouts.join(commit.to_string());
        if dir.is_dir() {
            return Ok(dir);
        }
        let partial = self.checkouts.join(format!("{commit}.partial"));
        remove(&partial)?;
        std::fs::create_dir_all(&partial).map_err(|err| Error::Io(partial.clone(), err))?;

        let doing = format!("check out {commit} of");
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
                self.url, error.path, error.error
            )));
        }
        std::fs::rename(&partial, &dir).map_err(|err| Error::Io(dir.clone(), err))?;
        Ok(dir)
    }
}

/// Fetches what `refspecs` name of `url` into `repo`.
fn fetch(repo: &gix::Repository, url: &str, refspecs: &[impl AsRef<str>]) -> Result<(), Error> {
    let fetching = failed(url, "fetch");
    let remote = (repo.remote_at(url).map_err(&fetching)?)
        .with_refspecs(refspecs.iter().map(AsRef::as_ref), Direction::Fetch)
        .map_err(&fetching)?
        .with_fetch_tags(gix::remote::fetch::Tags::None);
    let connection = remote.connect(Direction::Fetch).map_err(&fetching)?;
    let prepared = (connection.prepare_fetch(Discard, Default::default())).map_err(&fetching)?;
    prepared
        .receive(Discard, &AtomicBool::new(false))
        .map_err(&fetching)?;
    Ok(())
}

/// What a failure to `doing` the repository at `url` is reported as.
fn failed<E: std::fmt::Display>(url: &str, doing: &str) -> impl Fn(E) -> Error + use<E> {
    let what = format!("cannot {doing} {url}");
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
