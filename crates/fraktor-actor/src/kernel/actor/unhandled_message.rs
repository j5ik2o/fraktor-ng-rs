//! A message that an actor could not handle.

use crate::kernel::actor_ref::ActorRef;
use crate::kernel::message::Message;

/// The event that reports a message an actor could not handle.
///
/// Mirrors `org.apache.pekko.actor.UnhandledMessage`: it carries the message,
/// the actor that sent it, and the actor that did not handle it. Pekko's event
/// stream publishes it on the `UnhandledMessage` channel, where a logger can
/// turn it into a debug event.
#[derive(Clone)]
pub struct UnhandledMessage {
  message: Message,
  sender: ActorRef,
  recipient: ActorRef,
}

impl UnhandledMessage {
  /// Creates the report of `message`, sent by `sender` to `recipient`.
  pub fn new<M: core::any::Any + Send + Sync>(message: M, sender: ActorRef, recipient: ActorRef) -> Self {
    Self { message: Message::new(message), sender, recipient }
  }

  /// Returns the message that was not handled.
  pub fn message(&self) -> &Message {
    &self.message
  }

  /// Returns the actor that sent the message.
  pub fn sender(&self) -> &ActorRef {
    &self.sender
  }

  /// Returns the actor that did not handle the message.
  pub fn recipient(&self) -> &ActorRef {
    &self.recipient
  }
}
