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
use crate::kernel::actor::Terminated;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;
use crate::kernel::message::Message;

use super::ActorSystem;
use super::SystemName;
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

  fn has_pending(&self) -> bool {
    !self.queue.lock().unwrap().is_empty()
  }

  fn close(&self) {}
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

  fn has_pending(&self) -> bool {
    false
  }

  fn close(&self) {}
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
fn terminating_then_telling_then_dropping_every_handle_releases_the_boundary() {
  let boundary = Arc::new(ChannelBoundary::new());
  let mut system = ActorSystem::new("ChannelBoundarySpec", Arc::clone(&boundary) as Arc<_>);
  let recipient = system.actor_of(Props::new(|| NoOp));
  let weak = Arc::downgrade(&boundary);

  system.terminate();
  recipient.tell("hello", recipient.clone());

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

struct LocalSink {
  received: Arc<Mutex<Vec<&'static str>>>,
}

impl Actor for LocalSink {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if let Some(text) = message.downcast_ref::<&'static str>() {
      self.received.lock().unwrap().push(text);
    }
  }
}

/// Sends the first message with `tell` (which posts to the boundary) and the
/// second with `context.tell` (which stays on the shard-local queue). Both
/// address the same actor, so the mix must still arrive in send order.
struct MixedSender {
  by_tell: ActorRef,
  by_context: ActorRef,
}

impl Actor for MixedSender {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() != Some(&"go") {
      return;
    }
    self.by_tell.tell("first", context.self_ref());
    context.tell(&self.by_context, Message::new("second"));
  }
}

#[test]
fn a_local_message_from_one_sender_keeps_its_order_against_an_earlier_boundary_message() {
  let local_received = Arc::new(Mutex::new(Vec::new()));
  let local_observer = Arc::clone(&local_received);
  let mut system = system("PekkoCustomSpec");
  let local = system.actor_of(Props::new(move || LocalSink { received: Arc::clone(&local_observer) }));
  let sender = system.actor_of(Props::new({
    let local = local.clone();
    move || MixedSender { by_tell: local.clone(), by_context: local.clone() }
  }));

  sender.tell("go", sender.clone());
  system.run_until_idle();

  assert_eq!(*local_received.lock().unwrap(), alloc::vec!["first", "second"]);
}

struct StopWatcher {
  observed: Arc<Mutex<Vec<Terminated>>>,
  watched: Arc<Mutex<Option<ActorRef>>>,
}

impl Actor for StopWatcher {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if let Some(terminated) = message.downcast_ref::<Terminated>() {
      if self.watched.lock().unwrap().as_ref() == Some(terminated.target()) {
        self.observed.lock().unwrap().push(terminated.clone());
      }
    }
  }
}

struct FailingChild {
  failures: Arc<AtomicUsize>,
}

impl Actor for FailingChild {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {
    self.failures.fetch_add(1, Ordering::SeqCst);
    panic!("simulated failure");
  }
}

#[test]
fn a_child_started_with_the_stopping_strategy_stops_and_tells_its_watcher() {
  use crate::adaptor::UnwindCatcher;
  use crate::kernel::supervision::SupervisorStrategy;

  let observed = Arc::new(Mutex::new(Vec::<Terminated>::new()));
  let watched = Arc::new(Mutex::new(None));
  let failures = Arc::new(AtomicUsize::new(0));
  let boundary: Arc<dyn ShardBoundary> = Arc::new(QueueBoundary::new());
  let mut system = ActorSystem::with_failure_catcher("PekkoCustomSpec", boundary, Arc::new(UnwindCatcher));
  let watcher = system.actor_of(Props::new({
    let observed = Arc::clone(&observed);
    let watched = Arc::clone(&watched);
    move || StopWatcher { observed: Arc::clone(&observed), watched: Arc::clone(&watched) }
  }));
  let parent = system.actor_of(Props::new(|| NoOp));
  let child = system.downgrade().spawn_child(
    &parent,
    Props::new({
      let failures = Arc::clone(&failures);
      move || FailingChild { failures: Arc::clone(&failures) }
    }),
    Some("child"),
    SupervisorStrategy::stopping_strategy(),
  );
  *watched.lock().unwrap() = Some(child.clone());
  system.watch(&watcher, &child);

  child.tell("boom", watcher.clone());
  system.run_until_idle();

  let seen = observed.lock().unwrap();
  assert_eq!(seen.len(), 1, "the watcher did not receive exactly one Terminated");
  assert_eq!(seen[0].target(), &child);
  assert!(seen[0].existence_confirmed(), "a stopped child was watched before it stopped");
  assert_eq!(failures.load(Ordering::SeqCst), 1, "the stopping strategy let the child run again");
}

