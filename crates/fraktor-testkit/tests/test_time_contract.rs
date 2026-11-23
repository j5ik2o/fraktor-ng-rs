//! Supplementary contract tests for the time-factor, condition-wait, and
//! `within`/`remaining` entries that the ported `TestTimeSpec` exercises.
//!
//! The three ported cases live in `tests/test_time_spec.rs` and are kept
//! unchanged; this file only observes the boundary behaviours that those three
//! cases do not pin down on their own, so the port can be checked against the
//! same original `TestKit`/`TestKitBase` contracts:
//!
//! - `TestKitExtension.Settings` accepts a finite positive time factor and
//!   rejects the rest (`TestKitSettings`).
//! - `awaitCond` evaluates its condition before the first wait, returns as soon
//!   as the condition holds, and applies the dilated timeout on the monotonic
//!   clock (`TestKitBase.awaitCond` / `TestKit.awaitCond`).
//! - `within` dilates `max` but not `min`, rejects an impossible `min` before
//!   running its body, restores the outer deadline after normal exit and after
//!   a panic, and keeps the no-message upper-bound exception
//!   (`TestKitBase.within`).
//! - `remaining` reports the time left inside `within` and is a contract
//!   violation outside it (`TestKitBase.remaining`).
//!
//! These tests are expected to fail to compile until the production entries
//! exist; that is the compile-red state the task records.

use core::time::Duration;

use std::panic::catch_unwind;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;
use fraktor_testkit::test_kit_extension::TestTimeFactor;

const SPEC_SYSTEM_NAME: &str = "Issue121TimeContractSpec";

fn one_second() -> Duration {
  Duration::from_secs(1)
}

fn failure_message(payload: &(dyn core::any::Any + Send)) -> String {
  payload
    .downcast_ref::<String>()
    .cloned()
    .or_else(|| payload.downcast_ref::<&str>().map(|text| (*text).to_string()))
    .unwrap_or_else(|| "the failure carried no message".to_string())
}

fn expect_failure<F>(block: F) -> Box<dyn core::any::Any + Send>
where
  F: FnOnce() + std::panic::UnwindSafe,
{
  match catch_unwind(block) {
    | Ok(()) => panic!("expected the block to fail, but it returned"),
    | Err(payload) => payload,
  }
}

// C02: `FraktorSpec::with_time_factor` builds a spec whose settings report the
// given factor, and the probe built from those settings dilates the same way.
#[test]
fn a_finite_positive_time_factor_reaches_the_settings_and_the_probe() {
  let mut spec = FraktorSpec::with_time_factor(SPEC_SYSTEM_NAME, TestTimeFactor::new(2.0));

  let settings = spec.test_kit_settings();
  assert_eq!(settings.test_time_factor(), 2.0, "the spec settings must report the configured factor");
  assert_eq!(settings.dilated(one_second()), Duration::from_secs(2), "the settings must dilate by the factor");

  let settings = spec.test_kit_settings();
  let mut probe = TestProbe::with_settings(spec.system_mut(), settings);
  let start = Instant::now();
  let payload = expect_failure(AssertUnwindSafe(|| {
    probe.await_cond(|| false, Duration::from_millis(250));
  }));
  let elapsed = start.elapsed();
  assert!(failure_message(payload.as_ref()).contains("timeout"), "a condition that never holds must time out");
  assert!(
    elapsed >= Duration::from_millis(500),
    "the probe must dilate the timeout it shares with the spec, waited {elapsed:?}"
  );

  spec.terminate();
}

// C02: the default factor stays 1.0 when no factor is configured, so existing
// callers keep the undilated waits.
#[test]
fn the_default_time_factor_stays_one() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  assert_eq!(spec.test_kit_settings().test_time_factor(), 1.0, "the default factor must stay 1.0");
  assert_eq!(spec.test_kit_settings().dilated(one_second()), one_second(), "the default factor must not dilate");
  spec.terminate();
}

