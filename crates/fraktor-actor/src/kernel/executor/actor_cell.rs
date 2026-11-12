//! One actor inside the executor, with the behaviour it runs.

use alloc::boxed::Box;

use crate::kernel::actor::Actor;
use crate::kernel::actor::ActorContext;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::message::Message;

/// An actor that the executor owns and drives.
///
/// Mirrors the mailbox-and-actor pair that an `org.apache.pekko.actor.ActorCell`
/// keeps: the executor owns the behaviour and the handle, and lends the context
/// out only while a message is being handled.
pub(crate) struct ActorCell {
  behavior: Box<dyn Actor>,
  self_ref: ActorRef,
}

impl ActorCell {
  pub(crate) fn new(behavior: Box<dyn Actor>, self_ref: ActorRef) -> Self {
    Self { behavior, self_ref }
  }

  /// Returns whether this cell is the actor that `recipient` addresses.
  pub(crate) fn accepts(&self, recipient: &ActorRef) -> bool {
    &self.self_ref == recipient
  }

  /// Hands one message to the behaviour.
  pub(crate) fn deliver(&mut self, context: &mut ActorContext<'_>, message: Message) {
    self.behavior.receive(context, message);
  }
}
