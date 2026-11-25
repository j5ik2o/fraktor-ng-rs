// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file changes only by ab12a8f5ec #2766, 870d4499cc #2235; the cases and their inputs are
// unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1:
//   actor-tests/src/test/scala/org/apache/pekko/util/RecencyListSpec.scala
//
// The original's single case is kept as one test, with the original name as a
// comment. Every step from time 1 to time 20 runs in the original order.
//
// The original drives the list with `RecencyListSpec.TestClock`, a
// `RecencyList.Clock` that advances one second per `tick()` and answers
// `earlierTime(duration)` as `currentTime() - duration.toSeconds`. The port's
// clock is the kernel's monotonic `Clock` port, which reports a `Duration`
// instead of a unitless `Long`. The list subtracts the window from that
// `Duration` itself, so a test clock that reports whole seconds gives the same
// window boundaries as the original's `earlierTime`.
//
// Rust has no default arguments, so the original's `removeLeastRecent(n, skip)`
// and `removeMostRecent(n, skip)` overloads are the `_skipping` methods.
//
// The original's `check` starts with
// `expectedLeastToMostRecent.forall(recencyList.contains)` and discards the
// result, so that line asserts nothing. The port keeps it as an evaluation
// whose result is discarded, and does not turn it into an assertion.
//
// The test calls the production `RecencyList`. Until it exists the file does
// not compile: that compile error is the intended red for this node.

use core::{
  sync::atomic::{AtomicU64, Ordering},
  time::Duration,
};
use std::sync::Arc;

use fraktor_actor::kernel::{recency_list::RecencyList, Clock};

// The original's `RecencyListSpec.TestClock`: durations are always in seconds.
struct TestClock {
  time: AtomicU64,
}

impl TestClock {
  fn new() -> Self {
    Self { time: AtomicU64::new(0) }
  }

  fn tick(&self) {
    self.time.fetch_add(1, Ordering::SeqCst);
  }
}

impl Clock for TestClock {
  fn monotonic(&self) -> Duration {
    Duration::from_secs(self.time.load(Ordering::SeqCst))
  }
}

// The original's `check(recencyList, expectedLeastToMostRecent)`.
fn check(recency_list: &RecencyList<&'static str>, expected_least_to_most_recent: &[&'static str]) {
  // The original discards this result as well.
  let _discarded = expected_least_to_most_recent.iter().all(|value| recency_list.contains(value));
  assert_eq!(recency_list.len(), expected_least_to_most_recent.len());
  assert_eq!(recency_list.least_to_most_recent().copied().collect::<Vec<_>>(), expected_least_to_most_recent);
  let expected_most_to_least_recent: Vec<_> = expected_least_to_most_recent.iter().rev().copied().collect();
  assert_eq!(recency_list.most_to_least_recent().copied().collect::<Vec<_>>(), expected_most_to_least_recent);
}

// "RecencyList must track recency of elements"
#[test]
fn recency_list_must_track_recency_of_elements() {
  let clock = Arc::new(TestClock::new());
  let mut recency = RecencyList::new(clock.clone());

  check(&recency, &[]);

  clock.tick(); // time = 1
  recency.update("a");
  check(&recency, &["a"]);

  clock.tick(); // time = 2
  recency.update("b").update("c");
  check(&recency, &["a", "b", "c"]);

  clock.tick(); // time = 3
  recency.update("a").update("c");
  check(&recency, &["b", "a", "c"]);

  clock.tick(); // time = 4
  recency.update("d").update("e").update("f").update("g");
  check(&recency, &["b", "a", "c", "d", "e", "f", "g"]);

  clock.tick(); // time = 5
  recency.update("c").update("f");
  check(&recency, &["b", "a", "d", "e", "g", "c", "f"]);

  clock.tick(); // time = 6
  recency.remove(&"d").remove(&"g").remove(&"b").remove(&"f");
  check(&recency, &["a", "e", "c"]);

  clock.tick(); // time = 7
  recency.update("e").update("h").update("i").update("j");
  check(&recency, &["a", "c", "e", "h", "i", "j"]);

  clock.tick(); // time = 8
  assert_eq!(recency.remove_least_recent(3), vec!["a", "c", "e"]);
  check(&recency, &["h", "i", "j"]);

  clock.tick(); // time = 9
  recency.update("k").update("l").update("m").update("i");
  check(&recency, &["h", "j", "k", "l", "m", "i"]);

  clock.tick(); // time = 10
  assert_eq!(recency.remove_most_recent(3), vec!["i", "m", "l"]);
  check(&recency, &["h", "j", "k"]);

  clock.tick(); // time = 11
  recency.update("n").update("o");
  check(&recency, &["h", "j", "k", "n", "o"]);

  clock.tick(); // time = 12
  assert_eq!(recency.remove_least_recent_outside(Duration::from_secs(3)), vec!["h", "j"]);
  check(&recency, &["k", "n", "o"]);

  clock.tick(); // time = 13
  recency.update("p").update("q").update("k").update("r");
  check(&recency, &["n", "o", "p", "q", "k", "r"]);

  clock.tick(); // time = 14
  assert_eq!(recency.remove_most_recent_within(Duration::from_secs(3)), vec!["r", "k", "q", "p"]);
  check(&recency, &["n", "o"]);

  clock.tick(); // time = 15
  recency.update("s").update("t").update("u").update("v").update("w").update("x").update("y").update("z");
  check(&recency, &["n", "o", "s", "t", "u", "v", "w", "x", "y", "z"]);

  clock.tick(); // time = 16
  assert_eq!(recency.remove_least_recent_skipping(3, 3), vec!["t", "u", "v"]);
  check(&recency, &["n", "o", "s", "w", "x", "y", "z"]);

  clock.tick(); // time = 17
  assert_eq!(recency.remove_most_recent_skipping(3, 2), vec!["x", "w", "s"]);
  check(&recency, &["n", "o", "y", "z"]);

  clock.tick(); // time = 18
  assert_eq!(recency.remove_least_recent(10), vec!["n", "o", "y", "z"]);
  check(&recency, &[]);

  clock.tick(); // time = 19
  recency.update("a").update("b").update("c");
  check(&recency, &["a", "b", "c"]);

  clock.tick(); // time = 20
  assert_eq!(recency.remove_most_recent(10), vec!["c", "b", "a"]);
  check(&recency, &[]);
}
