//! A message that was not delivered and whose dead letter report is kept
//! out of the dead letter log.

use core::any::Any;

use crate::kernel::actor_ref::ActorRef;
use crate::kernel::message::Message;

/// The event that reports a message which was not delivered and whose
/// payload is marked for suppression.
///
/// Mirrors `org.apache.pekko.actor.SuppressedDeadLetter`: the payload
/// carries the [`DeadLetterSuppression`](super::DeadLetterSuppression)
/// marker, or the kernel knows the payload is one it generates itself, such
/// as a `Terminated` notice. The recipient named here is the reference that
/// processed the message — the dead letters handle, or the empty reference
/// of a selection that resolved to nothing — not the actor the message was
/// addressed to. The dead letter listener does not subscribe to this
/// channel, so these events are never counted in the dead letter log, while
/// a subscriber of [`AllDeadLetters`](super::AllDeadLetters) still sees
/// them.
#[derive(Clone)]
pub struct SuppressedDeadLetter {
  message: Message,
  sender: ActorRef,
  recipient: ActorRef,
}

impl SuppressedDeadLetter {
  /// Creates the report of `message`, sent by `sender` and processed by
  /// `recipient`.
  pub fn new<M: Any + Send + Sync>(message: M, sender: ActorRef, recipient: ActorRef) -> Self {
    Self { message: Message::new(message), sender, recipient }
  }

  /// Creates the report of an already wrapped `message`.
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

  /// Returns the reference that processed the message.
  pub fn recipient(&self) -> &ActorRef {
    &self.recipient
  }
}
