//! The actor that collects the messages a test expects.

use std::sync::mpsc::Sender;

use fraktor_actor::kernel::actor::Actor;
use fraktor_actor::kernel::actor::ActorContext;
use fraktor_actor::kernel::message::Message;

/// Forwards every message it receives to the queue of the test kit.
///
/// Mirrors the observable part of `org.apache.pekko.testkit.TestKit.TestActor`:
/// it appends each received message to the queue that `expectMsg` reads. Pekko
/// also records the sender of each message for `lastSender` and the
/// `TestActor` message sub-protocol; both belong to G0c.
pub(crate) struct TestActor {
  queue: Sender<Message>,
}

impl TestActor {
  pub(crate) fn new(queue: Sender<Message>) -> Self {
    Self { queue }
  }
}

impl Actor for TestActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    let _ = self.queue.send(message);
  }
}
