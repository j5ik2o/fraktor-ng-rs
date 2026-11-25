// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
// actor-tests/src/test/scala-2.13+/org/apache/pekko/actor/ActorSystemScala213PlusSpec.scala
//
// Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f) has no such file.
// Upstream #2486 added the case "Scala's Using automatically terminates
// ActorSystem" twice: to this Scala 2.13+ file on the 1.x line (3be76d1be7),
// and to `actor-tests/src/test/scala/org/apache/pekko/actor/ActorSystemSpec.scala`
// on the 2.0 line (d5ee8a63e3), which no longer splits out Scala 2.13+ tests.
// The case body is the same in both. This test is that case; it belongs to the
// v2.0.0 ActorSystemSpec, whose other cases are not ported yet.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::executor::ShardBoundary;
use fraktor_actor::kernel::message::Message;

struct TrackedActor {
  released: Arc<AtomicBool>,
}

impl Actor for TrackedActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

impl Drop for TrackedActor {
  fn drop(&mut self) {
    self.released.store(true, Ordering::Relaxed);
  }
}

#[test]
fn leaving_scope_terminates_the_actor_system() {
  let boundary = Arc::new(ChannelBoundary::new());
  let released = Arc::new(AtomicBool::new(false));
  let actor = {
    let mut system = ActorSystem::new("ActorSystemScala213PlusSpec", Arc::clone(&boundary) as Arc<_>);
    let released_by_actor = Arc::clone(&released);
    let actor = system.actor_of(Props::new(move || TrackedActor { released: Arc::clone(&released_by_actor) }));
    actor.tell((), actor.clone());
    assert!(boundary.has_pending(), "the running system accepts messages");
    assert!(!released.load(Ordering::Relaxed), "the actor is still owned inside the scope");
    actor
  };

  assert!(released.load(Ordering::Relaxed), "leaving the scope releases the actor");
  // The boundary outlives the system, so the pending state is observable as soon as Drop returns.
  assert!(!boundary.has_pending(), "leaving the scope drains the system's boundary");
  actor.tell((), actor.clone());
  assert!(!boundary.has_pending(), "the terminated system's boundary rejects new messages");
}
