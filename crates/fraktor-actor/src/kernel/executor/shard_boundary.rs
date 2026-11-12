//! The port that carries messages across a shard boundary.

use core::time::Duration;

use crate::kernel::message::Envelope;

/// The transport that carries a message from one shard to another.
///
/// Inside a shard the executor hands messages over on a local queue. Only when
/// a message crosses into another shard does it go through a boundary. Pekko
/// has no direct equivalent: it always uses a mailbox and a dispatcher. The
/// split is the temporary policy that the goal document set for G0b, and it is
/// expressed here as a port so that the kernel stays free of threading and IO
/// and the standard-library implementation lives in the adaptor.
pub trait ShardBoundary: Send + Sync {
  /// Posts one envelope for another shard to pick up.
  fn post(&self, envelope: Envelope);

  /// Takes one envelope if one is already waiting.
  fn poll(&self) -> Option<Envelope>;

  /// Waits at most `max` for one envelope.
  fn poll_within(&self, max: Duration) -> Option<Envelope>;
}
