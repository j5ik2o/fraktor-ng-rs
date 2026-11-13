//! What a supervisor decides for a failed child.

/// The action a supervisor takes for a child that threw while handling a
/// message.
///
/// Mirrors the subset of `org.apache.pekko.actor.SupervisorStrategy.Directive`
/// that the test probe needs: it restarts the child or stops it. Resuming and
/// escalating belong to later goals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Directive {
  /// Re-create the child from its recipe and run its `post_restart`.
  Restart,
  /// Stop the child and tell its watchers.
  Stop,
}
