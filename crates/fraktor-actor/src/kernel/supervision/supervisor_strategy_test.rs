//! Unit tests for [`SupervisorStrategy`](super::SupervisorStrategy).

use alloc::format;
use alloc::sync::Arc;

use crate::adaptor::ChannelBoundary;
use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::next_actor_ref;
use crate::kernel::supervision::Directive;
use crate::kernel::supervision::FailureKind;
use crate::kernel::supervision::RestartWindow;
use crate::kernel::supervision::SupervisorStrategy;

use core::time::Duration;

/// Builds a handle the decider tests pass in place of a failing child.
fn child_ref(name: &str) -> ActorRef {
  let path = ActorPath::from_string(&format!("pekko://test/user/{name}")).expect("the test path is valid");
  next_actor_ref(path, Arc::new(ChannelBoundary::new()))
}

#[test]
fn one_for_one_restarts_while_the_retries_last_and_stops_after() {
  let strategy = SupervisorStrategy::one_for_one(3);

  assert_eq!(strategy.decide(0, FailureKind::MessageHandling), Directive::Restart);
  assert_eq!(strategy.decide(1, FailureKind::MessageHandling), Directive::Restart);
  assert_eq!(strategy.decide(2, FailureKind::MessageHandling), Directive::Restart);
  assert_eq!(strategy.decide(3, FailureKind::MessageHandling), Directive::Stop);
}

#[test]
fn one_for_one_stops_when_the_maximum_is_zero() {
  let strategy = SupervisorStrategy::one_for_one(0);

  assert_eq!(strategy.decide(0, FailureKind::MessageHandling), Directive::Stop);
}

#[test]
fn one_for_one_stops_an_initialization_failure_by_default() {
  let strategy = SupervisorStrategy::one_for_one(3);

  assert_eq!(strategy.decide(0, FailureKind::Initialization), Directive::Stop);
}

#[test]
fn one_for_one_restarting_initialization_failures_restarts_a_failed_start() {
  let strategy = SupervisorStrategy::one_for_one_restarting_initialization_failures(3);

  assert_eq!(strategy.decide(0, FailureKind::Initialization), Directive::Restart);
  assert_eq!(strategy.decide(2, FailureKind::Initialization), Directive::Restart);
  assert_eq!(strategy.decide(3, FailureKind::Initialization), Directive::Stop);
}

#[test]
fn one_for_one_restarts_a_kill_while_the_retries_last() {
  let strategy = SupervisorStrategy::one_for_one(3);

  assert_eq!(strategy.decide(0, FailureKind::Killed), Directive::Restart);
  assert_eq!(strategy.decide(2, FailureKind::Killed), Directive::Restart);
  assert_eq!(strategy.decide(3, FailureKind::Killed), Directive::Stop);
}

#[test]
fn default_strategy_restarts_a_message_failure_without_a_limit() {
  let strategy = SupervisorStrategy::default_strategy();

  assert_eq!(strategy.decide(0, FailureKind::MessageHandling), Directive::Restart);
  assert_eq!(strategy.decide(1000, FailureKind::MessageHandling), Directive::Restart);
}

#[test]
fn default_strategy_stops_an_initialization_failure() {
  let strategy = SupervisorStrategy::default_strategy();

  assert_eq!(strategy.decide(0, FailureKind::Initialization), Directive::Stop);
}

#[test]
fn default_strategy_stops_a_kill() {
  let strategy = SupervisorStrategy::default_strategy();

  assert_eq!(strategy.decide(0, FailureKind::Killed), Directive::Stop);
}

#[test]
fn stopping_strategy_stops() {
  let strategy = SupervisorStrategy::stopping_strategy();

  assert_eq!(strategy.decide(0, FailureKind::MessageHandling), Directive::Stop);
}

#[test]
fn a_decider_sees_the_failing_child() {
  let child = child_ref("child");
  let strategy = SupervisorStrategy::one_for_one(3).with_decider(|failing, _failure| {
    if failing.path().name() == "child" {
      Directive::Stop
    } else {
      Directive::Restart
    }
  });

  assert_eq!(strategy.decide_for_child(&child, 0, FailureKind::MessageHandling), Directive::Stop);
}

#[test]
fn a_decider_decides_an_initialization_failure() {
  let child = child_ref("child");
  let strategy = SupervisorStrategy::one_for_one(3).with_decider(|_failing, failure| {
    if failure == FailureKind::Initialization {
      Directive::Restart
    } else {
      Directive::Stop
    }
  });

  assert_eq!(strategy.decide_for_child(&child, 0, FailureKind::Initialization), Directive::Restart);
}

#[test]
fn one_for_one_stops_a_decider_restart_after_the_retry_limit() {
  let child = child_ref("child");
  let strategy = SupervisorStrategy::one_for_one(1).with_decider(|_failing, _failure| Directive::Restart);

  assert_eq!(strategy.decide_for_child(&child, 0, FailureKind::MessageHandling), Directive::Restart);
  assert_eq!(strategy.decide_for_child(&child, 1, FailureKind::MessageHandling), Directive::Stop);
}

#[test]
fn restart_always_keeps_a_decider_restart_without_a_limit() {
  let child = child_ref("child");
  let strategy = SupervisorStrategy::default_strategy().with_decider(|_failing, _failure| Directive::Restart);

  assert_eq!(strategy.decide_for_child(&child, 1000, FailureKind::MessageHandling), Directive::Restart);
}

#[test]
#[allow(dropping_copy_types)] // The same regression test must run before Copy is removed.
fn dropping_a_strategy_releases_its_decider_capture() {
  let captured = Arc::new(Directive::Stop);
  let weak = Arc::downgrade(&captured);
  let strategy = SupervisorStrategy::one_for_one(3).with_decider(move |_, _| *captured);
  assert!(weak.upgrade().is_some(), "the decider must retain its capture while the strategy lives");

  drop(strategy);

  assert!(weak.upgrade().is_none(), "dropping the only strategy must release the decider capture");
}

