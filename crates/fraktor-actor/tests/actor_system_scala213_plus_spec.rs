// Ported from Pekko v1.7.1:
// actor-tests/src/test/scala-2.13+/org/apache/pekko/actor/ActorSystemScala213PlusSpec.scala

use std::sync::Arc;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::executor::ShardBoundary;
use fraktor_actor::kernel::message::Message;

struct NoopActor;

impl Actor for NoopActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

#[test]
fn leaving_scope_terminates_the_actor_system() {
  let boundary = Arc::new(ChannelBoundary::new());
  let actor = {
    let mut system = ActorSystem::new("ActorSystemScala213PlusSpec", Arc::clone(&boundary) as Arc<_>);
    let actor = system.actor_of(Props::new(|| NoopActor));
    actor.tell((), actor.clone());
    assert!(boundary.has_pending(), "the running system accepts messages");
    actor
  };

  // The boundary outlives the system, so the pending state is observable as soon as Drop returns.
  assert!(!boundary.has_pending(), "leaving the scope drains the system's boundary");
  actor.tell((), actor.clone());
  assert!(!boundary.has_pending(), "the terminated system's boundary rejects new messages");
}
