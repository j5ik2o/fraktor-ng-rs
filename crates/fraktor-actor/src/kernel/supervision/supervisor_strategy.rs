// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2017-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The strategy a parent applies to its children.

use core::time::Duration;

use crate::kernel::{
  actor::ActorContext,
  actor_ref::ActorRef,
  supervision::{Directive, FailureKind, RestartLimit, RestartWindow},
  sync::Arc,
};

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

/// Restarts the exception kinds represented by this port.
///
/// Mirrors `SupervisorStrategy.makeDecider(List(classOf[Exception]))`:
/// initialization and killed exceptions also inherit from `Exception`, so
/// they request a restart just like message-handling exceptions. The
/// all-for-one policy then checks every sibling's restart permission.
fn restart_on_exception(_child: &ActorRef, _failure: FailureKind) -> Directive {
  Directive::Restart
}

/// How a parent handles a child that fails.
///
/// Mirrors `org.apache.pekko.actor.SupervisorStrategy` for the strategies that
/// the ported tests need:
///
/// - `default_strategy`, which restarts every failed child without a limit but stops it when the
///   failure is an initialization failure or a `Kill`, as the default decider does for
///   `ActorInitializationException` and `ActorKilledException`, and `stopping_strategy`, which
///   stops it, as the test probe uses them;
/// - `one_for_one`, which restarts a failed child until its retry count runs out and then stops it;
/// - `one_for_one_restarting_initialization_failures`, which applies the same count to a child that
///   fails while it is being started.
///
/// A strategy may also carry a [`Decider`] and a [`ChildTerminated`] hook,
/// which a parent installs to choose per failure and to react to a terminated
/// child. The deciders of Pekko that choose per exception, and the resume and
/// escalate directives, belong to later goals.
///
/// A stored strategy stays cheap to clone because a recipe hands it to the
/// executor each time it creates an actor. Clones share ownership of the
/// installed closures through `Arc`. Dropping the last strategy releases the
/// closures and their captures; captured data with other owners follows those
/// owners' lifetimes too.
///
/// The retry count follows `FaultHandling.requestRestartPermission`: every
/// request adds one and the restart is allowed while the count does not exceed
/// the maximum, so a limit of three allows three restarts and stops the fourth
/// failure.
#[derive(Clone)]
pub struct SupervisorStrategy {
  policy: Policy,
  decider: Option<Arc<Decider>>,
  decider_custom: bool,
  child_terminated: Option<Arc<ChildTerminated>>,
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
    max_retries: RestartLimit,
    /// The time range the restart requests are counted in.
    window: RestartWindow,
    /// Whether a failure during startup is restarted instead of stopped.
    restart_initialization_failures: bool,
  },
  /// Restart all the direct children of a parent, or stop them all.
  ///
  /// Mirrors `AllForOneStrategy`: the failed child's failure is treated as one
  /// unit, and every direct sibling restarts when all of them may still
  /// restart within the retry count and the time window, or every one stops
  /// when any of them may not. `max_retries` is the number of restart requests
  /// each child may make, and `window` is the optional time range they are
  /// counted in.
  AllForOne {
    /// The number of restart requests that are allowed per child.
    max_retries: RestartLimit,
    /// The time range the restart requests are counted in, if any.
    window: RestartWindow,
  },
}

impl SupervisorStrategy {
  /// Returns the strategy that restarts a failed child, corresponding to
  /// `SupervisorStrategy.defaultStrategy`.
  pub fn default_strategy() -> Self {
    Self { policy: Policy::RestartAlways, decider: None, decider_custom: false, child_terminated: None }
  }

  /// Returns the strategy that stops a failed child, corresponding to
  /// `SupervisorStrategy.stoppingStrategy`.
  ///
  /// `TestProbe.childActorOf(props)` uses it, as Pekko does.
  pub fn stopping_strategy() -> Self {
    Self { policy: Policy::StopAlways, decider: None, decider_custom: false, child_terminated: None }
  }

  /// Returns the one-for-one strategy that restarts a failed child at most
  /// `max_retries` times.
  ///
  /// Mirrors `OneForOneStrategy(maxNrOfRetries = max_retries)` with a decider
  /// whose default case restarts. A child that fails while it is being started
  /// is stopped, as the default decider does for `ActorInitializationException`.
  pub fn one_for_one(max_retries: u32) -> Self {
    Self::one_for_one_with_window(RestartLimit::new(max_retries), RestartWindow::Unbounded)
  }

