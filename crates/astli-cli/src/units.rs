//! The units a design root's loads keep lowered, for a caller that loads
//! the same design again after an edit: a unit is lowered again only if a
//! file it read, or a path it asked after, reads otherwise now.
//!
//! What a unit lowers to depends on its build and on what its reader
//! answered, and on nothing else: units share no macro state. So each unit
//! is lowered through a reader that records every answer, its own text,
//! every header and every include directory searched, and is kept while
//! each answer is the same.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use astli_preproc::Build;
use astli_text::{Origins, Reader, SourceId};

use crate::analysis::Lowered;
use crate::error::Result;

/// The units of one design root, each lowered, by its path and its build.
pub(crate) type Units = HashMap<Key, Arc<Unit>>;

/// A unit's path and its build, the build known by its place: a build
/// holds every header of its package, too many paths to clone and hash for
/// each unit. Each load finds the one place of each of its builds among
/// those its units were kept with, by comparing it whole, once.
#[derive(Clone)]
pub(crate) struct Key(PathBuf, Arc<Build>);

impl PartialEq for Key {
    fn eq(&self, other: &Key) -> bool {
        self.0 == other.0 && Arc::ptr_eq(&self.1, &other.1)
    }
}

impl Eq for Key {}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
        Arc::as_ptr(&self.1).hash(state);
    }
}

/// A unit as lowered, and what its lowering read.
pub(crate) struct Unit {
    lowered: Lowered,
    /// Each path asked after, with whether it existed, in the order first
    /// asked.
    probed: Vec<(PathBuf, bool)>,
    /// Each path read, with what it read, in the order first read.
    read: Vec<(PathBuf, Text)>,
}

/// What reading a path gave a unit's lowering.
enum Text {
    /// The text its origins hold as the file, which keeps no second copy
    /// of the sources.
    Held(SourceId),
    /// A text its origins do not hold, such as a header whose include was
    /// a cycle.
    Own(Arc<str>),
    /// An error, as it is told.
    Failed(String),
}

/// One load of a design root: the units an earlier load kept, those this
/// load has used or lowered, and each answer the reader gave it.
pub(crate) struct Load {
    before: Arc<Units>,
    used: Mutex<Units>,
    /// Each path's answer, asked once in the load however many units read
    /// it, so that a header is read once.
    probed: Mutex<HashMap<PathBuf, bool>>,
    read: Mutex<HashMap<PathBuf, Read>>,
    /// Each build a unit of the load or of `before` was lowered with.
    builds: Mutex<Vec<Arc<Build>>>,
    lowered: AtomicUsize,
}

/// A path's text, or the kind and the message of the error reading it.
type Read = std::result::Result<Arc<str>, (io::ErrorKind, String)>;

impl Load {
    /// A load that may take the units in `before`.
    pub(crate) fn new(before: Arc<Units>) -> Load {
        let mut builds: Vec<Arc<Build>> = Vec::new();
        for Key(_, build) in before.keys() {
            if !builds.iter().any(|it| Arc::ptr_eq(it, build)) {
                builds.push(build.clone());
            }
        }
        Load {
            before,
            used: Mutex::default(),
            probed: Mutex::default(),
            read: Mutex::default(),
            builds: Mutex::new(builds),
            lowered: AtomicUsize::new(0),
        }
    }

    /// The one place of `build` among the load's builds.
    fn build(&self, build: &Build) -> Arc<Build> {
        let mut builds = held(&self.builds);
        if let Some(found) = builds.iter().find(|it| ***it == *build) {
            return found.clone();
        }
        let build = Arc::new(build.clone());
        builds.push(build.clone());
        build
    }

    /// The file at `path` lowered with `build`: as this load lowered it
    /// already, as an earlier load did if everything it read through
    /// `reader` reads the same, or else by `lower` through the reader it is
    /// given.
    pub(crate) fn lowered(
        &self,
        path: &Path,
        build: &Build,
        reader: &dyn Reader,
        lower: impl FnOnce(&dyn Reader) -> Result<Lowered>,
    ) -> Result<Lowered> {
        let key = Key(path.to_path_buf(), self.build(build));
        if let Some(unit) = held(&self.used).get(&key) {
            return Ok(unit.lowered.clone());
        }
        let answers = Answers { load: self, reader };
        let kept = (self.before.get(&key)).filter(|unit| answers.same(unit));
        let unit = match kept {
            Some(unit) => unit.clone(),
            None => {
                let recording = Recording {
                    answers: &answers,
                    probed: Mutex::default(),
                    read: Mutex::default(),
                };
                let lowered = lower(&recording)?;
                self.lowered.fetch_add(1, Ordering::Relaxed);
                Arc::new(recording.unit(lowered, &answers))
            }
        };
        let lowered = unit.lowered.clone();
        held(&self.used).insert(key, unit);
        Ok(lowered)
    }

