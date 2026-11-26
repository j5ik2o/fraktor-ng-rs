//! Supplementary unit tests for [`Index`](super::Index).
//!
//! These pin behaviours the ported spec does not exercise: comparator
//! equality (not `Eq`) decides set membership and the deduplication of
//! `values`, and `remove_key` on a missing key returns `None`.

use alloc::{string::String, vec::Vec};

use super::Index;

#[test]
fn membership_uses_the_comparator_not_eq() {
  // The comparator compares absolute values, so 1 and -1 are the same member.
  let index: Index<String, i32> = Index::new(4, |a: &i32, b: &i32| a.abs().cmp(&b.abs()));
  assert!(index.put("k".into(), 1));
  assert!(!index.put("k".into(), -1));
  assert_eq!(index.value_iterator(&"k".into()), Vec::from([1]));
  assert!(index.remove(&"k".into(), &-1));
  assert!(index.is_empty());
}

#[test]
fn values_deduplicates_by_the_comparator() {
  let index: Index<String, i32> = Index::new(4, |a: &i32, b: &i32| a.abs().cmp(&b.abs()));
  index.put("a".into(), 1);
  index.put("b".into(), -1);
  index.put("b".into(), 2);
  // -1 is comparator-equal to the earlier 1, so it is dropped.
  assert_eq!(index.values(), Vec::from([1, 2]));
}

#[test]
fn remove_key_on_a_missing_key_returns_none() {
  let index: Index<String, i32> = Index::new(4, |a: &i32, b: &i32| a.cmp(b));
  assert_eq!(index.remove_key(&"missing".into()), None);
  index.put("k".into(), 1);
  assert_eq!(index.remove_key(&"missing".into()), None);
  assert_eq!(index.remove_key(&"k".into()), Some(Vec::from([1])));
}

#[test]
fn snapshots_do_not_observe_later_writes() {
  let index: Index<String, i32> = Index::new(4, |a: &i32, b: &i32| a.cmp(b));
  index.put("k".into(), 1);
  let snapshot = index.value_iterator(&"k".into());
  index.put("k".into(), 2);
  assert_eq!(snapshot, Vec::from([1]));
  assert_eq!(index.value_iterator(&"k".into()), Vec::from([1, 2]));
}