#[test]
fn a_function_ref_stops_running_its_handler_after_terminate() {
  let seen = Arc::new(Mutex::new(Vec::new()));
  let observer = Arc::clone(&seen);
  let mut system = system("PekkoCustomSpec");
  let test_actor = system.function_ref("testActor", move |_sender: &ActorRef, message: Message| {
    if let Some(text) = message.downcast_ref::<&'static str>() {
      observer.lock().unwrap().push(*text);
    }
  });

  test_actor.tell("before", test_actor.clone());
  let before = seen.lock().unwrap().len();

  system.terminate();
  test_actor.tell("after", test_actor.clone());

  assert_eq!(before, 1, "the handler did not run before the terminate");
  assert_eq!(*seen.lock().unwrap(), alloc::vec!["before"], "the handler ran after the terminate");
}

#[test]
#[should_panic(expected = "terminat")]
fn function_ref_after_terminate_is_rejected() {
  let mut system = system("PekkoCustomSpec");
  system.terminate();

  system.function_ref("testActor", |_sender: &ActorRef, _message: Message| {});
}

#[test]
#[should_panic(expected = "terminat")]
fn weak_spawn_child_after_terminate_is_rejected() {
  let mut system = system("PekkoCustomSpec");
  let parent = system.actor_of(Props::new(|| NoOp));
  let weak = system.downgrade();
  system.terminate();

  weak.spawn_child(
    &parent,
    Props::new(|| NoOp),
    Some("late"),
    crate::kernel::supervision::SupervisorStrategy::default_strategy(),
  );
}

#[test]
#[should_panic(expected = "terminat")]
fn weak_function_ref_after_terminate_is_rejected() {
  let mut system = system("PekkoCustomSpec");
  let weak = system.downgrade();
  system.terminate();

  weak.function_ref("late", |_sender: &ActorRef, _message: Message| {});
}

#[test]
fn watching_a_live_function_ref_sends_no_terminated() {
  let observed = Arc::new(Mutex::new(Vec::<Terminated>::new()));
  let recorder = Arc::clone(&observed);
  let mut system = system("PekkoCustomSpec");
  let watcher = system.function_ref("watcher", move |_sender: &ActorRef, message: Message| {
    if let Some(terminated) = message.downcast_ref::<Terminated>() {
      recorder.lock().unwrap().push(terminated.clone());
    }
  });
  let target = system.function_ref("target", |_sender: &ActorRef, _message: Message| {});

  system.watch(&watcher, &target);
  system.run_until_idle();

  assert!(observed.lock().unwrap().is_empty(), "a live function reference was reported as terminated");
}

