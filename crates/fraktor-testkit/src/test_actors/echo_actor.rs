//! An actor that sends back the messages it receives.

use fraktor_actor::kernel::actor::Actor;
use fraktor_actor::kernel::actor::ActorContext;
use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::message::Message;

/// Sends each received message back to its sender, unchanged.
///
/// Mirrors `org.apache.pekko.testkit.TestActors.EchoActor`.
pub struct EchoActor;

impl EchoActor {
  /// Returns the recipe that starts an `EchoActor`.
  pub fn props() -> Props {
    Props::new(|| EchoActor)
  }
}

impl Actor for EchoActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let sender = context.sender();
    context.tell(&sender, message);
  }
}
