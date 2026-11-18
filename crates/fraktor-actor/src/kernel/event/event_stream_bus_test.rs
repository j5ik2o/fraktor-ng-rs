use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::event::EventClass;
use crate::kernel::event::EventStreamBus;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;

struct QueueBoundary {
  queue: Mutex<VecDeque<Envelope>>,
}

impl QueueBoundary {
  fn new() -> Self {
    Self { queue: Mutex::new(VecDeque::new()) }
  }

  fn len(&self) -> usize {
    self.queue.lock().unwrap().len()
  }
}

impl ShardBoundary for QueueBoundary {
  fn post(&self, envelope: Envelope) {
    self.queue.lock().unwrap().push_back(envelope);
  }

  fn poll(&self) -> Option<Envelope> {
    self.queue.lock().unwrap().pop_front()
  }

  fn poll_within(&self, _max: Duration) -> Option<Envelope> {
    self.poll()
  }

  fn has_pending(&self) -> bool {
    !self.queue.lock().unwrap().is_empty()
  }

  fn close(&self) {}
}

fn subscriber(boundary: Arc<dyn ShardBoundary>, path: &str) -> ActorRef {
  let path = ActorPath::from_string(path).expect("the subscriber path parses");
  ActorRef::new(0, 0, Arc::new(path), boundary)
}

// The forwarder must keep reaching its subscribers only while the subscription
// table is alive. A dropped table is the state the forwarder hits when the
// stream is gone, and the weak handle must turn that into a silent no-op
// instead of a panic or a delivery. Both halves are observed on the same
// weak handle so a handle that never delivers cannot pass the pair.
#[test]
fn a_weak_handle_delivers_while_the_table_is_alive_and_stays_silent_after_it_is_dropped() {
  let boundary = Arc::new(QueueBoundary::new());
  let bus = EventStreamBus::new();
  let weak = bus.downgrade();
  let target = subscriber(Arc::clone(&boundary) as Arc<dyn ShardBoundary>, "pekko://weak-handle/sys/user/probe");
  bus.subscribe(Some(target), EventClass::any()).expect("the target is not null");

  weak.publish(42_u32);
  assert_eq!(boundary.len(), 1, "the weak handle delivers exactly one envelope while the table is alive");

  drop(bus);

  let delivered_before = boundary.len();
  weak.publish(43_u32);
  assert_eq!(
    boundary.len(),
    delivered_before,
    "a delivery through the weak handle is dropped once the table is gone"
  );
}
