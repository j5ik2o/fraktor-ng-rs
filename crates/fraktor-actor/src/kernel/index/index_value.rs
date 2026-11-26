//! A concurrent multi-map from keys to comparator-ordered sets of values.

use alloc::{boxed::Box, collections::BTreeMap, vec::Vec};
use core::cmp::Ordering;

use spin::Mutex;

#[cfg(test)]
#[path = "index_value_test.rs"]
mod tests;

/// A thread-safe multi-map from keys to sets of values.
///
/// Mirrors `org.apache.pekko.util.Index` (marked `@InternalApi` in Pekko),
/// which Pekko's `ConcurrentMultiMap` type alias names. Each key maps to a
/// set of values ordered by the comparator given to [`new`](Self::new); two
/// values are the same set member when the comparator calls them equal, which
/// need not agree with `Eq`.
///
/// The whole map sits behind a single lock. Every public method takes the
/// lock at most once and never calls user code while holding it: the methods
/// that iterate or search ([`value_iterator`](Self::value_iterator),
/// [`values`](Self::values), [`keys`](Self::keys), [`for_each`](Self::for_each)
/// and [`find_value`](Self::find_value)) first copy a snapshot under the lock
/// and then work on the snapshot after releasing it, so a closure may call
/// back into the same `Index` without deadlocking, and it observes the
/// snapshot, not later writes. Values and keys taken out by `put`,
/// `remove`, `remove_key`, `remove_value` and `clear` are likewise dropped
/// only after the lock is released, so a value's `Drop` may also call back
/// into the index. The comparator is the one exception: it runs
/// under the lock, inside `put`, `remove` and `remove_value`, so **a
/// comparator must not call back into the `Index` it belongs to**.
///
/// `Index` is not `Clone`: share one instance between threads through an
/// `Arc`. The `map_size` given to [`new`](Self::new) is only a capacity hint,
/// as in the original; it is not observable.
pub struct Index<K, V> {
  values: Mutex<BTreeMap<K, Vec<V>>>,
  comparator: Box<Comparator<V>>,
  map_size: usize,
}

/// The comparator an Index orders and deduplicates its values by.
type Comparator<V> = dyn Fn(&V, &V) -> Ordering + Send + Sync;

/// The name Pekko's type alias gives this multi-map.
pub type ConcurrentMultiMap<K, V> = Index<K, V>;

impl<K, V> Index<K, V> {
  /// Creates an empty index.
  ///
  /// `map_size` is a capacity hint, as in the original; `comparator` orders
  /// each key's values and decides when two values are the same set member.
  pub fn new<F>(map_size: usize, comparator: F) -> Self
  where
    F: Fn(&V, &V) -> Ordering + Send + Sync + 'static,
  {
    Self { values: Mutex::new(BTreeMap::new()), comparator: Box::new(comparator), map_size }
  }

  /// Returns the capacity hint this index was created with.
  #[must_use]
  pub const fn map_size_hint(&self) -> usize {
    self.map_size
  }
}

impl<K: Ord, V> Index<K, V> {
  /// Adds `value` to the set of `key` and returns whether it was new.
  ///
  /// The comparator runs under the lock; it must not call back into this
  /// index.
  pub fn put(&self, key: K, value: V) -> bool {
    let mut duplicate = None;
    let added = {
      let mut guard = self.values.lock();
      let values = guard.entry(key).or_default();
      match values.binary_search_by(|candidate| (self.comparator)(candidate, &value)) {
        | Ok(_) => {
          duplicate = Some(value);
          false
        },
        | Err(position) => {
          values.insert(position, value);
          true
        },
      }
    };
    // A rejected duplicate is dropped after the lock is released, like the
    // values the remove methods take out.
    drop(duplicate);
    added
  }

  /// Removes `value` from the set of `key` and returns whether it was there.
  ///
  /// A key whose set becomes empty is dropped, as in the original. The
  /// comparator runs under the lock; it must not call back into this index.
  pub fn remove(&self, key: &K, value: &V) -> bool {
    let taken = {
      let mut guard = self.values.lock();
      let Some(values) = guard.get_mut(key) else {
        return false;
      };
      match values.binary_search_by(|candidate| (self.comparator)(candidate, value)) {
        | Ok(position) => {
          let removed = values.remove(position);
          let entry = if values.is_empty() { guard.extract_if(.., |k, _| k == key).next() } else { None };
          Some((removed, entry))
        },
        | Err(_) => None,
      }
    };
    match taken {
      | Some((removed, entry)) => {
        // The removed value, and the key and values of an emptied entry,
        // are dropped after the lock is released: a value's `Drop` may call
        // back into this index.
        drop(removed);
        drop(entry);
        true
      },
      | None => false,
    }
  }

