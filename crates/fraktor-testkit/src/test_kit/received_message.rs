//! A message with the sender that delivered it.

use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;

/// One message the test actor received, together with its sender.
///
/// Mirrors `org.apache.pekko.testkit.TestKit.TestActor.RealMessage`, which
/// pairs the message with the sender so that `lastSender` and `reply` can
/// address the actor that just spoke.
#[derive(Clone)]
pub struct ReceivedMessage {
  message: Message,
  sender: ActorRef,
}

impl ReceivedMessage {
  pub(crate) fn new(message: Message, sender: ActorRef) -> Self {
    Self { message, sender }
  }

  /// Returns the message.
  pub fn message(&self) -> &Message {
    &self.message
  }

  /// Returns the sender of the message.
  pub fn sender(&self) -> &ActorRef {
    &self.sender
  }
}
