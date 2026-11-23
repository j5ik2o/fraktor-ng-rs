//! A boundary that swallows every message it is handed.
//!
//! A stopped/no-op reference carries one of these, mirroring Pekko's
//! `EvilCollidingActorRef`: it keeps the identity of the actor it was detached
//! from but posts into a delivery entity that ignores everything and never
//! answers. It is what makes `watch` on that reference silent instead of an
//! immediate `Terminated`.

use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;

/// A delivery entity that discards every envelope.
pub(crate) struct NoOpBoundary;

impl ShardBoundary for NoOpBoundary {
  fn post(&self, _envelope: Envelope) {}

  fn poll(&self) -> Option<Envelope> {
    None
  }

  fn poll_within(&self, _max: core::time::Duration) -> Option<Envelope> {
    None
  }

  fn has_pending(&self) -> bool {
    false
  }

  fn close(&self) {}
}
