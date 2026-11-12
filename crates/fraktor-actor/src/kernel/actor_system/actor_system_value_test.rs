//! Unit tests for [`ActorSystem`](super::ActorSystem).

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use core::time::Duration;

use std::sync::Mutex;

use crate::kernel::actor::Actor;
use crate::kernel::actor::ActorContext;
use crate::kernel::actor::Props;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;
use crate::kernel::message::Message;

use super::ActorSystem;
use crate::adaptor::ChannelBoundary;

struct QueueBoundary {
  queue: Mutex<VecDeque<Envelope>>,
}

impl QueueBoundary {
  fn new() -> Self {
    Self { queue: Mutex::new(VecDeque::new()) }
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
}

fn system(name: &str) -> ActorSystem {
  ActorSystem::new(name, Arc::new(QueueBoundary::new()))
}

struct NoOp;

impl Actor for NoOp {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

struct Sink {
  received: Arc<Mutex<Vec<&'static str>>>,
}

impl Actor for Sink {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if let Some(text) = message.downcast_ref::<&'static str>() {
      self.received.lock().unwrap().push(text);
    }
  }
}

struct TargetEnvelope {
  target: ActorRef,
  text: &'static str,
}

struct ForwardTo {
  target: ActorRef,
}

struct Relay;

impl Actor for Relay {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(target) = message.downcast_ref::<TargetEnvelope>() {
      context.tell(&target.target, Message::new(target.text));
    }
    if let Some(forward) = message.downcast_ref::<ForwardTo>() {
      forward.target.forward(Message::new("forwarded"), context);
    }
  }
}

struct EndlessBoundary {
  polls: AtomicUsize,
  sink: Arc<Mutex<Vec<&'static str>>>,
  cap: usize,
  target: Mutex<Option<ActorRef>>,
}

impl EndlessBoundary {
  fn new(sink: Arc<Mutex<Vec<&'static str>>>, cap: usize) -> Self {
    Self { polls: AtomicUsize::new(0), sink, cap, target: Mutex::new(None) }
  }
}

impl ShardBoundary for EndlessBoundary {
  fn post(&self, _envelope: Envelope) {}

  fn poll(&self) -> Option<Envelope> {
    let polls = self.polls.fetch_add(1, Ordering::SeqCst);
    if self.sink.lock().unwrap().is_empty() && polls < self.cap {
      let target = self.target.lock().unwrap().clone();
      match target {
        | Some(target) => Some(Envelope::new(target.clone(), Message::new("queued"), target)),
        | None => None,
      }
    } else {
      None
    }
  }

  fn poll_within(&self, _max: Duration) -> Option<Envelope> {
    self.poll()
  }
}

struct Recorder {
  seen: Arc<Mutex<Vec<ActorRef>>>,
}

impl Actor for Recorder {
  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    self.seen.lock().unwrap().push(context.sender());
  }
}

#[test]
fn starts_running_with_the_given_name() {
  let system = system("PekkoCustomSpec");

  assert_eq!(system.name(), "PekkoCustomSpec");
  assert!(!system.is_terminated());
}

#[test]
fn terminates_on_request() {
  let mut system = system("PekkoCustomSpec");

  system.terminate();

  assert!(system.is_terminated());
}

#[test]
fn terminating_twice_is_a_no_op() {
  let mut system = system("PekkoCustomSpec");

  system.terminate();
  system.terminate();

  assert!(system.is_terminated());
}

#[test]
fn creates_a_distinct_handle_for_each_actor() {
  let mut system = system("PekkoCustomSpec");

  let first = system.actor_of(Props::new(|| NoOp));
  let second = system.actor_of(Props::new(|| NoOp));

  assert_ne!(first, second);
}

#[test]
fn a_cloned_handle_equals_the_original() {
  let mut system = system("PekkoCustomSpec");

  let handle = system.actor_of(Props::new(|| NoOp));
  let copy = handle.clone();

  assert_eq!(copy, handle);
}

