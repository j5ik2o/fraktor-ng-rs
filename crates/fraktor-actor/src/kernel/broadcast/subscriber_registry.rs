//! The subscriber set that a broadcaster owns.

use alloc::vec::Vec;

use crate::kernel::actor::ActorContext;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::broadcast::Subscribe;
use crate::kernel::broadcast::Unsubscribe;
use crate::kernel::broadcast::WithSubscribers;
use crate::kernel::message::Message;

/// The actors a broadcaster delivers to, in the order they registered.
///
/// Mirrors `org.apache.pekko.routing.Listeners`: a broadcaster mixes the set
/// into itself and chains [`Subscribers::handle_management`] into its own
/// receive so that the management messages update the set. The set holds the
/// actors for the lifetime of the broadcaster; the port keeps no system-wide
/// table and does not watch the actors, so a stopped actor stays until it is
/// removed.
///
/// Order is not part of the original case's assertions; the set happens to keep
/// registration order so that the port has a deterministic walk.
pub struct Subscribers {
  actors: Vec<ActorRef>,
}

impl Subscribers {
  /// Creates an empty subscriber set.
  pub fn new() -> Self {
    Self { actors: Vec::new() }
  }

  /// Handles one management message and reports whether the message was one.
  ///
  /// Returns `true` when `message` is a management message (a [`Subscribe`],
  /// [`Unsubscribe`], or [`WithSubscribers`]); the caller then stops handling the
  /// message. The return value says only that the message was recognized and
  /// consumed as management traffic, not that the set changed: a duplicate
  /// [`Subscribe`] or an [`Unsubscribe`] for an absent actor still returns
  /// `true`. Returns `false` for any other message, which the caller handles as
  /// its own traffic.
  pub fn handle_management(&mut self, message: &Message) -> bool {
    if let Some(subscribe) = message.downcast_ref::<Subscribe>() {
      self.add(subscribe.actor().clone());
      return true;
    }
    if let Some(unsubscribe) = message.downcast_ref::<Unsubscribe>() {
      self.remove(unsubscribe.actor());
      return true;
    }
    if let Some(with_subscribers) = message.downcast_ref::<WithSubscribers>() {
      self.for_each(with_subscribers);
      return true;
    }
    false
  }

  /// Delivers `message` to every current subscriber.
  ///
  /// Mirrors `Listeners.gossip`: each subscriber receives the message once and
  /// the broadcaster is the sender, as Pekko's `implicit self` names the
  /// enclosing actor when `gossip` runs inside it. The message is built once
  /// and cloned per subscriber so every delivery shares one payload.
  pub fn gossip<M>(&self, message: M, context: &mut ActorContext<'_>)
  where
    M: core::any::Any + Send + Sync,
  {
    let delivery = Message::new(message);
    for actor in &self.actors {
      context.tell(actor, delivery.clone());
    }
  }

  fn add(&mut self, actor: ActorRef) {
    if !self.actors.iter().any(|existing| existing == &actor) {
      self.actors.push(actor);
    }
  }

  fn remove(&mut self, actor: &ActorRef) {
    self.actors.retain(|existing| existing != actor);
  }

  fn for_each(&self, with_subscribers: &WithSubscribers) {
    for actor in &self.actors {
      with_subscribers.apply(actor);
    }
  }
}

impl Default for Subscribers {
  fn default() -> Self {
    Self::new()
  }
}
