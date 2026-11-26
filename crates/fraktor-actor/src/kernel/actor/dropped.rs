//! A message that an actor received and chose to drop.

use alloc::string::String;
use core::any::Any;

use crate::kernel::actor_ref::ActorRef;
use crate::kernel::message::Message;

/// The event that reports a message an actor dropped on purpose.
///
/// Mirrors `org.apache.pekko.actor.Dropped`: it carries the message, the
/// reason the actor gives for dropping it, the sender, and the actor that
/// dropped it. Pekko's three-argument form names no sender, and the log
/// line the dead letter listener writes for a dropped message does not name
/// one either; the port's convention is that a senderless report carries
/// the dead letters handle as its sender, which is how the listener tells
/// a real sender from none.
#[derive(Clone)]
pub struct Dropped {
  message: Message,
  reason: String,
  sender: ActorRef,
  recipient: ActorRef,
}

impl Dropped {
  /// Creates the report of `message`, dropped by `recipient` for `reason`.
  pub fn new<M: Any + Send + Sync>(message: M, reason: &str, sender: ActorRef, recipient: ActorRef) -> Self {
    Self { message: Message::new(message), reason: String::from(reason), sender, recipient }
  }

  /// Returns the message that was dropped.
  pub fn message(&self) -> &Message {
    &self.message
  }

  /// Returns why the actor dropped the message.
  pub fn reason(&self) -> &str {
    &self.reason
  }

  /// Returns the actor that sent the message.
  pub fn sender(&self) -> &ActorRef {
    &self.sender
  }

  /// Returns the actor that dropped the message.
  pub fn recipient(&self) -> &ActorRef {
    &self.recipient
  }
}