#[test]
#[allow(dropping_copy_types, clippy::clone_on_copy)] // Also runs against the leaking Copy implementation.
fn a_cloned_strategy_retains_the_decider_capture_until_the_last_drop() {
  let captured = Arc::new(Directive::Stop);
  let weak = Arc::downgrade(&captured);
  let strategy = SupervisorStrategy::one_for_one(3).with_decider(move |_, _| *captured);
  let cloned = strategy.clone();
  assert!(weak.upgrade().is_some(), "the strategies must retain the decider capture");

  drop(strategy);
  assert!(weak.upgrade().is_some(), "the remaining clone must retain the decider capture");

  drop(cloned);
  assert!(weak.upgrade().is_none(), "dropping the last clone must release the decider capture");
}

#[test]
#[allow(dropping_copy_types)] // The same regression test must run before Copy is removed.
fn dropping_a_strategy_releases_its_child_terminated_capture() {
  let captured = Arc::new(());
  let weak = Arc::downgrade(&captured);
  let strategy = SupervisorStrategy::one_for_one(3).with_child_terminated(move |_, _, _| {
    core::hint::black_box(&captured);
  });
  assert!(weak.upgrade().is_some(), "the hook must retain its capture while the strategy lives");

  drop(strategy);

  assert!(weak.upgrade().is_none(), "dropping the only strategy must release the child-terminated capture");
}

#[test]
#[allow(dropping_copy_types, clippy::clone_on_copy)] // Also runs against the leaking Copy implementation.
fn a_cloned_strategy_retains_the_child_terminated_capture_until_the_last_drop() {
  let captured = Arc::new(());
  let weak = Arc::downgrade(&captured);
  let strategy = SupervisorStrategy::one_for_one(3).with_child_terminated(move |_, _, _| {
    core::hint::black_box(&captured);
  });
  let cloned = strategy.clone();
  assert!(weak.upgrade().is_some(), "the strategies must retain the child-terminated capture");

  drop(strategy);
  assert!(weak.upgrade().is_some(), "the remaining clone must retain the child-terminated capture");

  drop(cloned);
  assert!(weak.upgrade().is_none(), "dropping the last clone must release the child-terminated capture");
}

#[test]
#[allow(clippy::clone_on_copy)] // Exercises Clone both before and after Copy is removed.
fn a_cloned_strategy_preserves_the_decider_and_retry_limit() {
  let child = child_ref("child");
  let strategy = SupervisorStrategy::one_for_one(1).with_decider(|_, failure| match failure {
    | FailureKind::Initialization => Directive::Stop,
    | FailureKind::MessageHandling | FailureKind::Killed => Directive::Restart,
  });
  let cloned = strategy.clone();

  for (failure, count, expected) in [
    (FailureKind::Initialization, 0, Directive::Stop),
    (FailureKind::MessageHandling, 0, Directive::Restart),
    (FailureKind::MessageHandling, 1, Directive::Stop),
    (FailureKind::Killed, 0, Directive::Restart),
  ] {
    assert_eq!(strategy.decide_for_child(&child, count, failure), expected);
    assert_eq!(cloned.decide_for_child(&child, count, failure), expected);
  }
}

#[test]
fn one_for_one_without_a_maximum_and_no_window_restarts_without_a_limit() {
  let strategy = SupervisorStrategy::one_for_one_with_window(None, RestartWindow::Unbounded);

  assert_eq!(strategy.decide(0, FailureKind::MessageHandling), Directive::Restart);
  assert_eq!(strategy.decide(1_000, FailureKind::MessageHandling), Directive::Restart);
  assert!(strategy.restart_permission().is_none(), "no window uses the directive decision, not the history");
}

#[test]
fn one_for_one_without_a_maximum_still_stops_an_initialization_failure() {
  let strategy = SupervisorStrategy::one_for_one_with_window(None, RestartWindow::Unbounded);

  assert_eq!(strategy.decide(0, FailureKind::Initialization), Directive::Stop);
}

#[test]
fn one_for_one_with_a_finite_window_defers_the_limit_to_the_history() {
  let child = child_ref("child");
  let strategy =
    SupervisorStrategy::one_for_one_with_window(Some(2), RestartWindow::Within(Duration::from_millis(500)))
      .with_decider(|_, _| Directive::Restart);

  // A finite window must not pre-empt the restart from the cumulative count;
  // the history is asked after the directive, so a high count still restarts.
  assert_eq!(strategy.decide_for_child(&child, 5, FailureKind::MessageHandling), Directive::Restart);
  assert_eq!(strategy.decide(5, FailureKind::MessageHandling), Directive::Restart);
}

#[test]
fn one_for_one_finite_window_reports_its_limit_and_window() {
  let window = Duration::from_secs(1);
  let finite = SupervisorStrategy::one_for_one_with_window(Some(2), RestartWindow::Within(window));
  let finite_permission = finite.restart_permission().expect("a finite window is checked against the history");
  assert!(finite_permission.0.permits(2));
  assert!(!finite_permission.0.permits(3));
  assert_eq!(finite_permission.1, Some(window));

  // An omitted maximum with a finite window resolves to one restart per window.
  let unspecified = SupervisorStrategy::one_for_one_with_window(None, RestartWindow::Within(window));
  let unspecified_permission =
    unspecified.restart_permission().expect("a finite window is checked against the history");
  assert!(unspecified_permission.0.permits(1));
  assert!(!unspecified_permission.0.permits(2), "the omitted maximum defaults to one inside the window");
  assert_eq!(unspecified_permission.1, Some(window));
}