#[test]
fn stopping_a_parent_stops_its_child_first_and_tells_the_child_watcher() {
  let order = Arc::new(Mutex::new(Vec::<&'static str>::new()));
  let child_seen = Arc::new(Mutex::new(Vec::<Terminated>::new()));
  let parent_seen = Arc::new(Mutex::new(Vec::<Terminated>::new()));
  let mut system = system("PekkoCustomSpec");

  let parent = system.actor_of(Props::new(|| NoOp));
  let child = system.downgrade().spawn_child(
    &parent,
    Props::new({
      let order = Arc::clone(&order);
      move || OrderedSink { order: Arc::clone(&order), label: "child delivered" }
    }),
    Some("child"),
    crate::kernel::supervision::SupervisorStrategy::default_strategy(),
  );

  let child_watcher = system.actor_of(Props::new({
    let order = Arc::clone(&order);
    let child_seen = Arc::clone(&child_seen);
    move || TerminatedSink { order: Arc::clone(&order), seen: Arc::clone(&child_seen), label: "child terminated" }
  }));
  let parent_watcher = system.actor_of(Props::new({
    let order = Arc::clone(&order);
    let parent_seen = Arc::clone(&parent_seen);
    move || TerminatedSink { order: Arc::clone(&order), seen: Arc::clone(&parent_seen), label: "parent terminated" }
  }));

  system.watch(&child_watcher, &child);
  system.watch(&parent_watcher, &parent);

  system.stop(&parent);
  child.tell("late", parent.clone());
  system.run_until_idle();

  assert_eq!(
    *order.lock().unwrap(),
    alloc::vec!["child terminated", "parent terminated"],
    "the notices arrived in the wrong order"
  );
  let child_notices = child_seen.lock().unwrap();
  assert_eq!(child_notices.len(), 1, "the child watcher was not told that the child stopped");
  assert!(child_notices[0].existence_confirmed());
  let parent_notices = parent_seen.lock().unwrap();
  assert_eq!(parent_notices.len(), 1, "the parent watcher was not told that the parent stopped");
  assert!(parent_notices[0].existence_confirmed());
}

struct OrderedSink {
  order: Arc<Mutex<Vec<&'static str>>>,
  label: &'static str,
}

#[test]
fn stopping_a_watched_function_ref_tells_its_watcher_and_stops_its_handler() {
  let observed = Arc::new(Mutex::new(Vec::<Terminated>::new()));
  let handled = Arc::new(AtomicUsize::new(0));
  let recorder = Arc::clone(&observed);
  let mut system = system("PekkoCustomSpec");
  let watcher = system.function_ref("watcher", move |_sender: &ActorRef, message: Message| {
    if let Some(terminated) = message.downcast_ref::<Terminated>() {
      recorder.lock().unwrap().push(terminated.clone());
    }
  });
  let counted = Arc::clone(&handled);
  let target = system.function_ref("target", move |_sender: &ActorRef, _message: Message| {
    counted.fetch_add(1, Ordering::SeqCst);
  });

  system.watch(&watcher, &target);
  system.run_until_idle();
  assert!(observed.lock().unwrap().is_empty(), "a live function reference was reported as terminated");

  system.stop(&target);
  system.run_until_idle();

  let notices = observed.lock().unwrap();
  assert_eq!(notices.len(), 1, "the watcher of a stopped function reference was not told");
  assert_eq!(notices[0].target(), &target);
  assert!(notices[0].existence_confirmed(), "a watched function reference was stopped after the watch");
  assert!(!notices[0].address_terminated());
  drop(notices);

  target.tell("after", target.clone());
  assert_eq!(handled.load(Ordering::SeqCst), 0, "a stopped function reference still ran its handler");
}

#[test]
fn stopping_a_function_ref_stops_its_child_first_and_tells_both_watchers() {
  let order = Arc::new(Mutex::new(Vec::<&'static str>::new()));
  let child_seen = Arc::new(Mutex::new(Vec::<Terminated>::new()));
  let target_seen = Arc::new(Mutex::new(Vec::<Terminated>::new()));
  let mut system = system("PekkoCustomSpec");

  let target = system.function_ref("target", |_sender: &ActorRef, _message: Message| {});
  let child = system.downgrade().spawn_child(
    &target,
    Props::new(|| NoOp),
    Some("child"),
    crate::kernel::supervision::SupervisorStrategy::default_strategy(),
  );

  let child_watcher = system.actor_of(Props::new({
    let order = Arc::clone(&order);
    let child_seen = Arc::clone(&child_seen);
    move || TerminatedSink { order: Arc::clone(&order), seen: Arc::clone(&child_seen), label: "child terminated" }
  }));
  let target_watcher = system.actor_of(Props::new({
    let order = Arc::clone(&order);
    let target_seen = Arc::clone(&target_seen);
    move || TerminatedSink { order: Arc::clone(&order), seen: Arc::clone(&target_seen), label: "target terminated" }
  }));

  system.watch(&child_watcher, &child);
  system.watch(&target_watcher, &target);

  system.stop(&target);
  system.run_until_idle();

  assert_eq!(
    *order.lock().unwrap(),
    alloc::vec!["child terminated", "target terminated"],
    "a function reference stopped before its child"
  );
  let child_notices = child_seen.lock().unwrap();
  assert_eq!(child_notices.len(), 1, "the child watcher was not told that the child stopped");
  assert!(child_notices[0].existence_confirmed());
  let target_notices = target_seen.lock().unwrap();
  assert_eq!(target_notices.len(), 1, "the watcher of the function reference was not told");
  assert!(target_notices[0].existence_confirmed());
}

impl Actor for OrderedSink {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {
    self.order.lock().unwrap().push(self.label);
  }
}

struct TerminatedSink {
  order: Arc<Mutex<Vec<&'static str>>>,
  seen: Arc<Mutex<Vec<Terminated>>>,
  label: &'static str,
}

impl Actor for TerminatedSink {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if let Some(terminated) = message.downcast_ref::<Terminated>() {
      self.order.lock().unwrap().push(self.label);
      self.seen.lock().unwrap().push(terminated.clone());
    }
  }
}

struct PanicsOnRestart;

impl Actor for PanicsOnRestart {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("simulated failure");
    }
  }

  fn post_restart(&mut self) {
    panic!("simulated failure of postRestart");
  }
}

/// Fails the first message, so the default strategy re-creates it; the recipe
/// passed to `Props` then panics on the next creation, which puts the failure
/// in the executor's restart path.
struct FailsFirstMessage;

impl Actor for FailsFirstMessage {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("simulated failure");
    }
  }
}

