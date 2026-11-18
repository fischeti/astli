//! What one run of a command shares across the packages and the designs it
//! reads: the flags it was given, the store, and what one package's or one
//! design's work found that another's would only find again.
//!
//! Every cache is keyed on what the work it saves reads, so that a hit is
//! what the work would have made: a wrong key would hand a design another
//! one's files without a word.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Once, OnceLock};

use astli_package::{Files, Package};
use astli_resolve::{Lock, Resolution, Store};
use astli_text::Diagnostic;

use crate::analysis::Lowered;
use crate::cli::RunArgs;
use crate::discovery::Made;
use crate::error::Result;
use crate::term::{self, Problem};

/// A run of a command, made once in `main` and shared by reference with
/// every thread of it.
pub struct Run {
    /// The flags every command takes.
    pub args: RunArgs,
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
    walked: Mutex<Vec<(Walk, Files)>>,
    /// Each file lowered, by its path and what its build was made from.
    lowered: Mutex<HashMap<(PathBuf, Made), Lowered>>,
    /// Each problem told, for a run that tells each once.
    told: Mutex<HashSet<Problem>>,
    /// Each finding in a file told, by the file's path, for a run that
    /// tells each once.
    found: Mutex<HashSet<(PathBuf, Diagnostic)>>,
}

/// What a resolution reads besides the store: the roots it resolves, the
/// workspace root and its members, as their manifests and patches say, and
/// the lock it prefers. Whatever a command changes between two resolutions,
/// a requirement, a patch, a pin or what it keeps of the lock, is in one of
/// these.
#[derive(PartialEq)]
pub struct Resolving {
    pub root: Package,
    pub members: Vec<Package>,
    pub preferred: Lock,
}

/// What a package's walk reads: the package as the design reads it, its
/// features enabled and its patches among the rest, and the include
/// directories of its dependencies, searched after its own.
type Walk = (Package, Vec<PathBuf>);

impl Run {
    pub fn new(args: RunArgs) -> Run {
        Run {
            args,
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

    /// The store the environment names, showing each clone, fetch and
    /// checkout it makes, and refusing them if the run is offline. One for
    /// the run, so that a mirror a command fetched is not fetched again by
    /// the resolution after it.
    pub fn store(&self) -> Result<Store> {
        if let Some(store) = self.store.get() {
            return Ok(store.clone());
        }
        let store =
            (Store::from_env()?.reporting(Arc::new(term::Steps))).offline(self.args.offline);
        Ok(self.store.get_or_init(|| store).clone())
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
    pub fn warn_once(&self, warn: impl FnOnce() -> Result) -> Result {
        let mut warned = Ok(());
        self.warned.call_once(|| warned = warn());
        warned
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

    /// `package`'s files, searching `dependencies`' include directories
    /// after its own, walked by `walk` unless the run walked them already,
    /// with the walk's place among the run's.
    pub fn walked<E>(
        &self,
        package: &Package,
        dependencies: &[PathBuf],
        walk: impl FnOnce() -> std::result::Result<Files, E>,
    ) -> std::result::Result<(usize, Files), E> {
        let found = (held(&self.walked).iter().enumerate())
            .find(|(_, ((it, dirs), _))| it == package && dirs == dependencies)
            .map(|(at, (_, files))| (at, files.clone()));
        if let Some(found) = found {
            return Ok(found);
        }
        let files = walk()?;
        let mut walked = held(&self.walked);
        walked.push(((package.clone(), dependencies.to_vec()), files.clone()));
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
