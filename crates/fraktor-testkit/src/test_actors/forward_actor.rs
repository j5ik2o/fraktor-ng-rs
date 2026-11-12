//! An actor that forwards the messages it receives to a target.

use fraktor_actor::kernel::actor::Actor;
use fraktor_actor::kernel::actor::ActorContext;
use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;

/// Forwards each received message to a target, keeping the original sender.
///
/// Mirrors `org.apache.pekko.testkit.TestActors.ForwardActor`.
pub struct ForwardActor {
  target: ActorRef,
}

impl ForwardActor {
  /// Returns the recipe that starts a `ForwardActor` towards `target`.
  pub fn props(target: ActorRef) -> Props {
    Props::new(move || ForwardActor { target: target.clone() })
  }
}

impl Actor for ForwardActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    self.target.forward(message, context);
  }
}