fn fails_first_message_or_panics_on_recreation(creations: Arc<AtomicUsize>) -> Props {
  Props::new(move || {
    let created = creations.fetch_add(1, Ordering::SeqCst);
    if created >= 1 {
      panic!("simulated failure of the recipe");
    }
    FailsFirstMessage
  })
}

#[test]
fn a_child_whose_post_restart_fails_is_stopped() {
  let observed = Arc::new(Mutex::new(Vec::<Terminated>::new()));
  let watched = Arc::new(Mutex::new(None));
  let boundary: Arc<dyn ShardBoundary> = Arc::new(QueueBoundary::new());
  let mut system =
    ActorSystem::with_failure_catcher("PekkoCustomSpec", boundary, Arc::new(crate::adaptor::UnwindCatcher));
  let watcher = system.actor_of(Props::new({
    let observed = Arc::clone(&observed);
    let watched = Arc::clone(&watched);
    move || StopWatcher { observed: Arc::clone(&observed), watched: Arc::clone(&watched) }
  }));
  let parent = system.actor_of(Props::new(|| NoOp));
  let child = system.downgrade().spawn_child(
    &parent,
    Props::new(|| PanicsOnRestart),
    Some("child"),
    crate::kernel::supervision::SupervisorStrategy::default_strategy(),
  );
  *watched.lock().unwrap() = Some(child.clone());
  system.watch(&watcher, &child);

  child.tell("boom", watcher.clone());
  let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| system.run_until_idle()));

  assert!(outcome.is_ok(), "the failure of postRestart escaped the catcher");
  let seen = observed.lock().unwrap();
  assert_eq!(seen.len(), 1, "the watcher did not receive exactly one Terminated");
  assert!(seen[0].existence_confirmed(), "a stopping failure was reported as a late watch");
}

