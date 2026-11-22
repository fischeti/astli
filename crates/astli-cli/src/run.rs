//! What one run of a command shares across the packages and the designs it
//! reads: the flags it was given, where it reads source files from, the
//! store, and what one package's or one design's work found that another's
//! would only find again.
//!
//! Every cache is keyed on what the work it saves reads, so that a hit is
//! what the work would have made: a wrong key would hand a design another
//! one's files without a word.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Once, OnceLock};

use astli_package::{Files, Package, Walk};
use astli_resolve::{Lock, Overrides, Resolution, Store};
use astli_text::{Diagnostic, Disk, Reader};

use crate::analysis::Lowered;
use crate::cli::RunArgs;
use crate::discovery::Made;
use crate::error::{Error, Result};
use crate::term::{self, Problem};

/// A run of a command, made once in `main` and shared by reference with
/// every thread of it; or a load of a design for a caller that is no
/// command, which renders what the load finds itself.
pub struct Run<'r> {
    /// The flags every command takes.
    pub args: RunArgs,
    /// What every source file is read through: the disk, for a command.
    reader: &'r dyn Reader,
    /// Each problem the run found, kept in the order found for a caller
    /// that renders them itself; `None` for a command, which writes each
    /// as it is found.
    kept: Option<Mutex<Vec<Problem>>>,
    /// Whether an edit has made a load stale, for a caller that loads
    /// again once it has; never, for a command.
    overtaken: Option<&'r (dyn Fn() -> bool + Sync)>,
    store: OnceLock<Store>,
    warned: Once,
    /// Whether the command runs once for each of several packages, as
    /// `members` runs it under `--workspace` or at a virtual root.
    packages: AtomicBool,
    /// Whether each problem and finding is told once: the run checks
    /// several designs, of several packages or of one package's sets of
    /// defines, which read many of the same files.
    once: AtomicBool,
    /// Each resolution made, with what it was resolved from.
    resolved: Mutex<Vec<(Resolving, Resolution)>>,
    /// Each package's files as walked, with what the walk read. A walk is
    /// known by its place here.
    walked: Mutex<Vec<(Walked, Files)>>,
    /// Each file lowered, by its path and what its build was made from.
    lowered: Mutex<HashMap<(PathBuf, Made), Lowered>>,
    /// Each problem told, for a run that tells each once.
    told: Mutex<HashSet<Problem>>,
    /// Each finding in a file told, by the file's path, for a run that
    /// tells each once.
    found: Mutex<HashSet<(PathBuf, Diagnostic)>>,
}

/// What a resolution reads besides the store: the roots it resolves, the
/// workspace root and then its members, as their manifests say, the pins
/// and patches it is resolved with, and the lock it prefers. Whatever a
/// command changes between two resolutions, a requirement, a patch, a pin
/// or what it keeps of the lock, is in one of these.
#[derive(PartialEq)]
pub struct Resolving {
    pub roots: Vec<Package>,
    pub overrides: Overrides,
    pub preferred: Lock,
}

/// What a package's walk reads: the package as the design reads it, what
/// the run decided of it, its features enabled and the patches it skips
/// among the rest, and the include directories of its dependencies,
/// searched after its own.
type Walked = (Package, Walk, Vec<PathBuf>);

impl Run<'static> {
    /// A run of a command, reading from the disk and writing what it finds.
    pub fn new(args: RunArgs) -> Run<'static> {
        Run::with(args, &Disk, None)
    }
}

