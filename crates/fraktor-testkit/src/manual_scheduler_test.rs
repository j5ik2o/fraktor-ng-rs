//! Unit tests for [`ManualScheduler`](super::ManualScheduler) that cover
//! regressions the eight ported cases do not observe.
//!
//! The ported spec fixes the reference scenario. These tests add only what the
//! reference scenario leaves unobserved on its own: the immediate run at a zero
//! initial delay before any advance, the deadline boundary (equal runs, earlier
//! does not), the cancel result for a registration that already ran or was
//! cancelled twice, the refusal of a zero interval and of deadline overflow,
//! advance ownership during unwind and re-entry, execution on the calling
//! thread, and new registrations made by callbacks. Each case exists because the ported cases would still pass with
//! the wrong behaviour, and no case duplicates a ported scenario.

use std::panic::catch_unwind;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use super::ManualScheduler;
use super::ManualSchedulerTask;

fn counter() -> (Arc<AtomicUsize>, Arc<dyn Fn() + Send + Sync>) {
  let counter = Arc::new(AtomicUsize::new(0));
  let runnable = {
    let counter = Arc::clone(&counter);
    Arc::new(move || {
      counter.fetch_add(1, Ordering::SeqCst);
    }) as Arc<dyn Fn() + Send + Sync>
  };
  (counter, runnable)
}

fn count(counter: &AtomicUsize) -> usize {
  counter.load(Ordering::SeqCst)
}

fn failure_text(failure: Box<dyn core::any::Any + Send>) -> String {
  match failure.downcast::<String>() {
    | Ok(text) => *text,
    | Err(failure) => failure.downcast::<&'static str>().map(|text| text.to_string()).unwrap_or_default(),
  }
}

#[test]
fn schedule_runs_immediately_when_the_initial_delay_is_zero() {
  let (counter, runnable) = counter();
  let scheduler = ManualScheduler::new();
  let threads = Arc::new(Mutex::new(Vec::new()));
  let recorded = Arc::clone(&threads);

  scheduler.schedule(
    Duration::ZERO,
    Duration::from_secs(5),
    Arc::new(move || {
      runnable();
      recorded.lock().unwrap().push(thread::current().id());
    }),
  );

  assert_eq!(count(&counter), 1, "a zero initial delay must run during schedule, before any time passes");
  assert_eq!(*threads.lock().unwrap(), vec![thread::current().id()]);
}

#[test]
fn time_passes_runs_a_task_whose_deadline_equals_the_new_time() {
  let (counter, runnable) = counter();
  let scheduler = ManualScheduler::new();
  let threads = Arc::new(Mutex::new(Vec::new()));
  let recorded = Arc::clone(&threads);
  scheduler.schedule(
    Duration::from_secs(5),
    Duration::from_secs(5),
    Arc::new(move || {
      runnable();
      recorded.lock().unwrap().push(thread::current().id());
    }),
  );
  assert_eq!(count(&counter), 0);
  assert!(threads.lock().unwrap().is_empty());

  scheduler.time_passes(Duration::from_secs(5));

  assert_eq!(count(&counter), 1, "a deadline equal to the new time must run");
  assert_eq!(*threads.lock().unwrap(), vec![thread::current().id()]);
}

#[test]
fn time_passes_does_not_run_a_task_before_its_deadline() {
  let (counter, runnable) = counter();
  let scheduler = ManualScheduler::new();
  scheduler.schedule(Duration::from_secs(5), Duration::from_secs(5), runnable);

  scheduler.time_passes(Duration::from_millis(4_999));

  assert_eq!(count(&counter), 0, "a deadline one millisecond later must not run yet");
}

#[test]
fn delayed_registration_does_not_run_another_due_callback_before_cancellation() {
  let (count_b, runnable_b) = counter();
  let scheduler = Arc::new(ManualScheduler::new());
  let weak = Arc::downgrade(&scheduler);
  let sibling = Arc::new(Mutex::new(None::<ManualSchedulerTask>));
  let pending = Arc::clone(&sibling);
  scheduler.schedule_once(
    Duration::from_secs(1),
    Arc::new(move || {
      let scheduler = weak.upgrade().unwrap();
      scheduler.schedule_once(Duration::from_secs(1), Arc::new(|| {}));
      scheduler.schedule(Duration::from_secs(1), Duration::from_secs(1), Arc::new(|| {}));
      assert!(pending.lock().unwrap().take().unwrap().cancel());
    }),
  );
  *sibling.lock().unwrap() = Some(scheduler.schedule_once(Duration::from_secs(1), runnable_b));

  scheduler.time_passes(Duration::from_secs(1));

  assert_eq!(count(&count_b), 0, "a registration cancelled by an earlier callback in the same advance must not run");
}

