// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/util/DurationSpec.scala
//
// The original has three cases that run. The fourth, "check its range", is
// inside a block comment in the original and never runs, so it is not counted
// (docs/design/issue-185-util-specs.md §1). Of the three:
//
// - "form a one-dimensional vector field" is ported as the first test below.
//   The type under test is `scala.concurrent.duration.FiniteDuration` in the
//   original and `core::time::Duration` in the port, which is the duration
//   type the whole fraktor-actor kernel uses. `two / one` in the original is
//   `two.div_duration_f64(one)` in the port: Rust has no
//   `Div<Duration> for Duration`.
// - "respect correct treatment of infinities" is not ported. It is recorded
//   in the ledger as an excluded case awaiting approval: `Duration` has no
//   infinite or undefined values, and fraktor represents unbounded time with
//   a per-use-site type instead (docs/design/issue-185-util-specs.md §7).
// - "support fromNow" is ported as the second test below. The original
//   builds deadlines with `2.seconds.fromNow` and `2.seconds(fromNow)`; the
//   two spellings are one construction in Rust. Issue #185 requires the port
//   to be deterministic without real time, so the deadline reads a manual
//   clock implementing the kernel's `Clock` port instead of sleeping. The
//   manual clock keeps its time in nanoseconds (the precedent clocks keep
//   seconds or milliseconds and cannot express one nanosecond), and the
//   test advances it by one second plus one nanosecond: advancing by
//   exactly one second would leave exactly one second, while the original's
//   remaining time is strictly below one second because its sleep lasts at
//   least one second and the check itself takes time. The one nanosecond
//   stands for that check time, so the direction of both bounds is the
//   original's.

use core::{
  sync::atomic::{AtomicU64, Ordering},
  time::Duration,
};

use fraktor_actor::kernel::Clock;

// A hand-driven monotonic clock that keeps its time in nanoseconds.
struct ManualClock {
  nanos: AtomicU64,
}

impl ManualClock {
  fn new() -> Self {
    Self { nanos: AtomicU64::new(0) }
  }

  fn advance(&self, duration: Duration) {
    self.nanos.fetch_add(duration.as_nanos() as u64, Ordering::SeqCst);
  }
}

impl Clock for ManualClock {
  fn monotonic(&self) -> Duration {
    Duration::from_nanos(self.nanos.load(Ordering::SeqCst))
  }
}

// The original's `deadline.timeLeft`: the time from the clock's current
// reading to the deadline, clamped at zero once the deadline has passed
// (the original's `timeLeft` can go negative; this case never reads it
// there).
fn time_left(deadline: Duration, clock: &ManualClock) -> Duration {
  deadline.saturating_sub(clock.monotonic())
}

// The original's `0 * one`: written with a named factor because clippy's
// `erasing_op` lint forbids the literal form.
const ZERO_FACTOR: u32 = 0;

#[test]
fn duration_forms_a_one_dimensional_vector_field() {
  let zero = Duration::ZERO;
  let one = Duration::from_secs(1);
  let two = one + one;
  let three = one * 3;
  assert_eq!(one * ZERO_FACTOR, zero);
  assert_eq!(one * 2, two);
  assert_eq!(three - two, one);
  assert_eq!(three / 3, one);
  assert_eq!(two.div_duration_f64(one), 2.0);
  assert_eq!(one + zero, one);
  assert_eq!(one / 1_000_000, Duration::from_micros(1));
}

#[test]
fn duration_from_now_time_left_decreases_when_a_manual_clock_advances() {
  let clock = ManualClock::new();
  // The original's `2.seconds.fromNow` and `2.seconds(fromNow)`.
  let deadline = clock.monotonic() + Duration::from_secs(2);
  let deadline2 = clock.monotonic() + Duration::from_secs(2);
  assert!(time_left(deadline, &clock) > Duration::from_secs(1));
  assert!(time_left(deadline2, &clock) > Duration::from_secs(1));
  // The original sleeps for one second here.
  clock.advance(Duration::from_secs(1) + Duration::from_nanos(1));
  assert!(time_left(deadline, &clock) < Duration::from_secs(1));
  assert!(time_left(deadline2, &clock) < Duration::from_secs(1));
}