  /// Returns the one-for-one strategy that also restarts a child that fails
  /// while it is being started.
  ///
  /// Mirrors `OneForOneStrategy(maxNrOfRetries = max_retries)` with a decider
  /// that maps `ActorInitializationException` to `Restart`, as the ported test
  /// uses to count the startup attempts of a child that never starts.
  pub fn one_for_one_restarting_initialization_failures(max_retries: u32) -> Self {
    Self {
      policy: Policy::OneForOne {
        max_retries: RestartLimit::new(max_retries),
        window: RestartWindow::Unbounded,
        restart_initialization_failures: true,
      },
      decider: None,
      decider_custom: false,
      child_terminated: None,
    }
  }

  /// Returns the one-for-one strategy with an optional retry maximum and a
  /// time window, mirroring `OneForOneStrategy(maxNrOfRetries, withinTimeRange)`.
  ///
  /// `max_retries` is the number of restarts allowed per child. Use
  /// [`RestartLimit::unlimited`] to omit the maximum. `window` is the time range
  /// the requests are counted in, as
  /// Pekko's `withinTimeRange`. The two parameters resolve as
  /// `ChildRestartStats.requestRestartPermission` does:
  ///
  /// - an omitted maximum with no window grants every restart request without counting it;
  /// - an omitted maximum with a finite window allows one restart per window, because a missing
  ///   maximum defaults to one inside the window algorithm;
  /// - a finite maximum with no window counts the restarts over the child's lifetime;
  /// - a finite maximum with a finite window counts them inside the window and reopens the count
  ///   once the window has expired.
  ///
  /// A finite window reads monotonic time from the system boundary or an
  /// installed clock, as the all-for-one strategy does. A child that fails
  /// while it is being started is stopped, as `one_for_one` does.
  pub fn one_for_one_with_window(max_retries: RestartLimit, window: RestartWindow) -> Self {
    Self {
      policy: Policy::OneForOne { max_retries, window, restart_initialization_failures: false },
      decider: None,
      decider_custom: false,
      child_terminated: None,
    }
  }

  /// Returns the all-for-one strategy that restarts or stops every direct
  /// sibling together.
  ///
  /// Mirrors `AllForOneStrategy(maxNrOfRetries, withinTimeRange)(List(classOf[Exception]))`:
  /// the installed decider requests a restart for message-handling,
  /// initialization, and killed exceptions. Every sibling must grant restart
  /// permission before any sibling restarts; otherwise all siblings stop.
  /// [`SupervisorStrategy::with_decider`] replaces that decision when needed.
  /// `window` determines when the restart count resets. A finite window
  /// requires monotonic time from the system boundary or an installed clock.
  pub fn all_for_one(max_retries: u32, window: RestartWindow) -> Self {
    Self {
      policy: Policy::AllForOne { max_retries: RestartLimit::new(max_retries), window },
      decider: Some(Arc::new(restart_on_exception)),
      decider_custom: false,
      child_terminated: None,
    }
  }

  /// Returns whether this strategy applies a restart or stop to all siblings.
  pub(crate) fn is_all_for_one(&self) -> bool {
    matches!(self.policy, Policy::AllForOne { .. })
  }

  /// Returns the per-child restart limit and time window a strategy checks
  /// against a child's [`RestartHistory`](crate::kernel::supervision).
  ///
  /// Returns `Some` for an all-for-one strategy and for a one-for-one strategy
  /// with a finite window; the pair is what the history is asked to permit
  /// against. Returns `None` for the other strategies, including a one-for-one
  /// with no window, whose retry count the directive decision already applies.
  /// A one-for-one without a maximum but with a finite window resolves to one
  /// restart per window, as Pekko's window algorithm defaults the maximum to
  /// one.
  pub(crate) fn restart_permission(&self) -> Option<(RestartLimit, Option<Duration>)> {
    match self.policy {
      | Policy::AllForOne { max_retries, window } => Some((max_retries, window.duration())),
      | Policy::OneForOne { max_retries, window, .. } => {
        window.duration().map(|duration| (max_retries.for_window(), Some(duration)))
      },
      | Policy::RestartAlways | Policy::StopAlways => None,
    }
  }

