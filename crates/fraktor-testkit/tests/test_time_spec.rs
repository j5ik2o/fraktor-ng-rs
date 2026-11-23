// Ported from Pekko v1.7.1 TestTimeSpec:
//   https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/testkit/src/test/scala/org/apache/pekko/testkit/TestTimeSpec.scala
//
// Cases (original line numbers):
//   - "correctly dilate times" (TimingTest, L24-34): time factor 2.0, an always
//     false awaitCond times out and the elapsed time is in [2s, 3s).
//   - "awaitAssert must throw correctly" (L36-43).
//   - "throw if `remaining` is called outside of `within`" (L45-49).
//
// Original test names are kept as comments next to each ported case. The
// original's TimingTest is neither excluded nor ignored.

use core::time::Duration;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Instant;

use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

const CUSTOM_SPEC_SYSTEM_NAME: &str = "PekkoCustomSpec";

// The original `PekkoSpec(Map("pekko.test.timefactor" -> 2.0))`: the spec is
// built with the test time factor 2.0, so every dilated duration is doubled.
fn test_time_factor() -> f64 {
  2.0
}

fn one_second() -> Duration {
  Duration::from_secs(1)
}

struct TestTimeSpecCase {
  fraktor_spec: FraktorSpec,
}

impl TestTimeSpecCase {
  fn new() -> Self {
    // The original passes the time factor to the spec constructor. The port
    // expresses the same call through the spec's time-factor entry point so
    // that `test_kit_settings().dilated` scales by 2.0.
    Self { fraktor_spec: FraktorSpec::with_time_factor(CUSTOM_SPEC_SYSTEM_NAME, test_time_factor()) }
  }

  fn test_probe(&mut self) -> TestProbe {
    // Share the spec's settings so the probe's waits use the same time factor
    // (2.0) that the spec was built with.
    let settings = self.fraktor_spec.test_kit_settings();
    TestProbe::with_settings(self.fraktor_spec.system_mut(), settings)
  }
}

impl Drop for TestTimeSpecCase {
  fn drop(&mut self) {
    self.fraktor_spec.terminate();
  }
}

// Reads the assertion message from a caught panic payload, mirroring how
// `intercept[AssertionError]` observes the failure in the original.
fn failure_message(payload: &(dyn core::any::Any + Send)) -> String {
  payload
    .downcast_ref::<String>()
    .cloned()
    .or_else(|| payload.downcast_ref::<&str>().map(|text| (*text).to_string()))
    .unwrap_or_else(|| "the failure carried no message".to_string())
}

// The original's assertion helper: a block that must fail. The caught payload
// is returned so the caller can check what kind of failure it was.
fn expect_failure<F>(block: F) -> Box<dyn core::any::Any + Send>
where
  F: FnOnce() + std::panic::UnwindSafe,
{
  match catch_unwind(block) {
    | Ok(()) => panic!("expected the block to fail, but it returned"),
    | Err(payload) => payload,
  }
}

// "A TestKit" must { "correctly dilate times" taggedAs TimingTest in { ... } }
#[test]
fn correctly_dilate_times() {
  let mut spec_case = TestTimeSpecCase::new();

  // assert(1.second.dilated.toNanos === 1000L * 1000L * 1000L * TestTimeFactor)
  let dilated = spec_case.fraktor_spec.test_kit_settings().dilated(one_second());
  assert_eq!(dilated, one_second().mul_f64(test_time_factor()), "1.second.dilated must equal 1e9 * TestTimeFactor");

  // val start = System.nanoTime
  // intercept[AssertionError] { probe.awaitCond(false, 1.second) }
  // val stop = System.nanoTime
  let mut probe = spec_case.test_probe();
  let start = Instant::now();
  let payload = expect_failure(AssertUnwindSafe(|| {
    probe.await_cond(|| false, one_second());
  }));
  let elapsed = start.elapsed();

  // The timeout must be an assertion failure, not an unrelated panic.
  assert!(
    failure_message(payload.as_ref()).contains("timeout"),
    "awaitCond on an always false condition must fail with a timeout assertion"
  );

  // assert((stop - start) >= 1000L * 1000L * 1000L * 2)
  // assert((stop - start) < 1000L * 1000L * 1000L * 3)
  assert!(elapsed >= Duration::from_secs(2), "the dilated wait must last at least two seconds, was {elapsed:?}");
  assert!(elapsed < Duration::from_secs(3), "the dilated wait must last less than three seconds, was {elapsed:?}");
}

// "A TestKit" must { "awaitAssert must throw correctly" in { ... } }
#[test]
fn await_assert_must_throw_correctly() {
  let mut spec_case = TestTimeSpecCase::new();

  // awaitAssert("foo" should ===("foo"))
  // The default single-expect-default timeout is read from the settings (the raw
  // value); `await_assert` itself applies the time factor, so it is not dilated
  // here.
  let max = spec_case.fraktor_spec.test_kit_settings().single_expect_default_timeout();
  let value = spec_case.fraktor_spec.await_assert(max, Duration::from_millis(100), || {
    let actual = "foo";
    assert_eq!(actual, "foo");
    actual
  });
  assert_eq!(value, "foo", "a passing awaitAssert returns the assertion value");

  // within(300.millis, 2.seconds) {
  //   intercept[TestFailedException] {
  //     awaitAssert("foo" should ===("bar"), 500.millis, 300.millis)
  //   }
  // }
  spec_case.fraktor_spec.within_range(Duration::from_millis(300), Duration::from_secs(2), |spec| {
    let payload = expect_failure(AssertUnwindSafe(|| {
      spec.await_assert(Duration::from_millis(500), Duration::from_millis(300), || {
        let actual = "foo";
        let expected = "bar";
        assert_eq!(actual, expected);
      });
    }));
    let message = failure_message(payload.as_ref());
    // The failure must be the final assertion on foo/bar, not the enclosing
    // `within` scope exceeding its budget.
    assert!(
      message.contains("foo") && message.contains("bar"),
      "awaitAssert must rethrow the final foo/bar assertion failure, was {message}"
    );
    assert!(
      !message.contains("block took"),
      "the failure must not be the within scope exceeding its budget, was {message}"
    );
  });
}

// "A TestKit" must { "throw if `remaining` is called outside of `within`" in { ... } }
#[test]
fn throw_if_remaining_is_called_outside_of_within() {
  let spec_case = TestTimeSpecCase::new();

  // intercept[AssertionError] { remaining }
  let payload = expect_failure(AssertUnwindSafe(|| {
    let _ = spec_case.fraktor_spec.remaining();
  }));
  let message = failure_message(payload.as_ref());
  assert!(
    message.contains("remaining") && message.contains("within"),
    "remaining outside of within must fail with the contract-violation assertion, was {message}"
  );
}
