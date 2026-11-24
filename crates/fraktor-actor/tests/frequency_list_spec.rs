// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/scala/org/apache/pekko/util/FrequencyListSpec.scala
//
// The original's three cases under "FrequencyList must" are kept as one test
// each, named after the case, with the original name as a comment. Every
// operation from each case runs in the original order, and each `check` and
// `checkRecency` stays where the original puts it.
//
// The second case drives the list with a controlled clock, the
// `RecencyListSpec.TestClock` the original uses there (its `FrequencyListSpec`
// has an identical `TestClock`; the original picks the `RecencyListSpec` one).
// That clock advances one second per `tick()` and answers `earlierTime(duration)`
// as `currentTime() - duration.toSeconds`. The port's clock is the kernel's
// monotonic `Clock` port, which reports a `Duration` instead of a unitless
// `Long`. The list subtracts the window from that `Duration` itself, so a test
// clock that reports whole seconds gives the same window boundaries as the
// original's `earlierTime`.
//
// The original's `FrequencyList.empty[A](dynamicAging = false)` matches the
// port's `FrequencyList::empty(dynamic_aging)`, and the original's
// `new FrequencyList[A](dynamicAging, OptionVal.Some(clock))` matches
// `FrequencyList::new(dynamic_aging, Option<Arc<dyn Clock>>)`, the same
// `Option<Arc<dyn Clock>>` shape `SegmentedRecencyList::new` takes.
//
// Rust has no default arguments, so the original's `removeLeastFrequent(n = 1)`
// (and the `removeMostFrequent(n = 1)` overloads) are the port's
// `remove_least_frequent(n)` / `remove_most_frequent(n)` with `1` passed
// explicitly. The original's `removeLeastFrequent(n, skip)` and
// `removeMostFrequent(n, skip)` overloads, where `skip` is a value rather than a
// count, are the port's `_skipping` methods taking the skipped value by
// reference; passing a count instead of a value is a type error.
//
// The original's `check` and `checkRecency` each start with
// `expected.forall(frequencyList.contains)` and discard the result, so that line
// asserts nothing. The port keeps it as an evaluation whose result is discarded,
// and does not turn it into an assertion: faithfulness to the original wins over
// turning a no-op into a stronger check here. `check(frequency, Nil)` therefore
// confirms that the size is 0 and that both frequency orders are empty, as the
// original's `check` does (its `forall` over an empty list does not call
// `contains`).
//
// The test calls the production `FrequencyList`. Until it exists the file does
// not compile: that compile error is the intended red for this node.

use core::{
  sync::atomic::{AtomicU64, Ordering},
  time::Duration,
};
use std::sync::Arc;

use fraktor_actor::kernel::{frequency_list::FrequencyList, Clock};

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

// The original's `check(frequencyList, expectedLeastToMostFrequent)`.
fn check(frequency: &FrequencyList<&'static str>, expected_least_to_most_frequent: &[&'static str]) {
  // The original discards this result as well.
  let _discarded = expected_least_to_most_frequent.iter().all(|value| frequency.contains(value));
  assert_eq!(frequency.len(), expected_least_to_most_frequent.len());
  assert_eq!(frequency.least_to_most_frequent().copied().collect::<Vec<_>>(), expected_least_to_most_frequent);
  let expected_most_to_least_frequent: Vec<_> = expected_least_to_most_frequent.iter().rev().copied().collect();
  assert_eq!(frequency.most_to_least_frequent().copied().collect::<Vec<_>>(), expected_most_to_least_frequent);
}

// The original's `checkRecency(frequencyList, expectedLeastToMostRecent)`.
fn check_recency(frequency: &FrequencyList<&'static str>, expected_least_to_most_recent: &[&'static str]) {
  // The original discards this result as well.
  let _discarded = expected_least_to_most_recent.iter().all(|value| frequency.contains(value));
  assert_eq!(frequency.len(), expected_least_to_most_recent.len());
  assert_eq!(frequency.overall_least_to_most_recent().copied().collect::<Vec<_>>(), expected_least_to_most_recent);
  let expected_most_to_least_recent: Vec<_> = expected_least_to_most_recent.iter().rev().copied().collect();
  assert_eq!(frequency.overall_most_to_least_recent().copied().collect::<Vec<_>>(), expected_most_to_least_recent);
}

