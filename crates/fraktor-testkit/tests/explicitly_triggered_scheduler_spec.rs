// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file is the same as in v1.7.1.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1:
//   testkit/src/test/scala/org/apache/pekko/testkit/ExplicitlyTriggeredSchedulerSpec.scala
//
// Original test names are kept as comments next to each ported case.

use std::{
  sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
  },
  time::Duration,
};

use fraktor_testkit::manual_scheduler::ManualScheduler;

struct TestScope {
  scheduler: ManualScheduler,
  counter: Arc<AtomicUsize>,
  runnable: Arc<dyn Fn() + Send + Sync>,
}

impl TestScope {
  fn new() -> Self {
    let counter = Arc::new(AtomicUsize::new(0));
    let runnable = {
      let counter = Arc::clone(&counter);
      Arc::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
      }) as Arc<dyn Fn() + Send + Sync>
    };
    Self { scheduler: ManualScheduler::new(), counter, runnable }
  }

  fn count(&self) -> usize {
    self.counter.load(Ordering::SeqCst)
  }
}

// "ExplicitlyTriggeredScheduler" must { "execute a scheduled task" in { ... } }
#[test]
fn execute_a_scheduled_task() {
  let scope = TestScope::new();

  scope.scheduler.schedule(Duration::from_secs(0), Duration::from_secs(5), Arc::clone(&scope.runnable));

  scope.scheduler.time_passes(Duration::from_secs(12));

  assert_eq!(scope.count(), 3);
}

// "ExplicitlyTriggeredScheduler" must { "execute a scheduled task with initial delay" in { ... } }
#[test]
fn execute_a_scheduled_task_with_initial_delay() {
  let scope = TestScope::new();

  scope.scheduler.schedule(Duration::from_secs(5), Duration::from_secs(5), Arc::clone(&scope.runnable));

  scope.scheduler.time_passes(Duration::from_secs(12));

  assert_eq!(scope.count(), 2);
}

// "ExplicitlyTriggeredScheduler" must { "execute a scheduled task only once" in { ... } }
#[test]
fn execute_a_scheduled_task_only_once() {
  let scope = TestScope::new();

  scope.scheduler.schedule_once(Duration::from_secs(5), Arc::clone(&scope.runnable));

  scope.scheduler.time_passes(Duration::from_secs(100));

  assert_eq!(scope.count(), 1);
}

// "ExplicitlyTriggeredScheduler" must { "schedule multiple identical tasks" in { ... } }
#[test]
fn schedule_multiple_identical_tasks() {
  let scope = TestScope::new();

  scope.scheduler.schedule_once(Duration::from_secs(1), Arc::clone(&scope.runnable));
  scope.scheduler.schedule_once(Duration::from_secs(1), Arc::clone(&scope.runnable));
  scope.scheduler.schedule_once(Duration::from_secs(1), Arc::clone(&scope.runnable));

  scope.scheduler.time_passes(Duration::from_secs(2));

  assert_eq!(scope.count(), 3);
}

// "ExplicitlyTriggeredScheduler" must { "cancel scheduled task" in { ... } }
#[test]
fn cancel_scheduled_task() {
  let scope = TestScope::new();

  let task = scope.scheduler.schedule(Duration::from_secs(5), Duration::from_secs(5), Arc::clone(&scope.runnable));

  scope.scheduler.time_passes(Duration::from_secs(7));
  assert_eq!(scope.count(), 1);

  let cancellation_result = task.cancel();
  assert!(cancellation_result);

  scope.scheduler.time_passes(Duration::from_secs(4));
  assert_eq!(scope.count(), 1);
}

// "ExplicitlyTriggeredScheduler" must { "cancel one out of many scheduled tasks" in { ... } }
#[test]
fn cancel_one_out_of_many_scheduled_tasks() {
  let scope = TestScope::new();

  scope.scheduler.schedule(Duration::from_secs(5), Duration::from_secs(5), Arc::clone(&scope.runnable));
  let task = scope.scheduler.schedule(Duration::from_secs(5), Duration::from_secs(5), Arc::clone(&scope.runnable));
  scope.scheduler.schedule(Duration::from_secs(5), Duration::from_secs(5), Arc::clone(&scope.runnable));

  scope.scheduler.time_passes(Duration::from_secs(7));
  assert_eq!(scope.count(), 3);

  let cancellation_result = task.cancel();
  assert!(cancellation_result);

  scope.scheduler.time_passes(Duration::from_secs(4));
  assert_eq!(scope.count(), 5);
}

// "ExplicitlyTriggeredScheduler" must { "not execute task if cancelled immediately" in { ... } }
#[test]
fn not_execute_task_if_cancelled_immediately() {
  let scope = TestScope::new();

  let task = scope.scheduler.schedule(Duration::from_secs(5), Duration::from_secs(5), Arc::clone(&scope.runnable));
  task.cancel();

  scope.scheduler.time_passes(Duration::from_secs(7));

  assert_eq!(scope.count(), 0);
}

// "ExplicitlyTriggeredScheduler" must { "allow to move in time many times" in { ... } }
#[test]
fn allow_to_move_in_time_many_times() {
  let scope = TestScope::new();

  scope.scheduler.schedule(Duration::from_secs(5), Duration::from_secs(5), Arc::clone(&scope.runnable));

  scope.scheduler.time_passes(Duration::from_secs(7));
  assert_eq!(scope.count(), 1);

  scope.scheduler.time_passes(Duration::from_secs(1));
  assert_eq!(scope.count(), 1);

  scope.scheduler.time_passes(Duration::from_secs(10));
  assert_eq!(scope.count(), 3);
}
