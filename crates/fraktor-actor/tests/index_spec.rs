// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/util/IndexSpec.scala
//
// The original's seven cases under "An Index must" are kept as one test each,
// named after the case, with the original name as a comment. The original runs
// on a PekkoSpec ActorSystem and uses the dispatcher for the parallel case;
// the port uses no ActorSystem, and the parallel case runs on a fixed number
// of std threads started together with a Barrier, with a deterministic
// SplitMix64 per thread in place of scala.util.Random (the ledger's `random`
// premise flag). The original's read-side condition (keys 5..10 are never
// removed, so their value sets are never empty) is kept as it is.

use std::{
  collections::BTreeSet,
  sync::{Arc, Barrier},
  thread,
};

use fraktor_actor::kernel::index::Index;

type StrIndex = Index<String, i32>;

fn empty_index() -> StrIndex {
  Index::new(100, |a: &i32, b: &i32| a.cmp(b))
}

fn index_with_values() -> StrIndex {
  let index = empty_index();
  index.put("s1".to_string(), 1);
  index.put("s1".to_string(), 2);
  index.put("s1".to_string(), 3);
  index.put("s2".to_string(), 1);
  index.put("s2".to_string(), 2);
  index.put("s3".to_string(), 2);
  index
}

fn values_of(index: &StrIndex, key: &str) -> BTreeSet<i32> {
  index.value_iterator(&key.to_string()).into_iter().collect()
}

// "take and return a value"
#[test]
fn take_and_return_a_value() {
  let index = empty_index();
  index.put("s1".to_string(), 1);
  assert_eq!(values_of(&index, "s1"), BTreeSet::from([1]));
}

// "take and return several values"
#[test]
fn take_and_return_several_values() {
  let index = empty_index();
  assert!(index.put("s1".to_string(), 1));
  assert!(!index.put("s1".to_string(), 1));
  index.put("s1".to_string(), 2);
  index.put("s1".to_string(), 3);
  index.put("s2".to_string(), 4);
  assert_eq!(values_of(&index, "s1"), BTreeSet::from([1, 2, 3]));
  assert_eq!(values_of(&index, "s2"), BTreeSet::from([4]));
}

// "remove values"
#[test]
fn remove_values() {
  let index = empty_index();
  index.put("s1".to_string(), 1);
  index.put("s1".to_string(), 2);
  index.put("s2".to_string(), 1);
  index.put("s2".to_string(), 2);
  // Remove value
  assert!(index.remove(&"s1".to_string(), &1));
  assert!(!index.remove(&"s1".to_string(), &1));
  assert_eq!(values_of(&index, "s1"), BTreeSet::from([2]));
  // Remove key
  match index.remove_key(&"s2".to_string()) {
    | Some(values) => assert_eq!(values.into_iter().collect::<BTreeSet<_>>(), BTreeSet::from([1, 2])),
    | None => panic!("removing an existing key must return its values"),
  }
  assert_eq!(index.remove_key(&"s2".to_string()), None);
  assert_eq!(values_of(&index, "s2"), BTreeSet::new());
}

// "remove the specified value"
#[test]
fn remove_the_specified_value() {
  let index = index_with_values();
  index.remove_value(&1);
  assert_eq!(values_of(&index, "s1"), BTreeSet::from([2, 3]));
  assert_eq!(values_of(&index, "s2"), BTreeSet::from([2]));
  assert_eq!(values_of(&index, "s3"), BTreeSet::from([2]));
}

// "apply a function for all key-value pairs and find every value"
#[test]
fn apply_a_function_for_all_pairs_and_find_every_value() {
  let index = index_with_values();
  let mut value_count = 0;
  index.for_each(|key, value| {
    value_count += 1;
    assert_eq!(index.find_value(key, |candidate| candidate == value), Some(*value));
  });
  assert_eq!(value_count, 6);
}

// "be cleared"
#[test]
fn be_cleared() {
  let index = index_with_values();
  assert!(!index.is_empty());
  index.clear();
  assert!(index.is_empty());
}

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

// "be able to be accessed in parallel"
#[test]
fn be_able_to_be_accessed_in_parallel() {
  let index: Arc<Index<i32, i32>> = Arc::new(Index::new(100, |a: &i32, b: &i32| a.cmp(b)));
  let nr_of_tasks = 10_000;
  let nr_of_keys = 10;
  let nr_of_values = 10;
  // Fill index
  for key in 0..nr_of_keys {
    for value in 0..nr_of_values {
      index.put(key, value);
    }
  }
  let nr_of_threads = 4;
  let barrier = Arc::new(Barrier::new(nr_of_threads));
  let mut handles = Vec::new();
  for thread_id in 0..nr_of_threads {
    let index = Arc::clone(&index);
    let barrier = Arc::clone(&barrier);
    handles.push(thread::spawn(move || {
      let mut random = SplitMix64(0x1870_0000 + thread_id as u64);
      barrier.wait();
      for _ in 0..(nr_of_tasks / nr_of_threads) {
        match random.below(4) {
          | 0 => {
            index.put(random.below(nr_of_keys as u64) as i32, random.below(nr_of_values as u64) as i32);
          },
          | 1 => {
            index.remove(&(random.below((nr_of_keys / 2) as u64) as i32), &(random.below(nr_of_values as u64) as i32));
          },
          | 2 => {
            index.remove_key(&(random.below((nr_of_keys / 2) as u64) as i32));
          },
          | _ => {
            let key = random.below(nr_of_keys as u64) as i32;
            let values = index.value_iterator(&key);
            if key >= nr_of_keys / 2 {
              assert!(!values.is_empty());
            }
          },
        }
      }
    }));
  }
  for handle in handles {
    handle.join().expect("a parallel task panicked");
  }
}