#[test]
fn cancelling_a_callback_can_drop_a_value_that_cancels_another_registration() {
  struct CancelOnDrop(ManualSchedulerTask);

  impl Drop for CancelOnDrop {
    fn drop(&mut self) {
      assert!(self.0.cancel());
    }
  }

  let (count, runnable) = counter();
  let scheduler = ManualScheduler::new();
  let cleanup = CancelOnDrop(scheduler.schedule_once(Duration::from_secs(1), runnable));
  let task = scheduler.schedule_once(
    Duration::from_secs(1),
    Arc::new(move || {
      let _cleanup = &cleanup;
    }),
  );

  assert!(task.cancel());
  scheduler.time_passes(Duration::from_secs(1));
  assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[test]
fn cancel_returns_true_only_for_a_registration_that_is_still_pending() {
  let (counter, runnable) = counter();
  let scheduler = ManualScheduler::new();
  let task = scheduler.schedule(Duration::from_secs(5), Duration::from_secs(5), runnable);

  assert!(task.cancel(), "cancelling a pending registration must report success");
  assert!(!task.cancel(), "cancelling the same registration twice must report failure");

  scheduler.time_passes(Duration::from_secs(7));
  assert_eq!(count(&counter), 0);
}

#[test]
fn a_repeating_callback_can_cancel_its_own_future_runs() {
  let (count, runnable) = counter();
  let scheduler = ManualScheduler::new();
  let handle = Arc::new(Mutex::new(None::<ManualSchedulerTask>));
  let own_handle = Arc::clone(&handle);
  *handle.lock().unwrap() = Some(scheduler.schedule(
    Duration::from_secs(1),
    Duration::from_secs(1),
    Arc::new(move || {
      runnable();
      assert!(own_handle.lock().unwrap().take().unwrap().cancel());
    }),
  ));

  scheduler.time_passes(Duration::from_secs(3));
  assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[test]
fn schedule_rejects_a_zero_interval_without_registering_anything() {
  let (counter, runnable) = counter();
  let scheduler = ManualScheduler::new();
  let first = scheduler.schedule(Duration::from_secs(5), Duration::from_secs(5), Arc::clone(&runnable));

  let failure = catch_unwind(AssertUnwindSafe(|| {
    scheduler.schedule(Duration::from_secs(0), Duration::ZERO, Arc::clone(&runnable));
  }))
  .expect_err("schedule accepted a zero interval");
  assert!(!failure_text(failure).is_empty());

  assert!(first.cancel(), "the rejection must not disturb the pending registration");
  scheduler.time_passes(Duration::from_secs(5));
  assert_eq!(count(&counter), 0);
}

#[test]
fn schedule_rejects_when_the_first_deadline_overflows() {
  let (counter, runnable) = counter();
  let scheduler = ManualScheduler::new();
  let near_end = Duration::MAX - Duration::from_secs(5);
  scheduler.schedule_once(near_end, Arc::new(|| {}));
  scheduler.time_passes(near_end);

  let failure = catch_unwind(AssertUnwindSafe(|| {
    scheduler.schedule(Duration::from_secs(10), Duration::from_secs(1), Arc::clone(&runnable));
  }))
  .expect_err("schedule accepted an overflowing first deadline");
  assert!(!failure_text(failure).is_empty());

  scheduler.time_passes(Duration::from_secs(1));
  assert_eq!(count(&counter), 0, "a rejected registration must never run");
}

#[test]
fn immediate_repeating_registration_checks_its_next_deadline_before_insertion() {
  let (count, runnable) = counter();
  let scheduler = ManualScheduler::new();
  scheduler.time_passes(Duration::MAX - Duration::from_secs(1));

  let result = catch_unwind(AssertUnwindSafe(|| {
    scheduler.schedule(Duration::ZERO, Duration::from_secs(2), runnable);
  }));

  assert!(result.is_err());
  scheduler.time_passes(Duration::from_secs(1));
  assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[test]
fn time_passes_rejects_an_advance_that_overflows_before_running_any_task() {
  let (counter, runnable) = counter();
  let scheduler = ManualScheduler::new();
  let near_end = Duration::MAX - Duration::from_secs(5);
  scheduler.schedule_once(near_end, Arc::new(|| {}));
  scheduler.time_passes(near_end);
  scheduler.schedule(Duration::from_secs(1), Duration::from_secs(1), runnable);

  let failure = catch_unwind(AssertUnwindSafe(|| {
    scheduler.time_passes(Duration::from_secs(10));
  }))
  .expect_err("time_passes accepted an overflowing advance");
  assert!(!failure_text(failure).is_empty());

  assert_eq!(count(&counter), 0, "an overflowing advance must run nothing before it refuses");
  scheduler.time_passes(Duration::from_millis(999));
  assert_eq!(count(&counter), 0, "the earlier deadline must still be pending after the refusal");
}

#[test]
fn time_passes_rejects_when_a_repeating_task_cannot_be_rescheduled() {
  for advance in [4, 5] {
    let (counter, runnable) = counter();
    let scheduler = ManualScheduler::new();
    let near_end = Duration::MAX - Duration::from_secs(10);
    scheduler.time_passes(near_end);
    let task = scheduler.schedule(Duration::from_secs(5), Duration::from_secs(20), runnable);
    let before = scheduler.shared.next_due_at_or_before(Duration::MAX).unwrap();

    let result = catch_unwind(AssertUnwindSafe(|| scheduler.time_passes(Duration::from_secs(advance))));
    assert_eq!(result.is_err(), advance == 5);
    assert_eq!(count(&counter), 0, "the refusal must happen before the task runs");
    let expected_now = if advance == 4 { near_end + Duration::from_secs(4) } else { near_end };
    assert_eq!(scheduler.shared.now(), expected_now);
    let after = scheduler.shared.next_due_at_or_before(Duration::MAX).unwrap();
    assert_eq!(after.id(), before.id());
    assert_eq!(after.deadline(), before.deadline());
    assert_eq!(after.interval(), before.interval());
    assert!(task.cancel());
    scheduler.time_passes(Duration::from_secs(1));
    assert_eq!(scheduler.shared.now(), expected_now + Duration::from_secs(1));
  }
}

#[test]
fn time_passes_validates_later_repetitions_before_any_execution() {
  for advance in [6, 10] {
    let (counter, runnable) = counter();
    let scheduler = ManualScheduler::new();
    let start = Duration::MAX - Duration::from_secs(10);
    scheduler.time_passes(start);
    let task = scheduler.schedule(Duration::from_secs(1), Duration::from_secs(6), runnable);
    let before = scheduler.shared.next_due_at_or_before(Duration::MAX).unwrap();

    let result = catch_unwind(AssertUnwindSafe(|| scheduler.time_passes(Duration::from_secs(advance))));
    let after = scheduler.shared.next_due_at_or_before(Duration::MAX).unwrap();
    if advance == 6 {
      assert!(result.is_ok());
      assert_eq!(count(&counter), 1);
      assert_eq!(scheduler.shared.now(), start + Duration::from_secs(6));
      assert_eq!(after.deadline(), start + Duration::from_secs(7));
    } else {
      assert!(result.is_err());
      assert_eq!(count(&counter), 0);
      assert_eq!(scheduler.shared.now(), start);
      assert_eq!(after.deadline(), before.deadline());
    }
    assert_eq!(after.id(), before.id());
    assert_eq!(after.interval(), before.interval());
    assert!(task.cancel());
  }
}

#[test]
fn time_passes_releases_its_advance_when_a_callback_panics() {
  let scheduler = ManualScheduler::new();
  let task = scheduler.schedule(Duration::from_secs(1), Duration::from_secs(1), Arc::new(|| panic!("callback")));

  assert!(catch_unwind(AssertUnwindSafe(|| scheduler.time_passes(Duration::from_secs(3)))).is_err());

  assert!(!task.cancel(), "a panicking repeating callback must not be rescheduled");
  scheduler.time_passes(Duration::from_secs(1));
  assert_eq!(scheduler.shared.now(), Duration::from_secs(2));
}

#[test]
fn a_rejected_nested_advance_does_not_release_the_outer_advance() {
  for delay in [Duration::ZERO, Duration::from_secs(1)] {
    let scheduler = Arc::new(ManualScheduler::new());
    let weak = Arc::downgrade(&scheduler);
    scheduler.schedule_once(
      delay,
      Arc::new(move || {
        let scheduler = weak.upgrade().unwrap();
        for _ in 0..2 {
          assert!(catch_unwind(AssertUnwindSafe(|| scheduler.time_passes(Duration::ZERO))).is_err());
        }
      }),
    );

    scheduler.time_passes(Duration::from_secs(3));
    assert_eq!(scheduler.shared.now(), Duration::from_secs(3));
  }
}

#[test]
fn a_callback_can_register_a_child_within_or_beyond_the_current_target() {
  for child_delay in [1, 3] {
    let scheduler = Arc::new(ManualScheduler::new());
    let weak = Arc::downgrade(&scheduler);
    let executions = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&executions);
    scheduler.schedule_once(
      Duration::from_secs(1),
      Arc::new(move || {
        let scheduler = weak.upgrade().unwrap();
        assert_eq!(scheduler.shared.now(), Duration::from_secs(1));
        let weak_child = Arc::downgrade(&scheduler);
        let recorded = Arc::clone(&recorded);
        scheduler.schedule_once(
          Duration::from_secs(child_delay),
          Arc::new(move || {
            recorded.lock().unwrap().push(weak_child.upgrade().unwrap().shared.now());
          }),
        );
      }),
    );

    scheduler.time_passes(Duration::from_secs(3));

    assert_eq!(scheduler.shared.now(), Duration::from_secs(3));
    if child_delay == 1 {
      assert_eq!(*executions.lock().unwrap(), vec![Duration::from_secs(2)]);
    } else {
      assert!(executions.lock().unwrap().is_empty());
      assert_eq!(scheduler.shared.next_due_at_or_before(Duration::MAX).unwrap().deadline(), Duration::from_secs(4));
      scheduler.time_passes(Duration::from_secs(1));
      assert_eq!(*executions.lock().unwrap(), vec![Duration::from_secs(4)]);
    }
  }
}