// "FrequencyList must track frequency of elements"
#[test]
fn frequency_list_must_track_frequency_of_elements() {
  let mut frequency = FrequencyList::<&'static str>::empty(false);

  check(&frequency, &[]);

  frequency.update("a");
  check(&frequency, &[/* 1: */ "a"]);

  frequency.update("b").update("c");
  check(&frequency, &[/* 1: */ "a", "b", "c"]);

  frequency.update("a").update("c");
  check(&frequency, &[/* 1: */ "b", /* 2: */ "a", "c"]);

  frequency.update("d").update("e").update("f").update("g");
  check(&frequency, &[/* 1: */ "b", "d", "e", "f", "g", /* 2: */ "a", "c"]);

  frequency.update("c").update("f");
  check(&frequency, &[/* 1: */ "b", "d", "e", "g", /* 2: */ "a", "f", /* 3: */ "c"]);

  frequency.remove("d").remove("g").remove("b").remove("f");
  check(&frequency, &[/* 1: */ "e", /* 2: */ "a", /* 3: */ "c"]);

  frequency.update("e").update("h").update("i");
  check(&frequency, &[/* 1: */ "h", "i", /* 2: */ "a", "e", /* 3: */ "c"]);

  assert_eq!(frequency.remove_least_frequent(3), vec!["h", "i", "a"]);
  check(&frequency, &[/* 2: */ "e", /* 3: */ "c"]);

  frequency.update("j").update("k").update("l").update("m");
  check(&frequency, &[/* 1: */ "j", "k", "l", "m", /* 2: */ "e", /* 3: */ "c"]);

  assert_eq!(frequency.remove_least_frequent_skipping(1, &"j"), vec!["k"]);
  check(&frequency, &[/* 1: */ "j", "l", "m", /* 2: */ "e", /* 3: */ "c"]);

  assert_eq!(frequency.remove_least_frequent_skipping(2, &"l"), vec!["j", "m"]);
  check(&frequency, &[/* 1: */ "l", /* 2: */ "e", /* 3: */ "c"]);

  frequency.update("n").update("o").update("p").update("e").update("o").update("l");
  check(&frequency, &[/* 1: */ "n", "p", /* 2: */ "o", "l", /* 3: */ "c", "e"]);

  assert_eq!(frequency.remove_most_frequent(3), vec!["e", "c", "l"]);
  check(&frequency, &[/* 1: */ "n", "p", /* 2: */ "o"]);

  frequency.update("q").update("r").update("p").update("o").update("n");
  check(&frequency, &[/* 1: */ "q", "r", /* 2: */ "p", "n", /* 3: */ "o"]);

  assert_eq!(frequency.remove_most_frequent_skipping(1, &"o"), vec!["n"]);
  check(&frequency, &[/* 1: */ "q", "r", /* 2: */ "p", /* 3: */ "o"]);

  assert_eq!(frequency.remove_most_frequent_skipping(2, &"p"), vec!["o", "r"]);
  check(&frequency, &[/* 1: */ "q", /* 2: */ "p"]);
}

