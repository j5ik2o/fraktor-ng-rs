//! A view of one dead letter event, whichever of the four kinds it is.

use crate::kernel::actor::DeadLetter;
use crate::kernel::actor::Dropped;
use crate::kernel::actor::SuppressedDeadLetter;
use crate::kernel::actor::UnhandledMessage;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::message::Message;

/// A view of one dead letter event, whichever of the four kinds it is.
///
/// Mirrors the pattern match Pekko code writes against the
/// `AllDeadLetters` trait: the view borrows the event a [`Message`]
/// carries and answers its message, sender, and recipient uniformly, which
/// is what the dead letter listener and the routing rules read.
pub enum AnyDeadLetter<'a> {
  /// A message that was not delivered.
  DeadLetter(&'a DeadLetter),
  /// A message that was not delivered and is kept out of the log.
  Suppressed(&'a SuppressedDeadLetter),
  /// A message an actor dropped on purpose.
  Dropped(&'a Dropped),
  /// A message an actor did not handle.
  Unhandled(&'a UnhandledMessage),
}

impl<'a> AnyDeadLetter<'a> {
  /// Returns the view of the event `message` carries, or `None` when it
  /// carries none of the four kinds.
  pub fn from_message(message: &'a Message) -> Option<Self> {
    if let Some(event) = message.downcast_ref::<DeadLetter>() {
      return Some(Self::DeadLetter(event));
    }
    if let Some(event) = message.downcast_ref::<SuppressedDeadLetter>() {
      return Some(Self::Suppressed(event));
    }
    if let Some(event) = message.downcast_ref::<Dropped>() {
      return Some(Self::Dropped(event));
    }
    if let Some(event) = message.downcast_ref::<UnhandledMessage>() {
      return Some(Self::Unhandled(event));
    }
    None
  }

  /// Returns the message the event reports.
  pub fn message(&self) -> &Message {
    match self {
      | Self::DeadLetter(event) => event.message(),
      | Self::Suppressed(event) => event.message(),
      | Self::Dropped(event) => event.message(),
      | Self::Unhandled(event) => event.message(),
    }
  }

  /// Returns the actor that sent the reported message.
  pub fn sender(&self) -> &ActorRef {
    match self {
      | Self::DeadLetter(event) => event.sender(),
      | Self::Suppressed(event) => event.sender(),
      | Self::Dropped(event) => event.sender(),
      | Self::Unhandled(event) => event.sender(),
    }
  }

  /// Returns the actor the event names as its recipient.
  pub fn recipient(&self) -> &ActorRef {
    match self {
      | Self::DeadLetter(event) => event.recipient(),
      | Self::Suppressed(event) => event.recipient(),
      | Self::Dropped(event) => event.recipient(),
      | Self::Unhandled(event) => event.recipient(),
    }
  }
}
