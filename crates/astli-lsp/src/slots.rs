//! The designs the open buffers are in: each loaded on a thread of its own
//! once the edits to what it reads settle, and the snapshot it last loaded, swapped
//! in whole.
//!
//! A load takes the buffers as they were when it started, and asks between
//! units whether an edit has come since that it would have to read. Each
//! edit bumps the generation of every design that reads the file, at once,
//! so that a load running sees it at its next unit and stops; another is
//! made once the edits settle.

use std::collections::HashSet;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use astli_text::Reader;
use crossbeam_channel::Sender;

use crate::buffers::{Buffers, Texts, key};
use crate::tasks::panic_message;
use crate::{Host, Snapshot};

/// How long a design waits after an edit to what it reads before it is
/// loaded again: a little longer than the pause between keys of someone
/// typing, so that a word typed is one load rather than one a key, and
/// short enough that the design is current by the time they look.
const DEBOUNCE: Duration = Duration::from_millis(100);

/// The last generation handed to a slot. One count for every slot, so that
/// a load of a design dropped never matches the generation of one made
/// again at the same root.
static GENERATIONS: AtomicU64 = AtomicU64::new(0);

/// A generation no slot has had.
fn fresh() -> u64 {
    GENERATIONS.fetch_add(1, Ordering::SeqCst) + 1
}

/// The paths a load read, or asked whether they exist, as it reads them.
type Read = Arc<Mutex<HashSet<PathBuf>>>;

/// A design an open buffer is in.
pub struct Slot {
    /// What its last load that no edit overtook made.
    pub current: Option<Arc<Snapshot>>,
    /// What that load read.
    read: HashSet<PathBuf>,
    /// Bumped by each edit to what it reads, which overtakes every load
    /// started before it.
    generation: Arc<AtomicU64>,
    /// Whether its next load finds and reads the package again.
    discover: bool,
    /// When it is next loaded, once the edits settle.
    pub due: Option<Instant>,
    /// The generation the newest load running started at, and what it has
    /// read so far.
    running: Option<(u64, Read)>,
    /// How long the load of `current` took.
    pub took: Duration,
}

/// A load, done: what it made, or the panic it ended in.
pub struct Loaded {
    pub root: PathBuf,
    started: u64,
    discover: bool,
    outcome: std::thread::Result<Option<Snapshot>>,
    texts: Texts,
    read: Read,
    took: Duration,
}

impl Slot {
    /// A design no load has read yet, whose first load discovers it.
    pub fn new() -> Slot {
        Slot {
            current: None,
            read: HashSet::new(),
            generation: Arc::new(AtomicU64::new(fresh())),
            discover: true,
            due: None,
            running: None,
            took: Duration::ZERO,
        }
    }

    /// Whether the design's last load read the file at `path`, or the load
    /// running has.
    pub fn reads(&self, path: &Path) -> bool {
        self.read.contains(path)
            || (self.running.as_ref()).is_some_and(|(_, read)| held(read).contains(path))
    }

    /// Overtakes every load running, and makes the design due once the
    /// edits settle.
    pub fn stale(&mut self, now: Instant) {
        self.overtake();
        self.due = Some(now + DEBOUNCE);
    }

    /// Overtakes every load running, as when the design is dropped.
    pub fn overtake(&self) {
        self.generation.store(fresh(), Ordering::SeqCst);
    }

    /// Loads the design at `root` on a thread of its own, reading `texts`
    /// before the disk, and sends what it made on `loads`, or the panic it
    /// ended in.
    ///
    /// Not on the pool: a load waits for the units it spreads over the
    /// pool, and a pool thread that waited would hold one they need, or
    /// with as many loads as threads, every one.
    pub fn load(
        &mut self,
        root: &Path,
        texts: Texts,
        host: &Arc<dyn Host>,
        loads: &Sender<Loaded>,
    ) {
        self.due = None;
        let started = self.generation.load(Ordering::SeqCst);
        let read = Read::default();
        self.running = Some((started, read.clone()));
        let discover = std::mem::take(&mut self.discover);
        let (root, host, loads) = (root.to_path_buf(), host.clone(), loads.clone());
        let generation = self.generation.clone();
        let loading = std::thread::Builder::new().name("astli-load".to_string());
        let spawned = loading.spawn(move || {
            let begun = Instant::now();
            let reader = Recording {
                texts: &texts,
                read: &read,
            };
            let overtaken = || generation.load(Ordering::SeqCst) != started;
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                host.load(&root, &reader, discover, &overtaken)
            }));
            let _ = loads.send(Loaded {
                root,
                started,
                discover,
                outcome,
                texts,
                read,
                took: begun.elapsed(),
            });
        });
        spawned.expect("a thread to load on");
    }

    /// Takes the snapshot `loaded` made as the design's current one, unless
    /// an edit overtook it, and hands it back; `Err` with what a load that
    /// panicked said. What is replaced is dropped off the loop. A load not
    /// taken may have discovered the package for none that is, so the next
    /// discovers again.
    ///
    /// A buffer `buffers` holds otherwise than the load read it was edited
    /// before the load reached it, which overtakes nothing: the snapshot is
    /// still taken, as the nearest there is, and the design loaded again.
    pub fn swap(
        &mut self,
        loaded: Loaded,
        buffers: &Buffers,
    ) -> Result<Option<Arc<Snapshot>>, String> {
        let Loaded {
            started,
            discover,
            outcome,
            texts,
            read,
            took,
            ..
        } = loaded;
        if (self.running.as_ref()).is_some_and(|(it, _)| *it == started) {
            self.running = None;
        }
        let current = self.generation.load(Ordering::SeqCst) == started;
        let snapshot = match outcome {
            Ok(Some(snapshot)) if current => snapshot,
            Ok(overtaken) => {
                self.discover |= discover;
                drop_later((overtaken, texts));
                return Ok(None);
            }
            Err(payload) => {
                self.discover |= discover;
                return Err(panic_message(&*payload).to_string());
            }
        };
        let read = std::mem::take(&mut *held(&read));
        let edited = read
            .iter()
            .any(|path| match (texts.get(path), buffers.get(path)) {
                (Some(was), Some(is)) => !Arc::ptr_eq(was, &is.text),
                (was, is) => was.is_some() != is.is_some(),
            });
        let snapshot = Arc::new(snapshot);
        let replaced = self.current.replace(snapshot.clone());
        self.read = read;
        self.took = took;
        if edited {
            self.stale(Instant::now());
        }
        drop_later((replaced, texts));
        Ok(Some(snapshot))
    }
}

/// Drops `value` on the pool: dropping a design takes tens of milliseconds,
/// which the loop would otherwise keep the client waiting for.
pub fn drop_later<T: Send + 'static>(value: T) {
    rayon::spawn(move || drop(value));
}

/// The buffers a load reads, noting each path it reads or asks after.
struct Recording<'a> {
    texts: &'a Texts,
    read: &'a Mutex<HashSet<PathBuf>>,
}

impl Recording<'_> {
    fn note(&self, path: &Path) {
        held(self.read).insert(key(path));
    }
}

impl Reader for Recording<'_> {
    fn exists(&self, path: &Path) -> bool {
        self.note(path);
        self.texts.exists(path)
    }

    fn read(&self, path: &Path) -> io::Result<String> {
        self.note(path);
        self.texts.read(path)
    }
}

/// `mutex`, held, whether or not a load panicked holding it: a set of
/// paths is whole between inserts.
fn held<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|it| it.into_inner())
}