// "FrequencyList must track overall recency of elements when enabled"
#[test]
fn frequency_list_must_track_overall_recency_of_elements_when_enabled() {
  let clock = Arc::new(TestClock::new());
  let clock_port: Arc<dyn Clock> = clock.clone();
  let mut frequency = FrequencyList::<&'static str>::new(false, Some(clock_port));

  check(&frequency, &[]);

  clock.tick(); // time = 1
  frequency.update("a");
  check(&frequency, &[/* 1: */ "a"]);
  check_recency(&frequency, &["a"]);

  clock.tick(); // time = 2
  frequency.update("b").update("c");
  check(&frequency, &[/* 1: */ "a", "b", "c"]);
  check_recency(&frequency, &["a", "b", "c"]);

  clock.tick(); // time = 3
  frequency.update("a").update("c");
  check(&frequency, &[/* 1: */ "b", /* 2: */ "a", "c"]);
  check_recency(&frequency, &["b", "a", "c"]);

  clock.tick(); // time = 4
  frequency.update("d").update("e").update("f");
  check(&frequency, &[/* 1: */ "b", "d", "e", "f", /* 2: */ "a", "c"]);
  check_recency(&frequency, &["b", "a", "c", "d", "e", "f"]);

  clock.tick(); // time = 5
  frequency.update("c").update("f");
  check(&frequency, &[/* 1: */ "b", "d", "e", /* 2: */ "a", "f", /* 3: */ "c"]);
  check_recency(&frequency, &["b", "a", "d", "e", "c", "f"]);

  clock.tick(); // time = 6
  frequency.remove("d").remove("b").remove("f");
  check(&frequency, &[/* 1: */ "e", /* 2: */ "a", /* 3: */ "c"]);
  check_recency(&frequency, &["a", "e", "c"]);

  clock.tick(); // time = 7
  frequency.update("e").update("h").update("i");
  check(&frequency, &[/* 1: */ "h", "i", /* 2: */ "a", "e", /* 3: */ "c"]);
  check_recency(&frequency, &["a", "c", "e", "h", "i"]);

  clock.tick(); // time = 8
  assert_eq!(frequency.remove_overall_least_recent(1), vec!["a"]);
  check(&frequency, &[/* 1: */ "h", "i", /* 2: */ "e", /* 3: */ "c"]);
  check_recency(&frequency, &["c", "e", "h", "i"]);

  clock.tick(); // time = 9
  frequency.update("i").update("j").update("k");
  check(&frequency, &[/* 1: */ "h", "j", "k", /* 2: */ "e", "i", /* 3: */ "c"]);
  check_recency(&frequency, &["c", "e", "h", "i", "j", "k"]);

  clock.tick(); // time = 10
  assert_eq!(frequency.remove_overall_most_recent(1), vec!["k"]);
  check(&frequency, &[/* 1: */ "h", "j", /* 2: */ "e", "i", /* 3: */ "c"]);
  check_recency(&frequency, &["c", "e", "h", "i", "j"]);

  clock.tick(); // time = 11
  assert_eq!(frequency.remove_overall_least_recent_outside(Duration::from_secs(3)), vec!["c", "e", "h"]);
  check(&frequency, &[/* 1: */ "j", /* 2: */ "i"]);
  check_recency(&frequency, &["i", "j"]);

  clock.tick(); // time = 12
  frequency.update("l").update("m");
  check(&frequency, &[/* 1: */ "j", "l", "m", /* 2: */ "i"]);
  check_recency(&frequency, &["i", "j", "l", "m"]);

  clock.tick(); // time = 13
  assert_eq!(frequency.remove_overall_most_recent_within(Duration::from_secs(3)), vec!["m", "l"]);
  check(&frequency, &[/* 1: */ "j", /* 2: */ "i"]);
  check_recency(&frequency, &["i", "j"]);

  clock.tick(); // time = 14
  frequency.update("n").update("o").update("n");
  check(&frequency, &[/* 1: */ "j", "o", /* 2: */ "i", "n"]);
  check_recency(&frequency, &["i", "j", "o", "n"]);
}

