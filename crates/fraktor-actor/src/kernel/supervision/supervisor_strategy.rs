//! The strategy a parent applies to its children.

use crate::kernel::supervision::Directive;

/// How a parent handles a child that fails.
///
/// Mirrors `org.apache.pekko.actor.SupervisorStrategy` for the two strategies
/// that the test probe needs: `defaultStrategy`, which restarts the child, and
/// `stoppingStrategy`, which stops it. The finer deciders of Pekko (resume,
/// escalate, per-exception decisions) belong to later goals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SupervisorStrategy {
  directive: Directive,
}

impl SupervisorStrategy {
  /// Returns the strategy that restarts a failed child, corresponding to
  /// `SupervisorStrategy.defaultStrategy`.
  pub fn default_strategy() -> Self {
    Self { directive: Directive::Restart }
  }

  /// Returns the strategy that stops a failed child, corresponding to
  /// `SupervisorStrategy.stoppingStrategy`.
  ///
  /// `TestProbe.childActorOf(props)` uses it, as Pekko does.
  pub fn stopping_strategy() -> Self {
    Self { directive: Directive::Stop }
  }

  /// Returns the action to take for a failed child.
  pub(crate) fn decide(&self) -> Directive {
    self.directive
  }
}
