//! The view an actor gets while it handles one message.

use alloc::collections::VecDeque;
use alloc::sync::Arc;

use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;
use crate::kernel::message::Message;

/// What an actor may do while it handles a message: address itself, name its
/// sender, and send messages.
///
/// Mirrors `org.apache.pekko.actor.ActorContext`. Pekko keeps the context on
/// the actor and lets it be read at any time; the port lends it out only for
/// the duration of one `receive` call, as the ownership model decides.
///
/// The context keeps the local queue of its shard. A message to an actor of the
/// same shard goes onto that queue; a message to an actor of another shard goes
/// through the recipient's boundary. That split is how "local queues inside a
/// shard, channels only at the boundary" is expressed in the code.
pub struct ActorContext<'a> {
  self_ref: ActorRef,
  sender: ActorRef,
  boundary: Arc<dyn ShardBoundary>,
  local_queue: &'a mut VecDeque<Envelope>,
}

impl<'a> ActorContext<'a> {
  pub(crate) fn new(
    self_ref: ActorRef,
    sender: ActorRef,
    boundary: Arc<dyn ShardBoundary>,
    local_queue: &'a mut VecDeque<Envelope>,
  ) -> Self {
    Self { self_ref, sender, boundary, local_queue }
  }

  /// Returns a handle to the actor that is handling the message.
  pub fn self_ref(&self) -> ActorRef {
    self.self_ref.clone()
  }

  /// Returns a handle to the actor that sent the message being handled.
  pub fn sender(&self) -> ActorRef {
    self.sender.clone()
  }

  /// Sends a message to `target`, naming this actor as the sender.
  pub fn tell(&mut self, target: &ActorRef, message: Message) {
    let envelope = Envelope::new(target.clone(), message, self.self_ref());
    self.dispatch(envelope);
  }

  pub(crate) fn forward(&mut self, target: &ActorRef, message: Message) {
    let envelope = Envelope::new(target.clone(), message, self.sender());
    self.dispatch(envelope);
  }

  fn dispatch(&mut self, envelope: Envelope) {
    match envelope.into_local_if_same_shard(&self.boundary) {
      | Ok(local) => self.local_queue.push_back(local),
      | Err(remote) => remote.post_to_boundary(),
    }
  }
}