#[test]
fn a_child_whose_factory_fails_on_restart_is_stopped() {
  let creations = Arc::new(AtomicUsize::new(0));
  let observed = Arc::new(Mutex::new(Vec::<Terminated>::new()));
  let watched = Arc::new(Mutex::new(None));
  let boundary: Arc<dyn ShardBoundary> = Arc::new(QueueBoundary::new());
  let mut system =
    ActorSystem::with_failure_catcher("PekkoCustomSpec", boundary, Arc::new(crate::adaptor::UnwindCatcher));
  let watcher = system.actor_of(Props::new({
    let observed = Arc::clone(&observed);
    let watched = Arc::clone(&watched);
    move || StopWatcher { observed: Arc::clone(&observed), watched: Arc::clone(&watched) }
  }));
  let parent = system.actor_of(Props::new(|| NoOp));
  let child = system.downgrade().spawn_child(
    &parent,
    fails_first_message_or_panics_on_recreation(Arc::clone(&creations)),
    Some("child"),
    crate::kernel::supervision::SupervisorStrategy::default_strategy(),
  );
  *watched.lock().unwrap() = Some(child.clone());
  system.watch(&watcher, &child);

  child.tell("boom", watcher.clone());
  let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| system.run_until_idle()));

  assert!(outcome.is_ok(), "the failure of the recipe escaped the catcher");
  assert_eq!(creations.load(Ordering::SeqCst), 2, "the child was not re-created once after the failed message");
  let seen = observed.lock().unwrap();
  assert_eq!(seen.len(), 1, "the watcher did not receive exactly one Terminated");
  assert!(seen[0].existence_confirmed(), "a stopping failure was reported as a late watch");
}

/// Counts its own `post_stop`, so a restart that fails can show how often the
/// stopped old instance ran it. The default `pre_restart` runs `post_stop`, so a
/// stop path that runs it once more makes the old instance run it twice.
struct CountsPostStopOnRestartFailure {
  post_stops: Arc<AtomicUsize>,
}

impl Actor for CountsPostStopOnRestartFailure {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("simulated failure");
    }
  }

  fn post_stop(&mut self) {
    self.post_stops.fetch_add(1, Ordering::SeqCst);
  }
}

struct CountsPostStopAndPanicsOnRestart {
  post_stops: Arc<AtomicUsize>,
}

impl Actor for CountsPostStopAndPanicsOnRestart {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("simulated failure");
    }
  }

  fn post_restart(&mut self) {
    panic!("simulated failure of postRestart");
  }

  fn post_stop(&mut self) {
    self.post_stops.fetch_add(1, Ordering::SeqCst);
  }
}

#[test]
fn a_child_whose_factory_fails_on_restart_runs_post_stop_once() {
  let creations = Arc::new(AtomicUsize::new(0));
  let post_stops = Arc::new(AtomicUsize::new(0));
  let observed = Arc::new(Mutex::new(Vec::<Terminated>::new()));
  let watched = Arc::new(Mutex::new(None));
  let boundary: Arc<dyn ShardBoundary> = Arc::new(QueueBoundary::new());
  let mut system =
    ActorSystem::with_failure_catcher("PekkoCustomSpec", boundary, Arc::new(crate::adaptor::UnwindCatcher));
  let watcher = system.actor_of(Props::new({
    let observed = Arc::clone(&observed);
    let watched = Arc::clone(&watched);
    move || StopWatcher { observed: Arc::clone(&observed), watched: Arc::clone(&watched) }
  }));
  let parent = system.actor_of(Props::new(|| NoOp));
  let child = system.downgrade().spawn_child(
    &parent,
    Props::new({
      let creations = Arc::clone(&creations);
      let post_stops = Arc::clone(&post_stops);
      move || {
        let created = creations.fetch_add(1, Ordering::SeqCst);
        if created >= 1 {
          panic!("simulated failure of the recipe");
        }
        CountsPostStopOnRestartFailure { post_stops: Arc::clone(&post_stops) }
      }
    }),
    Some("child"),
    crate::kernel::supervision::SupervisorStrategy::default_strategy(),
  );
  *watched.lock().unwrap() = Some(child.clone());
  system.watch(&watcher, &child);

  child.tell("boom", watcher.clone());
  let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| system.run_until_idle()));

  assert!(outcome.is_ok(), "the failure of the recipe escaped the catcher");
  assert_eq!(creations.load(Ordering::SeqCst), 2, "the child was not re-created once after the failed message");
  assert_eq!(
    post_stops.load(Ordering::SeqCst),
    1,
    "the old instance ran post_stop more than once when the recipe failed on restart"
  );
  let seen = observed.lock().unwrap();
  assert_eq!(seen.len(), 1, "the watcher did not receive exactly one Terminated");
  assert!(seen[0].existence_confirmed(), "a stopping failure was reported as a late watch");
}

