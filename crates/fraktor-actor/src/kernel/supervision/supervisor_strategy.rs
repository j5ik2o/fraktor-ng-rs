//! The strategy a parent applies to its children.

use crate::kernel::supervision::Directive;

#[cfg(test)]
#[path = "supervisor_strategy_test.rs"]
mod tests;

/// How a parent handles a child that fails.
///
/// Mirrors `org.apache.pekko.actor.SupervisorStrategy` for the strategies that
/// the ported tests need:
///
/// - `default_strategy`, which restarts every failed child without a limit but
///   stops it when the failure is an initialization failure or a `Kill`, as the
///   default decider does for `ActorInitializationException` and
///   `ActorKilledException`, and `stopping_strategy`, which stops it, as the
///   test probe uses them;
/// - `one_for_one`, which restarts a failed child until its retry count runs
///   out and then stops it;
/// - `one_for_one_restarting_initialization_failures`, which applies the same
///   count to a child that fails while it is being started.
///
/// The deciders of Pekko that choose per exception, and the resume and escalate
/// directives, belong to later goals. The retry count follows
/// `FaultHandling.requestRestartPermission`: every request adds one and the
/// restart is allowed while the count does not exceed the maximum, so a limit
/// of three allows three restarts and stops the fourth failure.
#[derive(Clone, Copy)]
pub struct SupervisorStrategy {
  policy: Policy,
}

/// The internal decision rule of a [`SupervisorStrategy`].
#[derive(Clone, Copy)]
enum Policy {
  /// Restart every failure except an initialization failure or a `Kill`, which
  /// it stops, as `SupervisorStrategy.defaultStrategy` with Pekko's default
  /// decider does.
  RestartAlways,
  /// Stop every failure, as `SupervisorStrategy.stoppingStrategy`.
  StopAlways,
  /// Restart a failure until `max_retries` requests are used, and stop when
  /// they run out.
  ///
  /// `restart_initialization_failures` decides whether a child that fails
  /// while it is being started is restarted, which is what the default
  /// decider's mapping of `ActorInitializationException` to `Stop` otherwise
  /// prevents.
  OneForOne {
    /// The number of restart requests that are allowed.
    max_retries: u32,
    /// Whether a failure during startup is restarted instead of stopped.
    restart_initialization_failures: bool,
  },
}

impl SupervisorStrategy {
  /// Returns the strategy that restarts a failed child, corresponding to
  /// `SupervisorStrategy.defaultStrategy`.
  pub fn default_strategy() -> Self {
    Self { policy: Policy::RestartAlways }
  }

  /// Returns the strategy that stops a failed child, corresponding to
  /// `SupervisorStrategy.stoppingStrategy`.
  ///
  /// `TestProbe.childActorOf(props)` uses it, as Pekko does.
  pub fn stopping_strategy() -> Self {
    Self { policy: Policy::StopAlways }
  }

  /// Returns the one-for-one strategy that restarts a failed child at most
  /// `max_retries` times.
  ///
  /// Mirrors `OneForOneStrategy(maxNrOfRetries = max_retries)` with a decider
  /// whose default case restarts. A child that fails while it is being started
  /// is stopped, as the default decider does for `ActorInitializationException`.
  pub fn one_for_one(max_retries: u32) -> Self {
    Self { policy: Policy::OneForOne { max_retries, restart_initialization_failures: false } }
  }

  /// Returns the one-for-one strategy that also restarts a child that fails
  /// while it is being started.
  ///
  /// Mirrors `OneForOneStrategy(maxNrOfRetries = max_retries)` with a decider
  /// that maps `ActorInitializationException` to `Restart`, as the ported test
  /// uses to count the startup attempts of a child that never starts.
  pub fn one_for_one_restarting_initialization_failures(max_retries: u32) -> Self {
    Self { policy: Policy::OneForOne { max_retries, restart_initialization_failures: true } }
  }

  /// Returns the action to take for a failed child.
  ///
  /// `restart_count` is the number of restart requests this child has already
  /// made. `failure` is the kind of failure the child raised. Mirrors
  /// `FaultHandling.requestRestartPermission`: the request adds one and is
  /// allowed while the result does not exceed the maximum.
  pub(crate) fn decide(&self, restart_count: u32, failure: FailureKind) -> Directive {
    match self.policy {
      | Policy::RestartAlways => match failure {
        | FailureKind::MessageHandling => Directive::Restart,
        | FailureKind::Initialization | FailureKind::Killed => Directive::Stop,
      },
      | Policy::StopAlways => Directive::Stop,
      | Policy::OneForOne { max_retries, restart_initialization_failures } => {
        if failure == FailureKind::Initialization && !restart_initialization_failures {
          return Directive::Stop;
        }
        let count = restart_count.saturating_add(1);
        if count <= max_retries {
          Directive::Restart
        } else {
          Directive::Stop
        }
      },
    }
  }
}

/// The kind of failure a child raised, as the default decider distinguishes it.
///
/// Mirrors the parts of `org.apache.pekko.actor.FaultHandling.defaultDecider`
/// that the strategies of this goal need: a failure during startup maps to
/// `Stop`, a `Kill` maps to `Stop`, and any other failure maps to `Restart`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FailureKind {
  /// The child failed while it was being created or in `preStart`, which Pekko
  /// wraps in `ActorInitializationException`.
  Initialization,
  /// The child received `Kill`, which Pekko raises as `ActorKilledException`.
  Killed,
  /// The child threw while handling a message.
  MessageHandling,
}

impl core::fmt::Debug for SupervisorStrategy {
  fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    match self.policy {
      | Policy::RestartAlways => formatter.write_str("SupervisorStrategy::RestartAlways"),
      | Policy::StopAlways => formatter.write_str("SupervisorStrategy::StopAlways"),
      | Policy::OneForOne { max_retries, restart_initialization_failures } => formatter
        .debug_struct("SupervisorStrategy::OneForOne")
        .field("max_retries", &max_retries)
        .field("restart_initialization_failures", &restart_initialization_failures)
        .finish(),
    }
  }
}