  /// Returns whether an unwindowed restart needs a cumulative record.
  ///
  /// An unlimited one-for-one policy grants requests without consuming a
  /// restart allowance. A later strategy can apply a limit to the same child.
  pub(crate) fn records_plain_restart(&self) -> bool {
    !matches!(self.policy, Policy::OneForOne { max_retries, .. } if max_retries.is_unlimited())
  }

  /// Returns this strategy with a decider that chooses per failing child.
  ///
  /// Mirrors `OneForOneStrategy()(decider)`: the decider replaces the default
  /// mapping of a failure kind to a directive. When no decider is installed,
  /// [`SupervisorStrategy::decide`] keeps its default mapping.
  ///
  /// The strategy owns the closure, and clones share that ownership until the
  /// last strategy holding it is dropped.
  pub fn with_decider<D>(mut self, decider: D) -> Self
  where
    D: Fn(&ActorRef, FailureKind) -> Directive + Send + Sync + 'static,
  {
    self.decider = Some(Arc::new(decider));
    self.decider_custom = true;
    self
  }

  /// Returns whether the decider was installed by a caller through
  /// [`with_decider`](Self::with_decider), rather than being the decision
  /// the policy itself carries (the all-for-one strategy's built-in
  /// restart-on-exception decider is not a caller's).
  pub(crate) fn has_custom_decider(&self) -> bool {
    self.decider_custom
  }

  /// Returns this strategy with a hook that runs once a child has terminated.
  ///
  /// Mirrors overriding `SupervisorStrategy.handleChildTerminated`. The hook
  /// runs on the owner of the strategy, with the owner's context, after one of
  /// its children has stopped. The strategy owns the closure and shares it
  /// with clones, as [`SupervisorStrategy::with_decider`] does.
  pub fn with_child_terminated<H>(mut self, hook: H) -> Self
  where
    H: Fn(&mut ActorContext<'_>, &ActorRef, &[ActorRef]) + Send + Sync + 'static,
  {
    self.child_terminated = Some(Arc::new(hook));
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
      | Policy::AllForOne { .. } => Directive::Restart,
      | Policy::OneForOne { max_retries, window, restart_initialization_failures } => {
        if failure == FailureKind::Initialization && !restart_initialization_failures {
          return Directive::Stop;
        }
        // A finite window is checked against the child's history after the
        // restart is chosen, so the cumulative count must not pre-empt it.
        if window.duration().is_some() {
          return Directive::Restart;
        }
        let count = restart_count.saturating_add(1);
        if max_retries.permits(count) {
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
    let Some(decider) = &self.decider else {
      return self.decide(restart_count, failure);
    };
    let directive = decider(child, failure);
    match self.policy {
      | Policy::OneForOne { max_retries, window, .. } => {
        // A finite window is checked against the child's history after the
        // restart is chosen, so the cumulative count must not pre-empt it.
        if window.duration().is_some() {
          return directive;
        }
        if directive == Directive::Restart && !max_retries.permits(restart_count.saturating_add(1)) {
          Directive::Stop
        } else {
          directive
        }
      },
      | Policy::AllForOne { .. } | Policy::RestartAlways | Policy::StopAlways => directive,
    }
  }

  /// Runs the child-terminated hook, if one is installed.
  pub(crate) fn handle_child_terminated(
    &self,
    context: &mut ActorContext<'_>,
    child: &ActorRef,
    children: &[ActorRef],
  ) {
    if let Some(hook) = &self.child_terminated {
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
      | Policy::OneForOne { max_retries, window, restart_initialization_failures } => formatter
        .debug_struct("SupervisorStrategy::OneForOne")
        .field("max_retries", &max_retries)
        .field("window", &window)
        .field("restart_initialization_failures", &restart_initialization_failures)
        .finish(),
      | Policy::AllForOne { max_retries, window } => formatter
        .debug_struct("SupervisorStrategy::AllForOne")
        .field("max_retries", &max_retries)
        .field("window", &window)
        .finish(),
    }
  }
}
