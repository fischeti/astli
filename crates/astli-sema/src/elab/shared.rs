//! What an elaboration keeps where every thread reads it: maps of answers
//! split into shards, each behind a lock of its own, and an arena of bodies
//! that is only ever added to.

use std::collections::hash_map::Entry;
use std::hash::{BuildHasher, Hash};
use std::sync::OnceLock;
use std::thread::ThreadId;

use parking_lot::{Mutex, MutexGuard};
use rustc_hash::{FxBuildHasher, FxHashMap};

/// How many shards a map is split into: enough that threads asking about
/// different keys rarely take the same lock.
const SHARDS: usize = 64;

/// A map split by key into shards, so that threads asking about different
/// keys do not wait on each other.
pub(super) struct Shards<K, V> {
    shards: Box<[Mutex<FxHashMap<K, V>>]>,
}

impl<K: Hash + Eq, V> Default for Shards<K, V> {
    fn default() -> Self {
        Shards {
            shards: (0..SHARDS).map(|_| Mutex::default()).collect(),
        }
    }
}

impl<K: Hash + Eq, V> Shards<K, V> {
    /// The shard holding `key`, locked.
    pub(super) fn shard(&self, key: &K) -> MutexGuard<'_, FxHashMap<K, V>> {
        // The shard's map hashes the key again with the same hasher, and
        // takes a bucket from the low bits and a tag from the top seven. The
        // shard is taken from the bits between: from the low bits, every key
        // in a shard would start its probe at the same bucket of each 64.
        let hash = FxBuildHasher.hash_one(key);
        let at = (hash << 7 >> (64 - SHARDS.ilog2())) as usize;
        self.shards[at].lock()
    }

    pub(super) fn get(&self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        self.shard(key).get(key).cloned()
    }

    /// `value` under `key` unless another is already there, and what is
    /// there after: so that two threads that computed it both return the
    /// one kept.
    pub(super) fn keep(&self, key: K, value: V) -> V
    where
        V: Clone,
    {
        self.shard(&key).entry(key).or_insert(value).clone()
    }
}

/// A query computed, or being computed by the threads named: `first`, and
/// `others` should more ask at once, which is rare enough that the common
/// case allocates nothing.
enum Slot<T> {
    Busy {
        first: ThreadId,
        others: Vec<ThreadId>,
    },
    Done(T),
}

/// The answers to one query, by its arguments.
pub(super) struct Memo<K, T>(Shards<K, Slot<T>>);

impl<K: Hash + Eq, T> Default for Memo<K, T> {
    fn default() -> Self {
        Memo(Shards::default())
    }
}

thread_local! {
    static THREAD: ThreadId = std::thread::current().id();
}

impl<K: Hash + Eq + Copy, T: Clone> Memo<K, T> {
    /// The answer to the query `key`, computed if not yet; `cycle` if this
    /// thread is computing it already. Another thread computing it is not
    /// waited for: both compute it, and the first to finish is kept.
    pub(super) fn get(&self, key: K, cycle: T, compute: impl FnOnce() -> T) -> T {
        let me = THREAD.with(|it| *it);
        match self.0.shard(&key).entry(key) {
            Entry::Occupied(mut slot) => match slot.get_mut() {
                Slot::Done(value) => return value.clone(),
                Slot::Busy { first, others } => {
                    if *first == me || others.contains(&me) {
                        return cycle;
                    }
                    others.push(me);
                }
            },
            Entry::Vacant(slot) => {
                slot.insert(Slot::Busy {
                    first: me,
                    others: Vec::new(),
                });
            }
        }
        let value = compute();
        match self.0.shard(&key).entry(key) {
            Entry::Occupied(mut slot) => match slot.get() {
                Slot::Done(kept) => kept.clone(),
                Slot::Busy { .. } => {
                    slot.insert(Slot::Done(value.clone()));
                    value
                }
            },
            Entry::Vacant(slot) => {
                slot.insert(Slot::Done(value.clone()));
                value
            }
        }
    }
}

/// Values only ever added to, read by index without a lock: chunk `k`
/// holds `BASE << k` of them, so that an index finds its chunk by its
/// magnitude and nothing added moves.
pub(super) struct Arena<T> {
    chunks: [OnceLock<Box<[OnceLock<T>]>>; CHUNKS],
    len: Mutex<usize>,
}

const BASE: usize = 64;
const CHUNKS: usize = 32;

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Arena {
            chunks: std::array::from_fn(|_| OnceLock::new()),
            len: Mutex::new(0),
        }
    }
}

impl<T> Arena<T> {
    /// The chunk index `at` is in, and its place there.
    fn place(at: usize) -> (usize, usize) {
        let chunk = (at / BASE + 1).ilog2() as usize;
        (chunk, at - BASE * ((1 << chunk) - 1))
    }

    /// Adds `value`, and returns its index.
    pub(super) fn push(&self, value: T) -> usize {
        let mut len = self.len.lock();
        let at = *len;
        let (chunk, offset) = Self::place(at);
        let slots = self.chunks[chunk]
            .get_or_init(|| (0..BASE << chunk).map(|_| OnceLock::new()).collect());
        if slots[offset].set(value).is_err() {
            unreachable!("an arena's slot is set once");
        }
        *len += 1;
        at
    }

    /// The value at `at`, which a [`push`](Arena::push) returned.
    pub(super) fn get(&self, at: usize) -> &T {
        let (chunk, offset) = Self::place(at);
        let slot = self.chunks[chunk]
            .get()
            .and_then(|slots| slots[offset].get());
        slot.expect("an arena index comes from a push")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_arena_gives_back_what_was_pushed_across_chunks() {
        let arena = Arena::default();
        for at in 0..10_000 {
            assert_eq!(arena.push(at), at);
        }
        for at in [0, 63, 64, 191, 192, 9_999] {
            assert_eq!(*arena.get(at), at);
        }
    }
}
