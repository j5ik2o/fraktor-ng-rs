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
