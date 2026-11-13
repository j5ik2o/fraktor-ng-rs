//! A boundary that hands each envelope to a function right away.

use alloc::sync::Arc;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;
use core::time::Duration;

use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;
use crate::kernel::message::Message;

/// The handler that a function reference runs for each delivered message.
pub(crate) type Handler = Arc<dyn Fn(&ActorRef, Message) + Send + Sync>;

/// A boundary whose shard holds no actors: a message posted to it runs a
/// handler on the spot.
///
/// Pekko's `TestActor` runs on the calling-thread dispatcher, so a message
/// handed to a test actor lands in its queue before the sender returns. The
/// port models that with a function reference: the reference shares this
/// boundary, and posting to it calls the handler instead of queueing. Nothing
/// is ever waiting here, so `poll` and `has_pending` report emptiness and the
/// handler alone decides what the message does.
pub(crate) struct FunctionBoundary {
  handler: Handler,
  open: AtomicBool,
}

impl FunctionBoundary {
  pub(crate) fn new(handler: Handler) -> Self {
    Self { handler, open: AtomicBool::new(true) }
  }
}

impl ShardBoundary for FunctionBoundary {
  fn post(&self, envelope: Envelope) {
    if !self.open.load(Ordering::Acquire) {
      return;
    }
    let (recipient, message, sender) = envelope.into_parts();
    let _ = recipient;
    (self.handler)(&sender, message);
  }

  fn poll(&self) -> Option<Envelope> {
    None
  }

  fn poll_within(&self, _max: Duration) -> Option<Envelope> {
    None
  }

  fn has_pending(&self) -> bool {
    false
  }

  fn close(&self) {
    self.open.store(false, Ordering::Release);
  }
}