impl<'r> Run<'r> {
    /// A load of a design for a caller that stays up and renders what the
    /// load finds itself, such as an editor's server. It reads every source
    /// file through `reader`, reaches no network, writes no lock and draws
    /// no progress, and keeps each problem for [`Run::problems`] rather
    /// than writing it. Dependencies it cannot settle or read are a problem
    /// too, and the package is read without them.
    pub fn loading(reader: &'r dyn Reader) -> Run<'r> {
        let args = RunArgs {
            quiet: true,
            locked: true,
            offline: true,
            ..RunArgs::default()
        };
        Run::with(args, reader, Some(Mutex::default()))
    }

    /// Makes the load stop at the next unit once `overtaken` says an edit
    /// has made it stale, ending in [`Error::Overtaken`].
    ///
    /// [`Error::Overtaken`]: crate::error::Error::Overtaken
    pub fn abandoning(mut self, overtaken: &'r (dyn Fn() -> bool + Sync)) -> Run<'r> {
        self.overtaken = Some(overtaken);
        self
    }

    /// `Err` once an edit has made the load stale, for its work to return
    /// with `?`.
    pub fn current(&self) -> Result<()> {
        match self.overtaken.is_some_and(|it| it()) {
            true => Err(Error::Overtaken),
            false => Ok(()),
        }
    }

    fn with(args: RunArgs, reader: &'r dyn Reader, kept: Option<Mutex<Vec<Problem>>>) -> Run<'r> {
        Run {
            args,
            reader,
            kept,
            overtaken: None,
            store: OnceLock::new(),
            warned: Once::new(),
            packages: AtomicBool::new(false),
            once: AtomicBool::new(false),
            resolved: Mutex::default(),
            walked: Mutex::default(),
            lowered: Mutex::default(),
            told: Mutex::default(),
            found: Mutex::default(),
        }
    }

    /// What every source file is read through.
    pub fn reader(&self) -> &'r dyn Reader {
        self.reader
    }

    /// Whether the run keeps its problems for its caller, which then wants
    /// what it can load rather than a failure: a dependency that cannot be
    /// settled or read is a problem, and the package is read without it.
    pub fn loads_what_it_can(&self) -> bool {
        self.kept.is_some()
    }

    /// Keeps `problem` for the caller, if the run keeps its problems, and
    /// says whether it did.
    pub fn keep(&self, problem: &Problem) -> bool {
        let Some(kept) = &self.kept else {
            return false;
        };
        // A plan made again for other features settles its dependencies
        // again, and finds the same.
        let mut kept = held(kept);
        if !kept.contains(problem) {
            kept.push(problem.clone());
        }
        true
    }

    /// The problems kept since the last call, in the order found; none for
    /// a command's run, which wrote each.
    pub fn problems(&self) -> Vec<Problem> {
        (self.kept.as_ref()).map_or_else(Vec::new, |kept| std::mem::take(&mut *held(kept)))
    }

    /// The store the environment names, refusing every clone and fetch if
    /// the run is offline. A command's shows each clone, fetch and checkout
    /// it makes. One for the run, so that a mirror a command fetched is not
    /// fetched again by the resolution after it.
    pub fn store(&self) -> Result<Store> {
        if let Some(store) = self.store.get() {
            return Ok(store.clone());
        }
        let mut store = Store::from_env()?.offline(self.args.offline);
        if self.kept.is_none() {
            store = store.reporting(Arc::new(term::Steps));
        }
        Ok(self.store.get_or_init(|| store).clone())
    }

    /// Makes the run use `store`, for a test that keeps its cache apart.
    #[cfg(test)]
    pub fn storing(self, store: Store) -> Run<'r> {
        let _ = self.store.set(store);
        self
    }

    /// Makes the run one of the command for each of several packages, from
    /// here on. Each package's run resolves the whole workspace and reads
    /// the siblings it requires, so it tells only what none before it told.
    pub fn for_each_package(&self) {
        self.packages.store(true, Ordering::Relaxed);
        self.tell_once();
    }

    /// Whether the command runs once for each of several packages, whose
    /// designs hold mostly the same packages alike: then each package is
    /// walked, and each file lowered, once for all of them.
    pub fn several_packages(&self) -> bool {
        self.packages.load(Ordering::Relaxed)
    }

    /// Makes the run tell each problem and finding once, from here on, as
    /// one that checks a package once for each set of defines its tops
    /// give.
    pub fn tell_once(&self) {
        self.once.store(true, Ordering::Relaxed);
    }

    /// Whether the run tells each problem and finding once.
    pub fn tells_once(&self) -> bool {
        self.once.load(Ordering::Relaxed)
    }

    /// Runs `warn` the first time the run asks, and never again.
    pub fn warn_once(&self, warn: impl FnOnce()) {
        self.warned.call_once(warn);
    }

    /// The resolution of what `resolving` says, made by `resolve` unless
    /// the run made it already.
    pub fn resolution(
        &self,
        resolving: Resolving,
        resolve: impl FnOnce(&Resolving) -> Result<Resolution>,
    ) -> Result<Resolution> {
        if let Some((_, resolution)) =
            (held(&self.resolved).iter()).find(|(it, _)| *it == resolving)
        {
            return Ok(resolution.clone());
        }
        let resolution = resolve(&resolving)?;
        held(&self.resolved).push((resolving, resolution.clone()));
        Ok(resolution)
    }

    /// Notes that `resolving` resolves to `resolution`, as it does when it
    /// prefers the lock written from that resolution.
    pub fn resolved(&self, resolving: Resolving, resolution: &Resolution) {
        let mut resolved = held(&self.resolved);
        if !resolved.iter().any(|(it, _)| *it == resolving) {
            resolved.push((resolving, resolution.clone()));
        }
    }

    /// `package`'s files, walked as `walk` says and searching
    /// `dependencies`' include directories after its own, found by `files`
    /// unless the run walked them already, with the walk's place among the
    /// run's.
    pub fn walked<E>(
        &self,
        package: &Package,
        walk: &Walk,
        dependencies: &[PathBuf],
        files: impl FnOnce() -> std::result::Result<Files, E>,
    ) -> std::result::Result<(usize, Files), E> {
        let found = (held(&self.walked).iter().enumerate())
            .find(|(_, ((it, how, dirs), _))| it == package && how == walk && dirs == dependencies)
            .map(|(at, (_, files))| (at, files.clone()));
        if let Some(found) = found {
            return Ok(found);
        }
        let files = files()?;
        let mut walked = held(&self.walked);
        let key = (package.clone(), walk.clone(), dependencies.to_vec());
        walked.push((key, files.clone()));
        Ok((walked.len() - 1, files))
    }

    /// The file at `path` lowered with the build `made` says, by `lower`
    /// unless the run lowered it so already.
    pub fn lowered(
        &self,
        path: PathBuf,
        made: &Made,
        lower: impl FnOnce() -> Result<Lowered>,
    ) -> Result<Lowered> {
        let key = (path, made.clone());
        if let Some(found) = held(&self.lowered).get(&key) {
            return Ok(found.clone());
        }
        let found = lower()?;
        held(&self.lowered).insert(key, found.clone());
        Ok(found)
    }

    /// Whether `problem` is told for the first time in the run, for one
    /// that [tells each once](Run::tells_once).
    pub fn first_told(&self, problem: &Problem) -> bool {
        held(&self.told).insert(problem.clone())
    }

    /// Whether `diagnostic`, in the file at `path`, is told for the first
    /// time in the run, for one that checks several designs: the run of
    /// each member depending on a sibling also reports the sibling's files,
    /// and each set of defines a package's tops give the files they share,
    /// and these say the same again unless what builds them changes what
    /// they hold.
    pub fn first_found(&self, path: &std::path::Path, diagnostic: &Diagnostic) -> bool {
        held(&self.found).insert((path.to_path_buf(), diagnostic.clone()))
    }
}

/// `mutex`, held, whether or not a thread panicked holding it: a panic
/// ends the run, and what was cached before it is whole.
fn held<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|it| it.into_inner())
}