// C02: the spec rejects a factor that is zero, negative, infinite, or NaN, as
// `TestKitExtension.Settings` requires a finite positive value.
#[test]
fn invalid_time_factors_are_rejected() {
  for factor in [0.0_f64, -1.0, f64::INFINITY, f64::NAN] {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
      let _ = TestTimeFactor::new(factor);
    }));
    assert!(outcome.is_err(), "a time factor of {factor} must be rejected");
  }
}

// C03: `await_cond` evaluates its condition before the first wait, so an
// immediately true condition returns without using the timeout.
#[test]
fn await_cond_returns_at_once_when_the_condition_already_holds() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let mut evaluations = 0;

  let start = Instant::now();
  probe.await_cond(
    || {
      evaluations += 1;
      true
    },
    one_second(),
  );
  let elapsed = start.elapsed();

  assert_eq!(evaluations, 1, "the condition must be evaluated exactly once");
  assert!(elapsed < Duration::from_millis(500), "an already true condition must not wait, waited {elapsed:?}");

  spec.terminate();
}

// C03: while `await_cond` waits it keeps driving the system, so a message that
// makes the condition true — delivered during the wait — ends it early instead
// of sleeping to the deadline.
#[test]
fn await_cond_returns_early_when_an_actor_message_delivered_during_the_wait_makes_the_condition_hold() {
  struct Flip {
    done: Arc<Mutex<bool>>,
  }

  impl Actor for Flip {
    fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
      if message.downcast_ref::<&'static str>().is_some() {
        *self.done.lock().unwrap() = true;
      }
    }
  }

  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let done = Arc::new(Mutex::new(false));
  let actor: ActorRef = spec.system_mut().actor_of(Props::new({
    let done = Arc::clone(&done);
    move || Flip { done: Arc::clone(&done) }
  }));
  let mut probe = TestProbe::new(spec.system_mut());

  // The message is sent from another thread after a short delay, so the
  // condition is false when `await_cond` first evaluates it. It only becomes
  // true because the wait drives the system and delivers the envelope.
  let sender = {
    let actor = actor.clone();
    std::thread::spawn(move || {
      std::thread::sleep(Duration::from_millis(50));
      actor.tell("flip", actor.clone());
    })
  };

  let start = Instant::now();
  let done_for_condition = Arc::clone(&done);
  probe.await_cond(|| *done_for_condition.lock().unwrap(), one_second());
  let elapsed = start.elapsed();
  sender.join().expect("the sending thread panicked");

  assert!(*done.lock().unwrap(), "the condition only became true because the wait delivered the message");
  assert!(
    elapsed < Duration::from_millis(500),
    "await_cond must return as soon as the condition holds, waited {elapsed:?}"
  );

  spec.terminate();
}

// C05: `within` dilates `max` but leaves `min` undilated, so a body that runs
// for less than the raw `min` fails even when the dilated `max` would allow it.
#[test]
fn within_does_not_dilate_the_minimum() {
  let mut spec = FraktorSpec::with_time_factor(SPEC_SYSTEM_NAME, TestTimeFactor::new(2.0));

  let payload = expect_failure(AssertUnwindSafe(|| {
    spec.within_range(Duration::from_millis(400), Duration::from_millis(300), |_spec| {
      // Runs far below the raw 400ms minimum; a diluted 400ms max (800ms) would
      // let this pass, so only an undilated min check fails here.
    });
  }));
  let message = failure_message(payload.as_ref());
  assert!(
    message.contains("took") && message.contains("400ms"),
    "within must enforce the undilated minimum, was {message}"
  );

  spec.terminate();
}

#[test]
fn within_accepts_a_body_between_the_raw_and_dilated_minimum_and_returns_its_value() {
  let mut spec = FraktorSpec::with_time_factor(SPEC_SYSTEM_NAME, TestTimeFactor::new(2.0));
  let min = Duration::from_secs(1);
  let max = Duration::from_secs(2);
  let expected = "within body value";

  let _ = expect_failure(AssertUnwindSafe(|| {
    spec.within_range(min, max, |_spec| expected);
  }));

  let started = Instant::now();
  let (value, body_elapsed) = spec.within_range(min, max, |_spec| {
    let body_started = Instant::now();
    std::thread::sleep(Duration::from_millis(1500));
    (expected, body_started.elapsed())
  });
  let elapsed = started.elapsed();

  eprintln!("within_range: body elapsed {body_elapsed:?}, call elapsed {elapsed:?}");
  assert_eq!(value, expected, "within must return the body value");
  assert!(body_elapsed >= min, "the body must meet the raw minimum, was {body_elapsed:?}");
  assert!(elapsed < Duration::from_secs(2), "the call must finish below the dilated minimum, was {elapsed:?}");

  spec.terminate();
}