// "FrequencyList must support dynamic aging with least frequently used eviction"
#[test]
fn frequency_list_must_support_dynamic_aging_with_least_frequently_used_eviction() {
  let mut regular = FrequencyList::<&'static str>::empty(false);
  let mut aging = FrequencyList::<&'static str>::empty(true);

  check(&regular, &[]);
  check(&aging, &[]);

  for _ in 1..=10 {
    regular.update("a").update("b").update("c");
  }
  check(&regular, &[/*10*/ "a", "b", "c"]);

  for _ in 1..=10 {
    aging.update("a").update("b").update("c");
  }
  check(&aging, &[/*10+0*/ "a", "b", "c"]);

  // age = 0

  regular.update("x").update("y").update("z");
  check(&regular, &[/*1*/ "x", "y", "z", /*10*/ "a", "b", "c"]);

  aging.update("x").update("y").update("z");
  check(&aging, &[/*1+0*/ "x", "y", "z", /*10+0*/ "a", "b", "c"]);

  assert_eq!(regular.remove_least_frequent(1), vec!["x"]);
  check(&regular, &[/*1*/ "y", "z", /*10*/ "a", "b", "c"]);

  assert_eq!(aging.remove_least_frequent(1), vec!["x"]);
  check(&aging, &[/*1+0*/ "y", "z", /*10+0*/ "a", "b", "c"]);

  // age = 1 (from last removal of "x")

  regular.update("x").update("y").update("z").update("z");
  check(&regular, &[/*1*/ "x", /*2*/ "y", /*3*/ "z", /*10*/ "a", "b", "c"]);

  aging.update("x").update("y").update("z").update("z");
  check(&aging, &[/*1+1*/ "x", /*2+1*/ "y", /*3+1*/ "z", /*10+0*/ "a", "b", "c"]);

  assert_eq!(regular.remove_least_frequent(2), vec!["x", "y"]);
  check(&regular, &[/*3*/ "z", /*10*/ "a", "b", "c"]);

  assert_eq!(aging.remove_least_frequent(2), vec!["x", "y"]);
  check(&aging, &[/*3+1*/ "z", /*10+0*/ "a", "b", "c"]);

  // age = 3 (from last removal of "y")

  regular.update("x").update("y").update("z");
  check(&regular, &[/*1*/ "x", "y", /*4*/ "z", /*10*/ "a", "b", "c"]);

  aging.update("x").update("y").update("z");
  check(&aging, &[/*1+3*/ "x", "y", /*4+3*/ "z", /*10+0*/ "a", "b", "c"]);

  assert_eq!(regular.remove_least_frequent(3), vec!["x", "y", "z"]);
  check(&regular, &[/*10*/ "a", "b", "c"]);

  assert_eq!(aging.remove_least_frequent(3), vec!["x", "y", "z"]);
  check(&aging, &[/*10+0*/ "a", "b", "c"]);

  // age = 7 (from last removal of "z")

  regular.update("x").update("y").update("y").update("z").update("z").update("z");
  check(&regular, &[/*1*/ "x", /*2*/ "y", /*3*/ "z", /*10*/ "a", "b", "c"]);

  aging.update("x").update("y").update("y").update("z").update("z").update("z");
  check(&aging, &[/*1+7*/ "x", /*2+7*/ "y", /*10+0*/ "a", "b", "c", /*3+7*/ "z"]);

  assert_eq!(regular.remove_least_frequent(2), vec!["x", "y"]);
  check(&regular, &[/*3*/ "z", /*10*/ "a", "b", "c"]);

  assert_eq!(aging.remove_least_frequent(2), vec!["x", "y"]);
  check(&aging, &[/*10+0*/ "a", "b", "c", /*3+7*/ "z"]);

  // age = 9 (from last removal of "y")

  regular.update("x").update("y").update("z");
  check(&regular, &[/*1*/ "x", "y", /*4*/ "z", /*10*/ "a", "b", "c"]);

  aging.update("x").update("y").update("z");
  check(&aging, &[/*10+0*/ "a", "b", "c", /*1+9*/ "x", "y", /*4+9*/ "z"]);

  assert_eq!(regular.remove_least_frequent(3), vec!["x", "y", "z"]);
  check(&regular, &[/*10*/ "a", "b", "c"]);

  assert_eq!(aging.remove_least_frequent(3), vec!["a", "b", "c"]);
  check(&aging, &[/*1+9*/ "x", "y", /*4+9*/ "z"]);

  // age = 10 (from last removal of "c")
}
