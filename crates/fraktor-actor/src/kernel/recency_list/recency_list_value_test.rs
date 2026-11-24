//! Unit tests for [`RecencyList`](super::RecencyList).

use alloc::{sync::Arc, vec, vec::Vec};
use core::{
  sync::atomic::{AtomicU64, Ordering},
  time::Duration,
};

use super::RecencyList;
use crate::kernel::Clock;

struct SecondsClock(AtomicU64);

impl Clock for SecondsClock {
  fn monotonic(&self) -> Duration {
    Duration::from_secs(self.0.load(Ordering::SeqCst))
  }
}

fn list_at(seconds: u64) -> (Arc<SecondsClock>, RecencyList<&'static str>) {
  let clock = Arc::new(SecondsClock(AtomicU64::new(seconds)));
  let list = RecencyList::new(clock.clone());
  (clock, list)
}

#[test]
fn a_window_reaching_before_the_epoch_keeps_every_value_outside_it() {
  let (_clock, mut list) = list_at(1);
  list.update("a").update("b");

  assert_eq!(list.remove_least_recent_outside(Duration::from_secs(5)), Vec::<&str>::new());
  assert_eq!(list.least_to_most_recent().copied().collect::<Vec<_>>(), vec!["a", "b"]);
}

#[test]
fn a_window_reaching_before_the_epoch_removes_every_value_within_it() {
  let (clock, mut list) = list_at(0);
  list.update("a");
  clock.0.store(1, Ordering::SeqCst);
  list.update("b");

  assert_eq!(list.remove_most_recent_within(Duration::from_secs(5)), vec!["b", "a"]);
  assert!(list.is_empty());
}

#[test]
fn removing_a_value_the_list_does_not_hold_changes_nothing() {
  let (_clock, mut list) = list_at(0);
  list.update("a");

  list.remove(&"missing");

  assert_eq!(list.len(), 1);
  assert!(list.contains(&"a"));
  assert!(!list.contains(&"missing"));
}

// A value that the list orders and looks up by `id` alone.
#[derive(Clone, Debug)]
struct Keyed {
  id: u32,
  label: &'static str,
}

impl PartialEq for Keyed {
  fn eq(&self, other: &Self) -> bool {
    self.id == other.id
  }
}

impl Eq for Keyed {}

impl PartialOrd for Keyed {
  fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for Keyed {
  fn cmp(&self, other: &Self) -> core::cmp::Ordering {
    self.id.cmp(&other.id)
  }
}

#[test]
fn updating_a_held_value_keeps_the_value_first_added() {
  let clock = Arc::new(SecondsClock(AtomicU64::new(0)));
  let mut list = RecencyList::new(clock.clone());
  list.update(Keyed { id: 7, label: "old" }).update(Keyed { id: 8, label: "other" });
  clock.0.store(1, Ordering::SeqCst);

  list.update(Keyed { id: 7, label: "new" });

  let labels: Vec<_> = list.least_to_most_recent().map(|keyed| keyed.label).collect();
  assert_eq!(labels, vec!["other", "old"]);
  let removed: Vec<_> = list.remove_most_recent(1).into_iter().map(|keyed| keyed.label).collect();
  assert_eq!(removed, vec!["old"]);
}