#[test]
fn a_child_whose_post_restart_fails_runs_post_stop_once() {
  let post_stops = Arc::new(AtomicUsize::new(0));
  let observed = Arc::new(Mutex::new(Vec::<Terminated>::new()));
  let watched = Arc::new(Mutex::new(None));
  let boundary: Arc<dyn ShardBoundary> = Arc::new(QueueBoundary::new());
  let mut system =
    ActorSystem::with_failure_catcher("PekkoCustomSpec", boundary, Arc::new(crate::adaptor::UnwindCatcher));
  let watcher = system.actor_of(Props::new({
    let observed = Arc::clone(&observed);
    let watched = Arc::clone(&watched);
    move || StopWatcher { observed: Arc::clone(&observed), watched: Arc::clone(&watched) }
  }));
  let parent = system.actor_of(Props::new(|| NoOp));
  let child = system.downgrade().spawn_child(
    &parent,
    Props::new({
      let post_stops = Arc::clone(&post_stops);
      move || CountsPostStopAndPanicsOnRestart { post_stops: Arc::clone(&post_stops) }
    }),
    Some("child"),
    crate::kernel::supervision::SupervisorStrategy::default_strategy(),
  );
  *watched.lock().unwrap() = Some(child.clone());
  system.watch(&watcher, &child);

  child.tell("boom", watcher.clone());
  let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| system.run_until_idle()));

  assert!(outcome.is_ok(), "the failure of postRestart escaped the catcher");
  assert_eq!(
    post_stops.load(Ordering::SeqCst),
    1,
    "the old instance ran post_stop more than once when postRestart failed on restart"
  );
  let seen = observed.lock().unwrap();
  assert_eq!(seen.len(), 1, "the watcher did not receive exactly one Terminated");
  assert!(seen[0].existence_confirmed(), "a stopping failure was reported as a late watch");
}

/// Counts the creations of its recipe and its own `post_stop`, and panics in
/// `pre_start`, so a child that never starts can show how often the executor
/// tried and whether it stopped it as one that never started.
struct CountsStartupAndPostStop {
  post_stops: Arc<AtomicUsize>,
}

impl Actor for CountsStartupAndPostStop {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn pre_start(&mut self) {
    panic!("simulated failure of preStart");
  }

  fn post_stop(&mut self) {
    self.post_stops.fetch_add(1, Ordering::SeqCst);
  }
}

#[test]
fn a_child_whose_pre_start_always_fails_stops_under_the_default_strategy() {
  let creations = Arc::new(AtomicUsize::new(0));
  let post_stops = Arc::new(AtomicUsize::new(0));
  let observed = Arc::new(Mutex::new(Vec::<Terminated>::new()));
  let watched = Arc::new(Mutex::new(None));
  let boundary: Arc<dyn ShardBoundary> = Arc::new(QueueBoundary::new());
  let mut system =
    ActorSystem::with_failure_catcher("PekkoCustomSpec", boundary, Arc::new(crate::adaptor::UnwindCatcher));
  let watcher = system.actor_of(Props::new({
    let observed = Arc::clone(&observed);
    let watched = Arc::clone(&watched);
    move || StopWatcher { observed: Arc::clone(&observed), watched: Arc::clone(&watched) }
  }));
  let child = system.actor_of(Props::new({
    let creations = Arc::clone(&creations);
    let post_stops = Arc::clone(&post_stops);
    move || {
      creations.fetch_add(1, Ordering::SeqCst);
      CountsStartupAndPostStop { post_stops: Arc::clone(&post_stops) }
    }
  }));
  *watched.lock().unwrap() = Some(child.clone());
  system.watch(&watcher, &child);
  system.run_until_idle();

  assert_eq!(creations.load(Ordering::SeqCst), 1, "the child was created more than once for a failed start");
  assert_eq!(post_stops.load(Ordering::SeqCst), 0, "a child that never started ran post_stop");
  let seen = observed.lock().unwrap();
  assert_eq!(seen.len(), 1, "the watcher did not receive exactly one Terminated");
}

