//! A handle that addresses one actor.

use alloc::sync::Arc;
use core::any::Any;
use core::fmt;
use core::hash::{Hash, Hasher};

use crate::kernel::actor::ActorContext;
use crate::kernel::actor_path::ActorPath;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;
use crate::kernel::message::Message;

#[cfg(test)]
#[path = "actor_ref_value_test.rs"]
mod tests;

/// A handle that lets a caller address one actor.
///
/// Mirrors `org.apache.pekko.actor.ActorRef`. It carries an identifier, a
/// generation number, the path of the actor, and the boundary of its shard,
/// which is where a message from outside the shard is posted. A handle to a
/// re-created actor carries a new identifier, so it does not compare equal to
/// the handle of the actor it replaced. Equality and hashing use only the
/// identifier and the generation; the path and the boundary are carried along
/// and never take part in comparing two handles.
pub struct ActorRef {
  id: u64,
  generation: u32,
  path: Arc<ActorPath>,
  boundary: Arc<dyn ShardBoundary>,
  detached: bool,
}

impl ActorRef {
  pub(crate) fn new(id: u64, generation: u32, path: Arc<ActorPath>, boundary: Arc<dyn ShardBoundary>) -> Self {
    Self { id, generation, path, boundary, detached: false }
  }

  /// Returns the path of the actor.
  pub fn path(&self) -> ActorPath {
    (*self.path).clone()
  }

  /// Sends a message to the actor, naming `sender` as the sender.
  pub fn tell<M: Any + Send + Sync>(&self, message: M, sender: ActorRef) {
    Envelope::new(self.clone(), Message::new(message), sender).post_to_boundary();
  }

  /// Sends a message to the actor while keeping the sender being handled.
  ///
  /// Mirrors `org.apache.pekko.actor.ActorRef.forward`, which is
  /// `tell(message, context.sender())`.
  pub fn forward(&self, message: Message, context: &mut ActorContext<'_>) {
    context.forward(self, message);
  }

  pub(crate) fn boundary(&self) -> &Arc<dyn ShardBoundary> {
    &self.boundary
  }

  pub(crate) fn is_detached(&self) -> bool {
    self.detached
  }

  /// Returns a handle with this actor's identity that posts into a delivery
  /// entity that swallows everything.
  ///
  /// The port cannot build Pekko's `EvilCollidingActorRef` on its own from a
  /// test, because the fields are private and equality already ignores the
  /// path and the boundary. This is the limited operation that produces one: it
  /// keeps the identifier, generation, and path of the actor it is called on and
  /// swaps in a no-op shard, so the handle collides with that actor while being
  /// a distinct reference. It is only meaningful on an actor that has stopped,
  /// which [`crate::kernel::actor::ActorContext::stopped_noop_ref`] checks for.
  pub(crate) fn detached_noop(&self) -> Self {
    Self {
      id: self.id,
      generation: self.generation,
      path: Arc::clone(&self.path),
      boundary: alloc::sync::Arc::new(crate::kernel::executor::NoOpBoundary),
      detached: true,
    }
  }

  /// Returns the identity of the shard this handle posts to, for a wait that
  /// drives that shard.
  pub(crate) fn boundary_key(&self) -> usize {
    Arc::as_ptr(&self.boundary) as *const () as usize
  }

  pub(crate) fn shares_boundary_with(&self, boundary: &Arc<dyn ShardBoundary>) -> bool {
    Arc::ptr_eq(&self.boundary, boundary)
  }
}

impl Clone for ActorRef {
  fn clone(&self) -> Self {
    Self {
      id: self.id,
      generation: self.generation,
      path: Arc::clone(&self.path),
      boundary: Arc::clone(&self.boundary),
      detached: self.detached,
    }
  }
}

impl PartialEq for ActorRef {
  fn eq(&self, other: &Self) -> bool {
    self.id == other.id && self.generation == other.generation
  }
}

impl Eq for ActorRef {}

impl Hash for ActorRef {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.id.hash(state);
    self.generation.hash(state);
  }
}

impl fmt::Debug for ActorRef {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "ActorRef({}, generation {})", self.id, self.generation)
  }
}
