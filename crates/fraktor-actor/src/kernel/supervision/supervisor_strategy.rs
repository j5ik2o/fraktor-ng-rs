//! The strategy a parent applies to its children.

use alloc::boxed::Box;

use crate::kernel::actor::ActorContext;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::supervision::Directive;
use crate::kernel::supervision::FailureKind;

#[cfg(test)]
#[path = "supervisor_strategy_test.rs"]
mod tests;

/// Decides the directive for a failing child, reading the child's handle.
///
/// Mirrors Pekko's `Decider` partial function, which a parent writes over the
/// caught exception; the `sender()` of that function is the failing child. The
/// port passes the child's handle as the first argument so a decider can read
/// it without a borrowed context, which a failure has already unwound.
pub type Decider = dyn Fn(&ActorRef, FailureKind) -> Directive + Send + Sync;

/// Notified once a child of the strategy's owner has terminated.
///
/// Mirrors `SupervisorStrategy.handleChildTerminated(context, child,
/// children)`. The port lends the owner's context so the hook can start a
/// replacement child, as the Pekko hook does.
pub type ChildTerminated = dyn Fn(&mut ActorContext<'_>, &ActorRef, &[ActorRef]) + Send + Sync;

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
/// A strategy may also carry a [`Decider`] and a [`ChildTerminated`] hook,
/// which a parent installs to choose per failure and to react to a terminated
/// child. The deciders of Pekko that choose per exception, and the resume and
/// escalate directives, belong to later goals.
///
/// A strategy is a small value: a stored strategy must stay cheap to copy,
/// because a recipe hands the same value to the executor each time it creates
/// an actor. The hooks a caller installs are kept as borrowed trait objects, so
/// the value stays `Copy`; [`SupervisorStrategy::with_decider`] and
/// [`SupervisorStrategy::with_child_terminated`] therefore extend the lifetime
/// of the closure they receive to `'static`.
///
/// The retry count follows `FaultHandling.requestRestartPermission`: every
/// request adds one and the restart is allowed while the count does not exceed
/// the maximum, so a limit of three allows three restarts and stops the fourth
/// failure.
#[derive(Clone, Copy)]
pub struct SupervisorStrategy {
  policy: Policy,
  decider: Option<&'static Decider>,
  child_terminated: Option<&'static ChildTerminated>,
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
    Self { policy: Policy::RestartAlways, decider: None, child_terminated: None }
  }

  /// Returns the strategy that stops a failed child, corresponding to
  /// `SupervisorStrategy.stoppingStrategy`.
  ///
  /// `TestProbe.childActorOf(props)` uses it, as Pekko does.
  pub fn stopping_strategy() -> Self {
    Self { policy: Policy::StopAlways, decider: None, child_terminated: None }
  }

  /// Returns the one-for-one strategy that restarts a failed child at most
  /// `max_retries` times.
  ///
  /// Mirrors `OneForOneStrategy(maxNrOfRetries = max_retries)` with a decider
  /// whose default case restarts. A child that fails while it is being started
  /// is stopped, as the default decider does for `ActorInitializationException`.
  pub fn one_for_one(max_retries: u32) -> Self {
    Self {
      policy: Policy::OneForOne { max_retries, restart_initialization_failures: false },
      decider: None,
      child_terminated: None,
    }
  }

  /// Returns the one-for-one strategy that also restarts a child that fails
  /// while it is being started.
  ///
  /// Mirrors `OneForOneStrategy(maxNrOfRetries = max_retries)` with a decider
  /// that maps `ActorInitializationException` to `Restart`, as the ported test
  /// uses to count the startup attempts of a child that never starts.
  pub fn one_for_one_restarting_initialization_failures(max_retries: u32) -> Self {
    Self {
      policy: Policy::OneForOne { max_retries, restart_initialization_failures: true },
      decider: None,
      child_terminated: None,
    }
  }

  /// Returns this strategy with a decider that chooses per failing child.
  ///
  /// Mirrors `OneForOneStrategy()(decider)`: the decider replaces the default
  /// mapping of a failure kind to a directive. When no decider is installed,
  /// [`SupervisorStrategy::decide`] keeps its default mapping.
  ///
  /// The strategy keeps its hooks as borrowed trait objects so that it stays
  /// `Copy`; the closure is therefore given a `'static` lifetime. A caller that
  /// builds the same strategy on each creation extends one closure per call,
  /// which is the price of a `Copy` strategy value.
  pub fn with_decider<D>(mut self, decider: D) -> Self
  where
    D: Fn(&ActorRef, FailureKind) -> Directive + Send + Sync + 'static,
  {
    self.decider = Some(Box::leak(Box::new(decider)));
    self
  }

  /// Returns this strategy with a hook that runs once a child has terminated.
  ///
  /// Mirrors overriding `SupervisorStrategy.handleChildTerminated`. The hook
  /// runs on the owner of the strategy, with the owner's context, after one of
  /// its children has stopped. It is kept as a borrowed trait object for the
  /// same reason as [`SupervisorStrategy::with_decider`].
  pub fn with_child_terminated<H>(mut self, hook: H) -> Self
  where
    H: Fn(&mut ActorContext<'_>, &ActorRef, &[ActorRef]) + Send + Sync + 'static,
  {
    self.child_terminated = Some(Box::leak(Box::new(hook)));
    self
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

  /// Returns the action to take for the failed `child`.
  ///
  /// Applies the installed [`Decider`] when there is one, so the decider sees
  /// the failing child's handle, and falls back to [`SupervisorStrategy::decide`]
  /// otherwise. Mirrors a Pekko `Decider`, whose `sender()` is the failing
  /// child.
  ///
  /// A `Restart` the decider chooses still needs the permission of a
  /// `one_for_one` strategy, as Pekko's `requestRestartPermission` checks it
  /// after the decider has answered; a restart beyond `max_retries` therefore
  /// becomes `Stop`. `RestartAlways` and `StopAlways` have no limit to enforce
  /// and keep the decider's answer as it is.
  pub(crate) fn decide_for_child(&self, child: &ActorRef, restart_count: u32, failure: FailureKind) -> Directive {
    let Some(decider) = self.decider else {
      return self.decide(restart_count, failure);
    };
    let directive = decider(child, failure);
    match self.policy {
      | Policy::OneForOne { max_retries, .. } => {
        if directive == Directive::Restart && restart_count.saturating_add(1) > max_retries {
          Directive::Stop
        } else {
          directive
        }
      },
      | Policy::RestartAlways | Policy::StopAlways => directive,
    }
  }

  /// Runs the child-terminated hook, if one is installed.
  pub(crate) fn handle_child_terminated(
    &self,
    context: &mut ActorContext<'_>,
    child: &ActorRef,
    children: &[ActorRef],
  ) {
    if let Some(hook) = self.child_terminated {
      hook(context, child, children);
    }
  }

  /// Returns whether a child-terminated hook is installed.
  pub(crate) fn has_child_terminated(&self) -> bool {
    self.child_terminated.is_some()
  }
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