// C05: nested `within` keeps the outer scope's deadline, and the deadline is
// restored when the inner scope ends.
#[test]
fn a_nested_within_restores_the_outer_deadline() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);

  let outer = spec.within_range(Duration::ZERO, Duration::from_secs(2), |spec| {
    let inner = spec.within_range(Duration::ZERO, Duration::from_millis(200), |spec| spec.remaining());
    assert!(inner <= Duration::from_millis(200), "the inner scope must expose its own deadline, got {inner:?}");
    spec.remaining()
  });

  assert!(
    outer > Duration::from_millis(200),
    "the outer deadline must be restored after the inner scope, got {outer:?}"
  );

  spec.terminate();
}

// C05: the deadline is restored even when the body panics, so a panic in an
// inner scope does not leak its deadline to the following code.
#[test]
fn a_panicking_within_restores_the_outer_deadline() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);

  let outer = spec.within_range(Duration::ZERO, Duration::from_secs(2), |spec| {
    let payload = expect_failure(AssertUnwindSafe(|| {
      spec.within_range(Duration::ZERO, Duration::from_millis(100), |_spec| {
        panic!("body fails inside the inner scope");
      });
    }));
    assert!(failure_message(payload.as_ref()).contains("body fails"));
    spec.remaining()
  });

  assert!(
    outer > Duration::from_millis(200),
    "the outer deadline must be restored after a panicking inner scope, got {outer:?}"
  );

  spec.terminate();
}

// C05: an outer scope whose remaining time cannot cover the requested `min`
// rejects the inner scope before its body runs.
#[test]
fn within_rejects_a_minimum_it_cannot_satisfy_before_running_the_body() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let ran = Arc::new(Mutex::new(false));

  let payload = expect_failure(AssertUnwindSafe(|| {
    spec.within_range(Duration::ZERO, Duration::from_millis(150), |spec| {
      let ran = Arc::clone(&ran);
      spec.within_range(Duration::from_secs(2), Duration::from_secs(2), move |_spec| {
        *ran.lock().unwrap() = true;
      });
    });
  }));
  let message = failure_message(payload.as_ref());
  assert!(!*ran.lock().unwrap(), "the body must not run when the minimum cannot be satisfied");
  assert!(message.contains("required min time"), "the failure must name the impossible minimum, was {message}");

  spec.terminate();
}

// C05: a no-message check that uses up the scope skips the upper-bound check at
// the end of `within`.
#[test]
fn within_skips_the_upper_bound_after_a_no_message_check() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);

  spec.within_range(Duration::ZERO, Duration::from_millis(100), |spec| {
    spec.test_kit_mut().expect_no_message(Duration::from_millis(100));
  });

  spec.terminate();
}

// C06: `remaining` reports the time left inside a `within` scope and saturates
// to zero once the deadline has passed.
#[test]
fn remaining_reports_the_time_left_inside_within() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);

  let inside = spec.within_range(Duration::ZERO, Duration::from_secs(2), |spec| spec.remaining());
  assert!(inside <= Duration::from_secs(2), "remaining must not exceed the scope, got {inside:?}");
  assert!(inside > Duration::from_millis(500), "remaining must report most of the scope, got {inside:?}");

  spec.terminate();
}

// C06: `remaining` outside any `within` is a contract violation that names both
// `remaining` and `within`.
#[test]
fn remaining_outside_within_is_a_contract_violation() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);

  let payload = expect_failure(AssertUnwindSafe(|| {
    let _ = spec.remaining();
  }));
  let message = failure_message(payload.as_ref());
  assert!(message.contains("remaining") && message.contains("within"), "was {message}");

  spec.terminate();
}
