//! The kind of failure a child raised.

/// The kind of failure a child raised, as the default decider distinguishes it.
///
/// Mirrors the parts of `org.apache.pekko.actor.FaultHandling.defaultDecider`
/// that the strategies of this goal need: a failure during startup maps to
/// `Stop`, a `Kill` maps to `Stop`, and any other failure maps to `Restart`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
  /// The child failed while it was being created or in `preStart`, which Pekko
  /// wraps in `ActorInitializationException`.
  Initialization,
  /// The child received `Kill`, which Pekko raises as `ActorKilledException`.
  Killed,
  /// The child threw while handling a message.
  MessageHandling,
}
