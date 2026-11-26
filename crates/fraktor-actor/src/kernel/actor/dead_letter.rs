//! A message that was not delivered to its recipient.

use core::any::Any;

use crate::kernel::actor_ref::ActorRef;
use crate::kernel::message::Message;

/// The event that reports a message which was not delivered.
///
/// Mirrors `org.apache.pekko.actor.DeadLetter`: it carries the message that
/// was not delivered, the actor that sent it, and the actor it was addressed
/// to. The event stream publishes it on the `DeadLetter` channel, which is
/// also a channel of [`AllDeadLetters`](super::AllDeadLetters).
#[derive(Clone)]
pub struct DeadLetter {
  message: Message,
  sender: ActorRef,
  recipient: ActorRef,
}

impl DeadLetter {
  /// Creates the report of `message`, sent by `sender` to `recipient`.
  pub fn new<M: Any + Send + Sync>(message: M, sender: ActorRef, recipient: ActorRef) -> Self {
    Self { message: Message::new(message), sender, recipient }
  }

  /// Creates the report of an already wrapped `message`.
  ///
  /// Used when a delivery that never reached its actor is handed to the dead
  /// letters handle with its payload intact, as the mailbox clean-up does
  /// with the messages left in a stopped actor's stash or queue.
  pub fn from_message(message: Message, sender: ActorRef, recipient: ActorRef) -> Self {
    Self { message, sender, recipient }
  }

  /// Returns the message that was not delivered.
  pub fn message(&self) -> &Message {
    &self.message
  }

  /// Returns the actor that sent the message.
  pub fn sender(&self) -> &ActorRef {
    &self.sender
  }

  /// Returns the actor the message was addressed to.
  pub fn recipient(&self) -> &ActorRef {
    &self.recipient
  }
}
