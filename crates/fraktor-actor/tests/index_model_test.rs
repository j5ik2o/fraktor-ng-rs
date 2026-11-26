// Model-based property tests and deterministic concurrency tests for Index.
//
// The model is a BTreeMap<i32, BTreeSet<i32>>; the comparator orders by value,
// so each key's set is the sorted value list. A fixed-seed SplitMix64 drives
// 400 random operation sequences of 120 operations each, and after every
// operation the port and the model are compared through the public API
// (value_iterator, values, keys, find_value, is_empty). Two deterministic
// concurrency tests pin the lock discipline: overlapping puts from several
// threads, and re-entrant calls from snapshot closures, the latter guarded by
// a watchdog thread so a deadlock fails instead of hanging.

use std::{
  collections::{BTreeMap, BTreeSet},
  sync::{
    Arc, Barrier,
    mpsc::{channel, RecvTimeoutError},
  },
  thread,
  time::Duration,
};

use fraktor_actor::kernel::index::Index;

struct SplitMix64(u64);

impl SplitMix64 {
  fn next_u64(&mut self) -> u64 {
    self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.0;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }

  fn below(&mut self, bound: u64) -> u64 {
    self.next_u64() % bound
  }
}

fn assert_matches_model(index: &Index<i32, i32>, model: &BTreeMap<i32, BTreeSet<i32>>) {
  assert_eq!(index.is_empty(), model.is_empty());
  let mut expected_keys: Vec<i32> = model.keys().copied().collect();
  expected_keys.sort_unstable();
  assert_eq!(index.keys(), expected_keys);
  let mut all_values: BTreeSet<i32> = BTreeSet::new();
  for values in model.values() {
    all_values.extend(values.iter().copied());
  }
  assert_eq!(index.values().into_iter().collect::<BTreeSet<_>>(), all_values);
  for key in 0..4 {
    let expected: Vec<i32> = model.get(&key).map_or_else(Vec::new, |set| set.iter().copied().collect());
    assert_eq!(index.value_iterator(&key), expected);
    for probe in 0..4 {
      let found = index.find_value(&key, |value| *value == probe);
      let expected_found =
        model.get(&key).and_then(|set| if set.contains(&probe) { Some(probe) } else { None });
      assert_eq!(found, expected_found);
    }
  }
  let mut pairs: BTreeSet<(i32, i32)> = BTreeSet::new();
  index.for_each(|key, value| {
    pairs.insert((*key, *value));
  });
  let mut expected_pairs: BTreeSet<(i32, i32)> = BTreeSet::new();
  for (key, values) in model {
    for value in values {
      expected_pairs.insert((*key, *value));
    }
  }
  assert_eq!(pairs, expected_pairs);
}

#[test]
fn index_matches_the_model_over_random_operation_sequences() {
  for seed in 0..400u64 {
    let index: Index<i32, i32> = Index::new(8, |a: &i32, b: &i32| a.cmp(b));
    let mut model: BTreeMap<i32, BTreeSet<i32>> = BTreeMap::new();
    let mut random = SplitMix64(0x1871_0000 + seed);
    for _ in 0..120 {
      let key = random.below(4) as i32;
      let value = random.below(4) as i32;
      match random.below(7) {
        | 0 | 1 => {
          let added = index.put(key, value);
          let inserted = model.entry(key).or_default().insert(value);
          assert_eq!(added, inserted);
        },
        | 2 => {
          let removed = index.remove(&key, &value);
          let existed = model.get_mut(&key).map_or(false, |set| set.remove(&value));
          if model.get(&key).map_or(false, |set| set.is_empty()) {
            model.remove(&key);
          }
          assert_eq!(removed, existed);
        },
        | 3 => {
          let taken = index.remove_key(&key);
          let expected = model.remove(&key).map(|set| set.into_iter().collect::<Vec<_>>());
          assert_eq!(taken, expected);
        },
        | 4 => {
          index.remove_value(&value);
          for set in model.values_mut() {
            set.remove(&value);
          }
          model.retain(|_, set| !set.is_empty());
        },
        | 5 => {
          index.clear();
          model.clear();
        },
        | _ => {},
      }
      assert_matches_model(&index, &model);
    }
  }
}

#[test]
fn concurrent_puts_of_overlapping_values_agree_on_the_total_count() {
  let index: Arc<Index<i32, i32>> = Arc::new(Index::new(16, |a: &i32, b: &i32| a.cmp(b)));
  let nr_of_threads = 4;
  let barrier = Arc::new(Barrier::new(nr_of_threads));
  let mut handles = Vec::new();
  for _ in 0..nr_of_threads {
    let index = Arc::clone(&index);
    let barrier = Arc::clone(&barrier);
    handles.push(thread::spawn(move || {
      barrier.wait();
      let mut added = 0usize;
      for value in 0..50 {
        if index.put(7, value) {
          added += 1;
        }
      }
      added
    }));
  }
  let total_added: usize = handles.into_iter().map(|handle| handle.join().expect("a worker panicked")).sum();
  assert_eq!(total_added, 50);
  assert_eq!(index.value_iterator(&7), (0..50).collect::<Vec<i32>>());
}

#[test]
fn reentrant_calls_from_snapshot_closures_neither_deadlock_nor_change_the_snapshot() {
  let run = thread::spawn(|| {
    let index: Index<i32, i32> = Index::new(16, |a: &i32, b: &i32| a.cmp(b));
    index.put(1, 10);
    index.put(1, 20);
    index.put(2, 30);
    let mut seen: Vec<(i32, i32)> = Vec::new();
    index.for_each(|key, value| {
      seen.push((*key, *value));
      index.put(9, 99);
      index.remove(&1, &10);
    });
    assert_eq!(seen, vec![(1, 10), (1, 20), (2, 30)]);
    let found = index.find_value(&1, |value| {
      index.put(8, 88);
      *value == 20
    });
    assert_eq!(found, Some(20));
  });
  let (sender, receiver) = channel();
  thread::spawn(move || {
    let result = run.join();
    let _ = sender.send(result);
  });
  match receiver.recv_timeout(Duration::from_secs(30)) {
    | Ok(Ok(())) => {},
    | Ok(Err(payload)) => std::panic::resume_unwind(payload),
    | Err(RecvTimeoutError::Timeout) => panic!("the re-entrant test did not finish within 30 seconds"),
    | Err(RecvTimeoutError::Disconnected) => panic!("the watchdog thread ended without a result"),
  }
}
