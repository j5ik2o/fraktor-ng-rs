//! The queue of one shard and the boundary of the shard that follows it.

use alloc::collections::VecDeque;
use alloc::sync::Arc;

use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;

/// The place a message sent from inside a shard lands, and the escape out of
/// it.
///
/// Pekko has one mailbox per actor, so a message reaches an actor only through
/// that mailbox. The temporary policy of this port keeps a single local queue
/// per shard, so a message may reach the shard's queue either directly (from
/// an actor of the same shard) or through the boundary (from a handle). Those
/// two ways could reorder messages from one sender to one recipient, which
/// Pekko promises never happens. This type keeps the promise: a message that
/// stays local is queued behind every envelope the same shard has already
/// posted to the boundary, because a sender that posted first must not be
/// overtaken by its own later message.
pub(crate) struct ShardQueue {
  local: VecDeque<Envelope>,
  boundary: Arc<dyn ShardBoundary>,
}

impl ShardQueue {
  pub(crate) fn new(boundary: Arc<dyn ShardBoundary>) -> Self {
    Self { local: VecDeque::new(), boundary }
  }

  pub(crate) fn boundary(&self) -> &Arc<dyn ShardBoundary> {
    &self.boundary
  }

  /// Queues a message that an actor of this shard sent to another actor of
  /// this shard, unless an earlier envelope is still waiting at the boundary.
  ///
  /// The order is kept per shard rather than per sender because the local
  /// queue is shared. That is stricter than Pekko, and it costs a turn at the
  /// boundary only when the boundary is not empty, so a sender to a shard
  /// that is already busy waits for the boundary just as Pekko would wait for
  /// the mailbox of the recipient.
  pub(crate) fn enqueue_locally(&mut self, envelope: Envelope) {
    if self.boundary.has_pending() {
      self.boundary.post(envelope);
    } else {
      self.local.push_back(envelope);
    }
  }

  /// Takes at most one envelope from the boundary and appends it locally.
  pub(crate) fn take_one_from_boundary(&mut self) -> bool {
    match self.boundary.poll() {
      | Some(envelope) => {
        self.local.push_back(envelope);
        true
      },
      | None => false,
    }
  }

  /// Waits at most `max` for one envelope from the boundary and appends it
  /// locally.
  pub(crate) fn await_one_from_boundary(&mut self, max: core::time::Duration) -> bool {
    match self.boundary.poll_within(max) {
      | Some(envelope) => {
        self.local.push_back(envelope);
        true
      },
      | None => false,
    }
  }

  /// Pops the oldest queued envelope.
  pub(crate) fn pop(&mut self) -> Option<Envelope> {
    self.local.pop_front()
  }

  /// Drops every queued envelope of the shard.
  pub(crate) fn clear(&mut self) {
    self.local.clear();
  }
}
