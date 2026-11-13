//! Unit tests for [`SupervisorStrategy`](super::SupervisorStrategy).

use crate::kernel::supervision::Directive;
use crate::kernel::supervision::SupervisorStrategy;

#[test]
fn one_for_one_restarts_while_the_retries_last_and_stops_after() {
  let strategy = SupervisorStrategy::one_for_one(3);

  assert_eq!(strategy.decide(0, false), Directive::Restart);
  assert_eq!(strategy.decide(1, false), Directive::Restart);
  assert_eq!(strategy.decide(2, false), Directive::Restart);
  assert_eq!(strategy.decide(3, false), Directive::Stop);
}

#[test]
fn one_for_one_stops_when_the_maximum_is_zero() {
  let strategy = SupervisorStrategy::one_for_one(0);

  assert_eq!(strategy.decide(0, false), Directive::Stop);
}

#[test]
fn one_for_one_stops_an_initialization_failure_by_default() {
  let strategy = SupervisorStrategy::one_for_one(3);

  assert_eq!(strategy.decide(0, true), Directive::Stop);
}

#[test]
fn one_for_one_restarting_initialization_failures_restarts_a_failed_start() {
  let strategy = SupervisorStrategy::one_for_one_restarting_initialization_failures(3);

  assert_eq!(strategy.decide(0, true), Directive::Restart);
  assert_eq!(strategy.decide(2, true), Directive::Restart);
  assert_eq!(strategy.decide(3, true), Directive::Stop);
}

#[test]
fn default_strategy_restarts_without_a_limit() {
  let strategy = SupervisorStrategy::default_strategy();

  assert_eq!(strategy.decide(0, false), Directive::Restart);
  assert_eq!(strategy.decide(1000, false), Directive::Restart);
}

#[test]
fn stopping_strategy_stops() {
  let strategy = SupervisorStrategy::stopping_strategy();

  assert_eq!(strategy.decide(0, false), Directive::Stop);
}
