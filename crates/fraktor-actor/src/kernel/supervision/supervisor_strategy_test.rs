//! Unit tests for [`SupervisorStrategy`](super::SupervisorStrategy).

use alloc::format;
use alloc::sync::Arc;

use crate::adaptor::ChannelBoundary;
use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::next_actor_ref;
use crate::kernel::supervision::Directive;
use crate::kernel::supervision::FailureKind;
use crate::kernel::supervision::SupervisorStrategy;

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