#[test]
fn a_child_told_to_kill_stops_under_the_default_strategy() {
  let received = Arc::new(Mutex::new(Vec::<&'static str>::new()));
  let pre_restarts = Arc::new(AtomicUsize::new(0));
  let post_restarts = Arc::new(AtomicUsize::new(0));
  let post_stops = Arc::new(AtomicUsize::new(0));
  let observed = Arc::new(Mutex::new(Vec::<Terminated>::new()));
  let watched = Arc::new(Mutex::new(None));
  let boundary: Arc<dyn ShardBoundary> = Arc::new(QueueBoundary::new());
  let mut system =
    ActorSystem::with_failure_catcher("PekkoCustomSpec", boundary, Arc::new(crate::adaptor::UnwindCatcher));
  let watcher = system.actor_of(Props::new({
    let observed = Arc::clone(&observed);
    let watched = Arc::clone(&watched);
    move || StopWatcher { observed: Arc::clone(&observed), watched: Arc::clone(&watched) }
  }));
  let child = system.actor_of(Props::new({
    let received = Arc::clone(&received);
    let pre_restarts = Arc::clone(&pre_restarts);
    let post_restarts = Arc::clone(&post_restarts);
    let post_stops = Arc::clone(&post_stops);
    move || CountsLifecycleHooks {
      received: Arc::clone(&received),
      pre_restarts: Arc::clone(&pre_restarts),
      post_restarts: Arc::clone(&post_restarts),
      post_stops: Arc::clone(&post_stops),
    }
  }));
  *watched.lock().unwrap() = Some(child.clone());
  system.watch(&watcher, &child);

  child.tell("Kill", watcher.clone());
  system.run_until_idle();
  child.tell("ping", watcher.clone());
  let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| system.run_until_idle()));

  assert!(outcome.is_ok(), "the kill escaped the catcher");
  assert_eq!(pre_restarts.load(Ordering::SeqCst), 0, "a killed child ran pre_restart");
  assert_eq!(post_restarts.load(Ordering::SeqCst), 0, "a killed child ran post_restart");
  assert_eq!(post_stops.load(Ordering::SeqCst), 1, "a killed child did not run post_stop exactly once");
  assert!(received.lock().unwrap().is_empty(), "a killed child answered a message sent after the kill");
  let seen = observed.lock().unwrap();
  assert_eq!(seen.len(), 1, "the watcher did not receive exactly one Terminated");
  assert!(seen[0].existence_confirmed(), "a stopping failure was reported as a late watch");
}

struct CountsLifecycleHooks {
  received: Arc<Mutex<Vec<&'static str>>>,
  pre_restarts: Arc<AtomicUsize>,
  post_restarts: Arc<AtomicUsize>,
  post_stops: Arc<AtomicUsize>,
}

impl Actor for CountsLifecycleHooks {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if let Some(text) = message.downcast_ref::<&'static str>() {
      self.received.lock().unwrap().push(text);
    }
  }

  fn pre_restart(&mut self, _reason: &(dyn core::any::Any + Send)) {
    self.pre_restarts.fetch_add(1, Ordering::SeqCst);
  }

  fn post_restart(&mut self) {
    self.post_restarts.fetch_add(1, Ordering::SeqCst);
  }

  fn post_stop(&mut self) {
    self.post_stops.fetch_add(1, Ordering::SeqCst);
  }
}

impl SystemName {
  fn parse_for_test(text: &str) -> Option<Self> {
    Self::new(text).ok()
  }
}

#[test]
fn system_name_rejects_an_empty_string() {
  assert!(SystemName::new("").is_err(), "an empty system name was accepted");
}

#[test]
fn system_name_keeps_a_non_empty_string() {
  assert_eq!(SystemName::new("PekkoSpecSpec").unwrap().as_str(), "PekkoSpecSpec");
}

#[test]
fn system_name_parses_every_non_empty_string() {
  assert_eq!(SystemName::parse_for_test("PekkoSpecSpec"), SystemName::new("PekkoSpecSpec").ok());
  assert_eq!(SystemName::parse_for_test(""), None, "an empty system name parsed successfully");
}