  /// Removes `key` with all its values and returns them in comparator order,
  /// or `None` when the key was not present.
  ///
  /// The original's `remove(key)` returns the removed values as an
  /// `Option[Set[V]]`; the port returns them as a sorted `Vec`.
  pub fn remove_key(&self, key: &K) -> Option<Vec<V>> {
    // The entry is taken out under the lock and the stored key is dropped
    // with it after the lock is released.
    let entry = {
      let mut guard = self.values.lock();
      guard.extract_if(.., |k, _| k == key).next()
    };
    entry.map(|(_, values)| values)
  }

  /// Removes `value` from the set of every key.
  ///
  /// Keys whose sets become empty are dropped, as in the original. The
  /// comparator runs under the lock; it must not call back into this index.
  pub fn remove_value(&self, value: &V) {
    let (removed, emptied) = {
      let mut guard = self.values.lock();
      let mut removed = Vec::new();
      for values in guard.values_mut() {
        if let Ok(position) = values.binary_search_by(|candidate| (self.comparator)(candidate, value)) {
          removed.push(values.remove(position));
        }
      }
      let emptied: Vec<(K, Vec<V>)> = guard.extract_if(.., |_, values| values.is_empty()).collect();
      (removed, emptied)
    };
    // Removed values and emptied entries are dropped after the lock is
    // released, as in `remove`.
    drop(removed);
    drop(emptied);
  }

  /// Removes every key and value.
  ///
  /// The taken map is dropped after the lock is released, as in `remove`.
  pub fn clear(&self) {
    let taken = {
      let mut guard = self.values.lock();
      core::mem::take(&mut *guard)
    };
    drop(taken);
  }

  /// Returns whether the index holds no key.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.values.lock().is_empty()
  }
}

impl<K: Ord, V: Clone> Index<K, V> {
  /// Returns a snapshot of the values of `key`, in comparator order.
  ///
  /// The original returns a live iterator over the weakly consistent
  /// `ConcurrentHashMap` view; the port returns an owned snapshot taken under
  /// the lock, so later writes do not appear in it.
  #[must_use]
  pub fn value_iterator(&self, key: &K) -> Vec<V> {
    self.values.lock().get(key).cloned().unwrap_or_default()
  }

  /// Returns every value in the index, deduplicated by comparator equality.
  ///
  /// The original collects the values into a `Set` by `equals`; the port has
  /// no `Eq` bound, so it deduplicates by the comparator instead. When the
  /// comparator agrees with `Eq` the results agree; when it does not, a value
  /// that is comparator-equal to an earlier one is dropped even if `Eq` would
  /// keep both. Values come out grouped by key in key order, each group in
  /// comparator order.
  #[must_use]
  pub fn values(&self) -> Vec<V> {
    let snapshot: Vec<Vec<V>> = self.values.lock().values().cloned().collect();
    let mut out: Vec<V> = Vec::new();
    for values in snapshot {
      for value in values {
        if !out.iter().any(|existing| (self.comparator)(existing, &value) == Ordering::Equal) {
          out.push(value);
        }
      }
    }
    out
  }

  /// Returns the first value of `key`, in comparator order, for which `f`
  /// returns `true`, or `None`.
  ///
  /// The values are snapshotted under the lock and `f` runs after the lock is
  /// released, so `f` may call back into this index; it observes the
  /// snapshot, not later writes.
  pub fn find_value<F>(&self, key: &K, mut f: F) -> Option<V>
  where
    F: FnMut(&V) -> bool,
  {
    let snapshot = self.value_iterator(key);
    snapshot.into_iter().find(|value| f(value))
  }
}

impl<K: Ord + Clone, V> Index<K, V> {
  /// Returns a snapshot of the keys, in key order.
  ///
  /// The original returns the keys as a `Set`; the port returns them as a
  /// `Vec`.
  #[must_use]
  pub fn keys(&self) -> Vec<K> {
    self.values.lock().keys().cloned().collect()
  }
}

impl<K: Ord + Clone, V: Clone> Index<K, V> {
  /// Calls `f` once for every key-value pair of a snapshot of the index.
  ///
  /// The snapshot is taken under the lock and `f` runs after the lock is
  /// released, so `f` may call back into this index; it observes the
  /// snapshot, not later writes. Pairs come out grouped by key in key order,
  /// each group in comparator order; the original's order is unspecified.
  pub fn for_each<F>(&self, mut f: F)
  where
    F: FnMut(&K, &V),
  {
    let snapshot: Vec<(K, V)> = {
      let guard = self.values.lock();
      guard.iter().flat_map(|(key, values)| values.iter().map(move |value| (key.clone(), value.clone()))).collect()
    };
    for (key, value) in &snapshot {
      f(key, value);
    }
  }
}
