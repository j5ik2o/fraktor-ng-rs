//! Unit tests for [`SegmentedRecencyList`](super::SegmentedRecencyList).

use alloc::{sync::Arc, vec, vec::Vec};
use core::{
  sync::atomic::{AtomicU64, Ordering},
  time::Duration,
};

use super::SegmentedRecencyList;
use crate::kernel::Clock;

struct SecondsClock(AtomicU64);

impl Clock for SecondsClock {
  fn monotonic(&self) -> Duration {
    Duration::from_secs(self.0.load(Ordering::SeqCst))
  }
}

fn levels<A: Ord + Clone>(list: &SegmentedRecencyList<A>, count: usize) -> Vec<Vec<A>> {
  (0..count).map(|level| list.least_to_most_recent_of(level).cloned().collect()).collect()
}

#[test]
#[should_panic(expected = "overall recency is not enabled")]
fn removing_by_overall_recency_without_a_clock_panics() {
  let mut list = SegmentedRecencyList::<&str>::empty(&[1, 1]);
  list.update("a");

  list.remove_overall_least_recent_outside(Duration::from_secs(1));
}

#[test]
fn a_window_reaching_before_the_epoch_keeps_every_value() {
  let clock: Arc<dyn Clock> = Arc::new(SecondsClock(AtomicU64::new(1)));
  let mut list = SegmentedRecencyList::new(&[2, 2], Some(clock));
  list.update("a").update("b").update("b");

  assert_eq!(list.remove_overall_least_recent_outside(Duration::from_secs(5)), Vec::<&str>::new());
  assert_eq!(levels(&list, 2), vec![vec!["a"], vec!["b"]]);
}

#[test]
fn updating_a_value_in_the_highest_segment_moves_it_to_the_most_recent_end() {
  let mut list = SegmentedRecencyList::empty(&[2, 2]);
  list.update("a").update("b").update("a").update("b");

  list.update("a");

  assert_eq!(levels(&list, 2), vec![vec![], vec!["b", "a"]]);
}

#[test]
#[should_panic(expected = "one limit per segment")]
fn updating_limits_with_another_segment_count_panics() {
  let mut list = SegmentedRecencyList::<&str>::empty(&[1, 1]);

  list.update_limits(&[1, 1, 1]);
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
  let clock_port: Arc<dyn Clock> = clock.clone();
  let mut list = SegmentedRecencyList::new(&[1, 1], Some(clock_port));
  list.update(Keyed { id: 7, label: "old" });
  clock.0.store(2, Ordering::SeqCst);

  list.update(Keyed { id: 7, label: "new" });

  let promoted: Vec<_> = list.least_to_most_recent_of(1).map(|keyed| keyed.label).collect();
  assert_eq!(promoted, vec!["old"]);
  let removed: Vec<_> =
    list.remove_overall_least_recent_outside(Duration::from_secs(0)).into_iter().map(|keyed| keyed.label).collect();
  assert_eq!(removed, Vec::<&str>::new());
  clock.0.store(3, Ordering::SeqCst);
  let removed: Vec<_> =
    list.remove_overall_least_recent_outside(Duration::from_secs(0)).into_iter().map(|keyed| keyed.label).collect();
  assert_eq!(removed, vec!["old"]);
}