    /// How many units the load lowered rather than took.
    pub(crate) fn count(&self) -> usize {
        self.lowered.load(Ordering::Relaxed)
    }

    /// The units the load used or lowered.
    pub(crate) fn into_used(self) -> Units {
        self.used.into_inner().unwrap_or_else(|it| it.into_inner())
    }
}

/// The reader a load reads through, each path asked once.
struct Answers<'a> {
    load: &'a Load,
    reader: &'a dyn Reader,
}

impl Answers<'_> {
    fn exists(&self, path: &Path) -> bool {
        if let Some(&exists) = held(&self.load.probed).get(path) {
            return exists;
        }
        let exists = self.reader.exists(path);
        *(held(&self.load.probed).entry(path.to_path_buf())).or_insert(exists)
    }

    fn read(&self, path: &Path) -> Read {
        if let Some(read) = held(&self.load.read).get(path) {
            return read.clone();
        }
        // Read without the lock, so that the units on other threads read
        // on meanwhile; the first answer stored is everyone's.
        let read = match self.reader.read(path) {
            Ok(text) => Ok(Arc::from(text)),
            Err(err) => Err((err.kind(), err.to_string())),
        };
        let mut known = held(&self.load.read);
        known.entry(path.to_path_buf()).or_insert(read).clone()
    }

    /// Whether every answer `unit`'s lowering was given is given again.
    fn same(&self, unit: &Unit) -> bool {
        let origins = &unit.lowered.1;
        (unit.probed.iter()).all(|(path, exists)| self.exists(path) == *exists)
            && (unit.read.iter()).all(|(path, text)| match (text, self.read(path)) {
                (Text::Held(file), Ok(now)) => origins.text(*file) == &*now,
                (Text::Own(was), Ok(now)) => *was == now,
                (Text::Failed(was), Err((_, now))) => *was == now,
                _ => false,
            })
    }
}

/// The reader one unit is lowered through, noting each answer it is given.
struct Recording<'a> {
    answers: &'a Answers<'a>,
    probed: Mutex<Vec<(PathBuf, bool)>>,
    read: Mutex<Vec<PathBuf>>,
}

impl Recording<'_> {
    /// The unit `lowered` is, with what it read. A text is the load's
    /// answer, the same however often it was read.
    fn unit(self, lowered: Lowered, answers: &Answers) -> Unit {
        let origins: &Origins = &lowered.1;
        let mut held_as = HashMap::new();
        for file in origins.files() {
            if let Some(path) = origins.path(file) {
                held_as.entry(path).or_insert(file);
            }
        }
        let paths = self.read.into_inner().unwrap_or_else(|it| it.into_inner());
        let read = (paths.into_iter())
            .map(|path| {
                let text = match answers.read(&path) {
                    Ok(text) => match held_as.get(path.as_path()) {
                        Some(&file) if origins.text(file) == &*text => Text::Held(file),
                        _ => Text::Own(text),
                    },
                    Err((_, said)) => Text::Failed(said),
                };
                (path, text)
            })
            .collect();
        let probed = self
            .probed
            .into_inner()
            .unwrap_or_else(|it| it.into_inner());
        Unit {
            lowered,
            probed,
            read,
        }
    }
}

impl Reader for Recording<'_> {
    fn exists(&self, path: &Path) -> bool {
        let exists = self.answers.exists(path);
        let mut probed = held(&self.probed);
        if !probed.iter().any(|(it, _)| it == path) {
            probed.push((path.to_path_buf(), exists));
        }
        exists
    }

    fn read(&self, path: &Path) -> io::Result<String> {
        let read = self.answers.read(path);
        let mut paths = held(&self.read);
        if !paths.iter().any(|it| it == path) {
            paths.push(path.to_path_buf());
        }
        match read {
            Ok(text) => Ok(text.to_string()),
            Err((kind, said)) => Err(io::Error::new(kind, said)),
        }
    }
}

/// `mutex`, held, whether or not a unit's lowering panicked holding it:
/// each map and list is whole between inserts.
fn held<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|it| it.into_inner())
}