#[test]
fn run_until_idle_delivers_a_message_sent_from_the_test_thread() {
  let received = Arc::new(Mutex::new(Vec::new()));
  let observer = Arc::clone(&received);
  let mut system = system("PekkoCustomSpec");
  let sink = system.actor_of(Props::new(move || Sink { received: Arc::clone(&observer) }));

  sink.tell("hello", sink.clone());
  system.run_until_idle();

  assert_eq!(*received.lock().unwrap(), alloc::vec!["hello"]);
}

#[test]
fn a_message_to_another_actor_of_the_same_shard_stays_local() {
  let received = Arc::new(Mutex::new(Vec::new()));
  let observer = Arc::clone(&received);
  let mut system = system("PekkoCustomSpec");
  let relay = system.actor_of(Props::new(|| Relay));
  let sink = system.actor_of(Props::new(move || Sink { received: Arc::clone(&observer) }));

  relay.tell(TargetEnvelope { target: sink.clone(), text: "hello" }, relay.clone());
  system.run_until_idle();

  assert_eq!(*received.lock().unwrap(), alloc::vec!["hello"]);
}

#[test]
fn terminate_stops_delivery() {
  let received = Arc::new(Mutex::new(Vec::new()));
  let observer = Arc::clone(&received);
  let mut system = system("PekkoCustomSpec");
  let sink = system.actor_of(Props::new(move || Sink { received: Arc::clone(&observer) }));

  system.terminate();
  sink.tell("hello", sink.clone());
  system.run_until_idle();

  assert!(received.lock().unwrap().is_empty());
}

#[test]
fn forward_keeps_the_original_sender() {
  let seen = Arc::new(Mutex::new(Vec::new()));
  let recorded = Arc::clone(&seen);
  let mut system = system("PekkoCustomSpec");
  let sender = system.actor_of(Props::new(|| NoOp));
  let recorder = system.actor_of(Props::new(move || Recorder { seen: Arc::clone(&recorded) }));
  let relay = system.actor_of(Props::new(|| Relay));

  relay.tell(ForwardTo { target: recorder.clone() }, sender.clone());
  system.run_until_idle();

  assert_eq!(*seen.lock().unwrap(), alloc::vec![sender]);
}

#[test]
fn first_actors_of_two_systems_are_distinct() {
  let mut first_system = system("PekkoCustomSpec");
  let mut second_system = system("PekkoCustomSpec");

  let first = first_system.actor_of(Props::new(|| NoOp));
  let second = second_system.actor_of(Props::new(|| NoOp));

  assert_ne!(first, second);
}

#[test]
fn dropping_the_system_releases_the_boundary_and_its_queued_messages() {
  let boundary = Arc::new(ChannelBoundary::new());
  let mut system = ActorSystem::new("ChannelBoundarySpec", Arc::clone(&boundary) as Arc<_>);
  let recipient = system.actor_of(Props::new(|| NoOp));
  let weak = Arc::downgrade(&boundary);

  recipient.tell("hello", recipient.clone());
  assert!(weak.upgrade().is_some());

  drop(system);
  drop(recipient);
  drop(boundary);

  assert!(weak.upgrade().is_none());
}

#[test]
#[should_panic(expected = "terminat")]
fn actor_of_after_terminate_is_rejected() {
  let mut system = system("PekkoCustomSpec");

  system.terminate();
  system.actor_of(Props::new(|| NoOp));
}

#[test]
fn run_until_idle_delivers_from_the_boundary_while_the_queue_keeps_filling() {
  let received = Arc::new(Mutex::new(Vec::new()));
  let observer = Arc::clone(&received);
  let boundary = Arc::new(EndlessBoundary::new(Arc::clone(&received), 1000));
  let mut system = ActorSystem::new("PekkoCustomSpec", Arc::clone(&boundary) as Arc<_>);
  let sink = system.actor_of(Props::new(move || Sink { received: Arc::clone(&observer) }));
  *boundary.target.lock().unwrap() = Some(sink.clone());

  system.run_until_idle();

  assert_eq!(*received.lock().unwrap(), alloc::vec!["queued"]);
  assert!(boundary.polls.load(Ordering::SeqCst) < 1000, "the sink was starved by the boundary");
}
