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
fn watching_an_already_stopped_actor_of_this_shard_still_reports_it_once() {
  // A stopped reference of this shard must keep the immediate notice: the
  // silent watch is only for the detached/no-op reference the UidClash case
  // builds, so an ordinary stop-and-watch does not lose its Terminated.
  let observed = Arc::new(Mutex::new(Vec::<Terminated>::new()));
  let watched = Arc::new(Mutex::new(None));
  let mut system = system("PekkoCustomSpec");
  let watcher = system.actor_of(Props::new({
    let observed = Arc::clone(&observed);
    let watched = Arc::clone(&watched);
    move || StopWatcher { observed: Arc::clone(&observed), watched: Arc::clone(&watched) }
  }));
  let target = system.actor_of(Props::new(|| NoOp));

  system.stop(&target);
  system.run_until_idle();
  *watched.lock().unwrap() = Some(target.clone());

  system.watch(&watcher, &target);
  system.run_until_idle();

  let seen = observed.lock().unwrap();
  assert_eq!(seen.len(), 1, "a stopped actor of this shard was not reported exactly once");
  assert!(!seen[0].existence_confirmed(), "a watch of an already stopped actor was reported as a confirmed existence");
  assert_eq!(seen[0].target(), &target, "the notice must name the stopped actor");
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

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
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

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
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

  fn pre_start(&mut self, _context: &mut ActorContext<'_>) {
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

  fn pre_restart(&mut self, _context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    self.pre_restarts.fetch_add(1, Ordering::SeqCst);
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.post_restarts.fetch_add(1, Ordering::SeqCst);
  }

  fn post_stop(&mut self) {
    self.post_stops.fetch_add(1, Ordering::SeqCst);
  }
}

/// Fails its first `failures` starts, then starts for real and answers strings.
struct FailsStartUntil {
  attempts: Arc<AtomicUsize>,
  failures: usize,
  received: Arc<Mutex<Vec<&'static str>>>,
}

impl Actor for FailsStartUntil {
  fn pre_start(&mut self, _context: &mut ActorContext<'_>) {
    let attempt = self.attempts.fetch_add(1, Ordering::SeqCst);
    if attempt < self.failures {
      panic!("simulated failure of preStart");
    }
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if let Some(text) = message.downcast_ref::<&'static str>() {
      self.received.lock().unwrap().push(text);
    }
  }
}

#[test]
fn a_start_failure_is_offered_to_the_decider() {
  let attempts = Arc::new(AtomicUsize::new(0));
  let received = Arc::new(Mutex::new(Vec::<&'static str>::new()));
  let boundary: Arc<dyn ShardBoundary> = Arc::new(QueueBoundary::new());
  let mut system =
    ActorSystem::with_failure_catcher("PekkoCustomSpec", boundary, Arc::new(crate::adaptor::UnwindCatcher));
  let parent = system.actor_of(Props::new(|| NoOp));
  let strategy =
    crate::kernel::supervision::SupervisorStrategy::one_for_one(3).with_decider(|_child, failure| match failure {
      | crate::kernel::supervision::FailureKind::Initialization => crate::kernel::supervision::Directive::Restart,
      | _ => crate::kernel::supervision::Directive::Stop,
    });
  let child = system.downgrade().spawn_child(
    &parent,
    Props::new({
      let attempts = Arc::clone(&attempts);
      let received = Arc::clone(&received);
      move || FailsStartUntil { attempts: Arc::clone(&attempts), failures: 1, received: Arc::clone(&received) }
    }),
    Some("child"),
    strategy,
  );
  assert_eq!(attempts.load(Ordering::SeqCst), 2, "the decider was not asked about the start failure");

  child.tell("hello", child.clone());
  system.run_until_idle();

  assert_eq!(*received.lock().unwrap(), alloc::vec!["hello"], "the child restarted by the decider did not answer");
}

#[test]
fn a_name_is_free_again_after_a_child_that_failed_its_start_stopped() {
  let boundary: Arc<dyn ShardBoundary> = Arc::new(QueueBoundary::new());
  let mut system =
    ActorSystem::with_failure_catcher("PekkoCustomSpec", boundary, Arc::new(crate::adaptor::UnwindCatcher));
  let parent = system.actor_of(Props::new(|| NoOp));
  let failed = system.downgrade().spawn_child(
    &parent,
    Props::new(|| CountsStartupAndPostStop { post_stops: Arc::new(AtomicUsize::new(0)) }),
    Some("bob"),
    crate::kernel::supervision::SupervisorStrategy::stopping_strategy(),
  );
  assert!(system.is_actor_terminated(&failed), "the child that failed its start is still alive");

  let replacement = system.downgrade().spawn_child(
    &parent,
    Props::new(|| NoOp),
    Some("bob"),
    crate::kernel::supervision::SupervisorStrategy::default_strategy(),
  );

  assert_eq!(replacement.path().name(), "bob");
  assert!(!system.is_actor_terminated(&replacement), "the replacement child is not alive");
}

#[test]
#[should_panic(expected = "Actor name [bob] is not unique!")]
fn weak_spawn_child_with_a_live_name_is_refused() {
  let mut system = system("PekkoCustomSpec");
  let parent = system.actor_of(Props::new(|| NoOp));
  let first = system.downgrade().spawn_child(
    &parent,
    Props::new(|| NoOp),
    Some("bob"),
    crate::kernel::supervision::SupervisorStrategy::default_strategy(),
  );
  assert!(!system.is_actor_terminated(&first));

  system.downgrade().spawn_child(
    &parent,
    Props::new(|| NoOp),
    Some("bob"),
    crate::kernel::supervision::SupervisorStrategy::default_strategy(),
  );
}

/// Asks to stop `victim` and then fails while handling the same message.
struct StopsAChildThenFails {
  victim: ActorRef,
}

impl Actor for StopsAChildThenFails {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      context.stop(&self.victim);
      panic!("simulated failure");
    }
  }
}

#[test]
fn a_stop_requested_before_a_failure_still_runs() {
  let boundary: Arc<dyn ShardBoundary> = Arc::new(QueueBoundary::new());
  let mut system =
    ActorSystem::with_failure_catcher("PekkoCustomSpec", boundary, Arc::new(crate::adaptor::UnwindCatcher));
  let victim = system.actor_of(Props::new(|| NoOp));
  let failing = system.actor_of(Props::new({
    let victim = victim.clone();
    move || StopsAChildThenFails { victim: victim.clone() }
  }));

  failing.tell("boom", failing.clone());
  let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| system.run_until_idle()));

  assert!(outcome.is_ok(), "the failure escaped the catcher");
  assert!(system.is_actor_terminated(&victim), "the stop requested before the failure was lost");
}

/// One recorded log event: its cause name and its message.
type RecordedEvent = (Option<alloc::string::String>, alloc::string::String);

/// Records the log events the executor publishes, so a test can count the
/// failures of a child-terminated hook.
struct RecordingSink {
  events: Arc<Mutex<Vec<RecordedEvent>>>,
}

impl crate::kernel::actor::Logging::EventSink for RecordingSink {
  fn publish(&self, event: &dyn crate::kernel::actor::Logging::LogEvent) {
    let cause = event.cause_name().map(alloc::string::String::from);
    self.events.lock().unwrap().push((cause, alloc::string::String::from(event.message())));
  }

  fn into_any(self: Arc<Self>) -> Arc<dyn core::any::Any + Send + Sync> {
    self
  }
}

fn with_catcher(name: &str) -> ActorSystem {
  let boundary: Arc<dyn ShardBoundary> = Arc::new(QueueBoundary::new());
  ActorSystem::with_failure_catcher(name, boundary, Arc::new(crate::adaptor::UnwindCatcher))
}

/// Creates a grandchild named "grand" and then fails its start, so the stop
/// path has to clean up the child the failed attempt left behind.
struct StartsGrandThenFails {
  grand: Arc<Mutex<Option<ActorRef>>>,
  post_stops: Arc<AtomicUsize>,
}

impl Actor for StartsGrandThenFails {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let grand = context.actor_of(Props::new(|| NoOp), "grand").expect("the grandchild name must be free");
    *self.grand.lock().unwrap() = Some(grand);
    panic!("simulated failure of preStart");
  }

  fn post_stop(&mut self) {
    self.post_stops.fetch_add(1, Ordering::SeqCst);
  }
}

/// Starts the failing child, then reuses its name once it has stopped.
struct StartsAFailingChild {
  hooks: Arc<AtomicUsize>,
  grand: Arc<Mutex<Option<ActorRef>>>,
  post_stops: Arc<AtomicUsize>,
  results: Arc<Mutex<Vec<&'static str>>>,
}

impl Actor for StartsAFailingChild {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(text) = message.downcast_ref::<&'static str>() else {
      return;
    };
    if *text == "spawn" {
      let grand = Arc::clone(&self.grand);
      let post_stops = Arc::clone(&self.post_stops);
      let created = context.actor_of(
        Props::new(move || StartsGrandThenFails { grand: Arc::clone(&grand), post_stops: Arc::clone(&post_stops) }),
        "boom",
      );
      self.results.lock().unwrap().push(if created.is_ok() { "spawned" } else { "refused" });
    } else if *text == "respawn" {
      match context.actor_of(Props::new(|| NoOp), "boom") {
        | Ok(kid) if !context.is_terminated(&kid) => self.results.lock().unwrap().push("reused-live"),
        | Ok(_) => self.results.lock().unwrap().push("reused-dead"),
        | Err(_) => self.results.lock().unwrap().push("blocked"),
      }
    }
  }

  fn supervisor_strategy(&self) -> crate::kernel::supervision::SupervisorStrategy {
    let hooks = Arc::clone(&self.hooks);
    crate::kernel::supervision::SupervisorStrategy::one_for_one(3).with_child_terminated(
      move |_context, _child, _siblings| {
        hooks.fetch_add(1, Ordering::SeqCst);
      },
    )
  }
}

#[test]
fn a_child_that_fails_its_start_stops_its_children_frees_its_name_and_runs_the_parent_hook() {
  let hooks = Arc::new(AtomicUsize::new(0));
  let grand = Arc::new(Mutex::new(None));
  let post_stops = Arc::new(AtomicUsize::new(0));
  let results = Arc::new(Mutex::new(Vec::new()));
  let mut system = with_catcher("PekkoCustomSpec");
  let parent = system.actor_of(Props::new({
    let hooks = Arc::clone(&hooks);
    let grand = Arc::clone(&grand);
    let post_stops = Arc::clone(&post_stops);
    let results = Arc::clone(&results);
    move || StartsAFailingChild {
      hooks: Arc::clone(&hooks),
      grand: Arc::clone(&grand),
      post_stops: Arc::clone(&post_stops),
      results: Arc::clone(&results),
    }
  }));

  parent.tell("spawn", parent.clone());
  system.run_until_idle();

  assert_eq!(*results.lock().unwrap(), alloc::vec!["spawned"]);
  let grand = grand.lock().unwrap().clone().expect("the failing start created a grandchild");
  assert!(system.is_actor_terminated(&grand), "the grandchild of the failed start is still alive");
  assert_eq!(hooks.load(Ordering::SeqCst), 1, "the parent hook did not run once for the stopped child");
  assert_eq!(post_stops.load(Ordering::SeqCst), 0, "a never-started instance ran post_stop");

  parent.tell("respawn", parent.clone());
  system.run_until_idle();
  assert_eq!(
    *results.lock().unwrap(),
    alloc::vec!["spawned", "reused-live"],
    "the name of the stopped child was not reusable"
  );
}

/// Starts a named worker on every start attempt and fails the first one.
struct StartsWorkerThenFailsOnce {
  attempts: Arc<AtomicUsize>,
  results: Arc<Mutex<Vec<&'static str>>>,
  workers: Arc<Mutex<Vec<ActorRef>>>,
}

impl Actor for StartsWorkerThenFailsOnce {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
    let worker = context.actor_of(Props::new(|| NoOp), "worker");
    self.results.lock().unwrap().push(if worker.is_ok() { "worker-spawned" } else { "worker-refused" });
    if let Ok(worker) = worker {
      self.workers.lock().unwrap().push(worker);
    }
    if attempt == 1 {
      panic!("simulated initialization failure");
    }
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

/// Starts the worker actor above on demand under a strategy that retries a
/// failed start.
struct SpawnsRestartingChild {
  attempts: Arc<AtomicUsize>,
  results: Arc<Mutex<Vec<&'static str>>>,
  workers: Arc<Mutex<Vec<ActorRef>>>,
}

impl Actor for SpawnsRestartingChild {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() != Some(&"start") {
      return;
    }
    let attempts = Arc::clone(&self.attempts);
    let results = Arc::clone(&self.results);
    let workers = Arc::clone(&self.workers);
    let started = context.actor_of(
      Props::new(move || StartsWorkerThenFailsOnce {
        attempts: Arc::clone(&attempts),
        results: Arc::clone(&results),
        workers: Arc::clone(&workers),
      }),
      "restarting",
    );
    self.results.lock().unwrap().push(if started.is_ok() { "started" } else { "refused" });
  }

  fn supervisor_strategy(&self) -> crate::kernel::supervision::SupervisorStrategy {
    crate::kernel::supervision::SupervisorStrategy::one_for_one_restarting_initialization_failures(3)
  }
}

#[test]
fn a_retried_start_does_not_keep_the_children_of_the_failed_attempt() {
  let attempts = Arc::new(AtomicUsize::new(0));
  let results = Arc::new(Mutex::new(Vec::new()));
  let workers = Arc::new(Mutex::new(Vec::new()));
  let mut system = with_catcher("PekkoCustomSpec");
  let parent = system.actor_of(Props::new({
    let attempts = Arc::clone(&attempts);
    let results = Arc::clone(&results);
    let workers = Arc::clone(&workers);
    move || SpawnsRestartingChild {
      attempts: Arc::clone(&attempts),
      results: Arc::clone(&results),
      workers: Arc::clone(&workers),
    }
  }));

  parent.tell("start", parent.clone());
  system.run_until_idle();

  assert_eq!(attempts.load(Ordering::SeqCst), 2, "the failed start was not retried exactly once");
  assert_eq!(
    *results.lock().unwrap(),
    alloc::vec!["worker-spawned", "worker-spawned", "started"],
    "the retried start did not reuse the name its first attempt took"
  );
  let workers = workers.lock().unwrap();
  assert_eq!(workers.len(), 2, "each attempt should have started its own worker");
  assert!(system.is_actor_terminated(&workers[0]), "the worker of the failed attempt survived the retry");
  assert!(!system.is_actor_terminated(&workers[1]), "the worker of the retried attempt is not alive");
}

/// Checks its own liveness from every hook it runs.
struct ReportsItsOwnLiveness {
  seen: Arc<Mutex<Vec<(&'static str, bool)>>>,
  parent: Option<ActorRef>,
}

impl Actor for ReportsItsOwnLiveness {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let me = context.self_ref();
    self.seen.lock().unwrap().push(("pre_start", context.is_terminated(&me)));
    if let Some(parent) = self.parent.clone() {
      let terminated = context.is_terminated(&parent);
      self.seen.lock().unwrap().push(("child_pre_start_parent", terminated));
    }
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let me = context.self_ref();
    self.seen.lock().unwrap().push(("receive", context.is_terminated(&me)));
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("simulated failure");
    }
  }

  fn post_restart(&mut self, context: &mut ActorContext<'_>) {
    let me = context.self_ref();
    self.seen.lock().unwrap().push(("post_restart", context.is_terminated(&me)));
  }
}

struct StartsLiveChild {
  seen: Arc<Mutex<Vec<(&'static str, bool)>>>,
}

impl Actor for StartsLiveChild {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let me = context.self_ref();
    self.seen.lock().unwrap().push(("pre_start", context.is_terminated(&me)));
    let seen = Arc::clone(&self.seen);
    let parent = me.clone();
    let _ = context.actor_of_with_strategy(
      Props::new(move || ReportsItsOwnLiveness { seen: Arc::clone(&seen), parent: Some(parent.clone()) }),
      crate::kernel::supervision::SupervisorStrategy::default_strategy(),
    );
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let me = context.self_ref();
    self.seen.lock().unwrap().push(("receive", context.is_terminated(&me)));
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("simulated failure");
    }
  }

  fn post_restart(&mut self, context: &mut ActorContext<'_>) {
    let me = context.self_ref();
    self.seen.lock().unwrap().push(("post_restart", context.is_terminated(&me)));
  }
}

#[test]
fn an_actor_is_not_terminated_while_it_runs() {
  let seen = Arc::new(Mutex::new(Vec::new()));
  let mut system = with_catcher("PekkoCustomSpec");
  let parent = system.actor_of(Props::new({
    let seen = Arc::clone(&seen);
    move || StartsLiveChild { seen: Arc::clone(&seen) }
  }));

  parent.tell("check", parent.clone());
  parent.tell("boom", parent.clone());
  system.run_until_idle();

  let seen = seen.lock().unwrap();
  for label in ["pre_start", "child_pre_start_parent", "receive", "post_restart"] {
    assert!(seen.iter().any(|(name, _)| *name == label), "the {label} check did not run");
  }
  for (label, terminated) in seen.iter() {
    assert!(!*terminated, "the actor was reported terminated inside {label}");
  }
}

/// Stops itself from `pre_start`, so the parent's hook must wait until the
/// parent frame has finished.
struct StopsItselfInPreStart;

impl Actor for StopsItselfInPreStart {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let me = context.self_ref();
    context.stop(&me);
  }
}

struct StartsSelfStoppingChild {
  hooks: Arc<AtomicUsize>,
  hook_liveness: Arc<Mutex<Vec<bool>>>,
}

impl Actor for StartsSelfStoppingChild {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"go") {
      let _ = context.actor_of(Props::new(|| StopsItselfInPreStart), "selfstop");
    }
  }

  fn supervisor_strategy(&self) -> crate::kernel::supervision::SupervisorStrategy {
    let hooks = Arc::clone(&self.hooks);
    let hook_liveness = Arc::clone(&self.hook_liveness);
    crate::kernel::supervision::SupervisorStrategy::one_for_one(3).with_child_terminated(
      move |context, _child, _siblings| {
        let me = context.self_ref();
        hook_liveness.lock().unwrap().push(context.is_terminated(&me));
        hooks.fetch_add(1, Ordering::SeqCst);
      },
    )
  }
}

#[test]
fn a_child_that_stops_itself_while_starting_still_runs_the_parent_hook_once() {
  let hooks = Arc::new(AtomicUsize::new(0));
  let hook_liveness = Arc::new(Mutex::new(Vec::new()));
  let mut system = with_catcher("PekkoCustomSpec");
  let parent = system.actor_of(Props::new({
    let hooks = Arc::clone(&hooks);
    let hook_liveness = Arc::clone(&hook_liveness);
    move || StartsSelfStoppingChild { hooks: Arc::clone(&hooks), hook_liveness: Arc::clone(&hook_liveness) }
  }));

  parent.tell("go", parent.clone());
  system.run_until_idle();

  assert_eq!(hooks.load(Ordering::SeqCst), 1, "the parent hook did not run exactly once");
  assert_eq!(*hook_liveness.lock().unwrap(), alloc::vec![false], "the parent was terminated inside its hook");
}

/// A parent whose child-terminated hook panics.
struct PanickingHookParent {
  strategy: crate::kernel::supervision::SupervisorStrategy,
  received: Arc<Mutex<Vec<&'static str>>>,
}

impl Actor for PanickingHookParent {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(text) = message.downcast_ref::<&'static str>() else {
      return;
    };
    if *text == "go" {
      let kid = context.actor_of(Props::new(|| NoOp), "kid").expect("the kid name must be free");
      context.stop(&kid);
    } else if *text == "ping" {
      self.received.lock().unwrap().push("ping");
      context.sender().tell("pong", context.self_ref());
    }
  }

  fn supervisor_strategy(&self) -> crate::kernel::supervision::SupervisorStrategy {
    self.strategy.clone()
  }
}

#[test]
fn a_panicking_child_terminated_hook_leaves_the_parent_alive_and_logs_once() {
  let events = Arc::new(Mutex::new(Vec::new()));
  let received = Arc::new(Mutex::new(Vec::new()));
  let replies = Arc::new(Mutex::new(Vec::new()));
  let mut system = with_catcher("PekkoCustomSpec");
  system.set_event_sink(Arc::new(RecordingSink { events: Arc::clone(&events) }));
  let observer = system.function_ref("observer", {
    let replies = Arc::clone(&replies);
    move |_sender: &ActorRef, message: Message| {
      if let Some(text) = message.downcast_ref::<&'static str>() {
        replies.lock().unwrap().push(*text);
      }
    }
  });
  let parent = system.actor_of(Props::new({
    let received = Arc::clone(&received);
    move || PanickingHookParent {
      strategy: crate::kernel::supervision::SupervisorStrategy::one_for_one(3)
        .with_child_terminated(|_context, _child, _siblings| panic!("simulated hook failure")),
      received: Arc::clone(&received),
    }
  }));

  parent.tell("go", parent.clone());
  system.run_until_idle();

  assert!(!system.is_actor_terminated(&parent), "the parent cell was lost when its hook panicked");
  parent.tell("ping", observer);
  system.run_until_idle();
  assert_eq!(*received.lock().unwrap(), alloc::vec!["ping"], "the parent no longer handled messages");
  assert_eq!(*replies.lock().unwrap(), alloc::vec!["pong"], "the parent did not answer after its hook panicked");

  let events = events.lock().unwrap();
  let failures: Vec<_> = events
    .iter()
    .filter(|(cause, message)| {
      cause.as_deref() == Some("IllegalStateException") && message == "handleChildTerminated failed"
    })
    .collect();
  assert_eq!(failures.len(), 1, "the hook failure was not logged exactly once: {events:?}");
}

/// The message every panicking decider raises, so a test can count its log.
const DECIDER_PANIC_MESSAGE: &str = "simulated decider panic";

/// Returns a strategy whose decider panics only for `failure`, so a test can
/// make the supervisor's decision fail at one failure kind while the other
/// kinds keep the strategy's answer.
fn strategy_with_a_panicking_decider(
  failure: crate::kernel::supervision::FailureKind,
) -> crate::kernel::supervision::SupervisorStrategy {
  crate::kernel::supervision::SupervisorStrategy::one_for_one(3).with_decider(move |_child, kind| {
    if kind == failure {
      panic!("{}", DECIDER_PANIC_MESSAGE);
    }
    crate::kernel::supervision::Directive::Restart
  })
}

/// Returns how many decider failures the sink recorded.
///
/// A single decider panic must be reported once, as a `RuntimeException` whose
/// message is the panic text, so a test counts the events with both facts
/// instead of trusting a substring somewhere in the log.
fn decider_failures(events: &Arc<Mutex<Vec<RecordedEvent>>>) -> usize {
  events
    .lock()
    .unwrap()
    .iter()
    .filter(|(cause, message)| cause.as_deref() == Some("RuntimeException") && message == DECIDER_PANIC_MESSAGE)
    .count()
}

/// Counts its own restarts and answers `ping`, so a test can tell whether the
/// supervising parent was restarted by its parent's strategy.
struct CountsSupervisorRestarts {
  post_restarts: Arc<AtomicUsize>,
}

impl Actor for CountsSupervisorRestarts {
  fn pre_restart(&mut self, _context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {}

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"ping") {
      context.sender().tell("pong", context.self_ref());
    }
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.post_restarts.fetch_add(1, Ordering::SeqCst);
  }
}

/// Starts a supervisor child in `pre_start` with the default strategy, so the
/// supervisor inherits this actor's strategy as a grandchild would.
struct StartsSupervisor {
  supervisor: Arc<Mutex<Option<ActorRef>>>,
  post_restarts: Arc<AtomicUsize>,
}

impl Actor for StartsSupervisor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let post_restarts = Arc::clone(&self.post_restarts);
    let supervisor = context
      .actor_of(
        Props::new(move || CountsSupervisorRestarts { post_restarts: Arc::clone(&post_restarts) }),
        "supervisor",
      )
      .expect("the supervisor name must be free");
    *self.supervisor.lock().unwrap() = Some(supervisor);
  }
}

#[test]
fn a_panicking_decider_on_a_message_failure_restarts_the_supervisor_and_keeps_the_child() {
  let events = Arc::new(Mutex::new(Vec::new()));
  let post_restarts = Arc::new(AtomicUsize::new(0));
  let supervisor_handle = Arc::new(Mutex::new(None));
  let replies = Arc::new(Mutex::new(Vec::new()));
  let mut system = with_catcher("PekkoCustomSpec");
  system.set_event_sink(Arc::new(RecordingSink { events: Arc::clone(&events) }));
  let observer = system.function_ref("observer", {
    let replies = Arc::clone(&replies);
    move |_sender: &ActorRef, message: Message| {
      if let Some(text) = message.downcast_ref::<&'static str>() {
        replies.lock().unwrap().push(*text);
      }
    }
  });
  let _grandparent = system.actor_of(Props::new({
    let supervisor = Arc::clone(&supervisor_handle);
    let post_restarts = Arc::clone(&post_restarts);
    move || StartsSupervisor { supervisor: Arc::clone(&supervisor), post_restarts: Arc::clone(&post_restarts) }
  }));
  let supervisor = supervisor_handle.lock().unwrap().clone().expect("the grandparent started its supervisor");

  let child = system.downgrade().spawn_child(
    &supervisor,
    Props::new(|| CountsRestarts { post_restarts: Arc::new(AtomicUsize::new(0)) }),
    Some("kid"),
    strategy_with_a_panicking_decider(crate::kernel::supervision::FailureKind::MessageHandling),
  );

  child.tell("boom", child.clone());
  let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| system.run_until_idle()));

  assert!(outcome.is_ok(), "the decider panic escaped the executor");
  assert!(!system.is_actor_terminated(&supervisor), "the supervising parent was not restarted");
  assert_eq!(post_restarts.load(Ordering::SeqCst), 1, "the supervising parent was not restarted exactly once");
  assert!(!system.is_actor_terminated(&child), "the failing child was stopped instead of leaving it to the supervisor");
  assert_eq!(
    decider_failures(&events),
    1,
    "the decider failure was not logged exactly once: {:?}",
    *events.lock().unwrap()
  );

  supervisor.tell("ping", observer);
  system.run_until_idle();
  assert_eq!(*replies.lock().unwrap(), alloc::vec!["pong"], "the restarted supervisor did not answer");

  let started_after = system.actor_of(Props::new(|| NoOp));
  assert!(!system.is_actor_terminated(&started_after), "the executor did not start an actor after the panic");
}

/// Starts a supervisor child that inherits this actor's stopping strategy, so
/// the supervisor's own failure is stopped by its parent's strategy.
struct StartsStoppingSupervisor {
  supervisor: Arc<Mutex<Option<ActorRef>>>,
}

impl Actor for StartsStoppingSupervisor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let supervisor = context.actor_of(Props::new(|| NoOp), "supervisor").expect("the supervisor name must be free");
    *self.supervisor.lock().unwrap() = Some(supervisor);
  }

  fn supervisor_strategy(&self) -> crate::kernel::supervision::SupervisorStrategy {
    crate::kernel::supervision::SupervisorStrategy::stopping_strategy()
  }
}

#[test]
fn a_panicking_decider_on_a_start_failure_stops_the_idle_supervisor_and_frees_its_name() {
  let events = Arc::new(Mutex::new(Vec::new()));
  let supervisor_handle = Arc::new(Mutex::new(None));
  let mut system = with_catcher("PekkoCustomSpec");
  system.set_event_sink(Arc::new(RecordingSink { events: Arc::clone(&events) }));
  let grandparent = system.actor_of(Props::new({
    let supervisor = Arc::clone(&supervisor_handle);
    move || StartsStoppingSupervisor { supervisor: Arc::clone(&supervisor) }
  }));
  let supervisor = supervisor_handle.lock().unwrap().clone().expect("the grandparent started its supervisor");

  let spawned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    system.downgrade().spawn_child(
      &supervisor,
      Props::new(|| FailsStartUntil {
        attempts: Arc::new(AtomicUsize::new(0)),
        failures: 1,
        received: Arc::new(Mutex::new(Vec::new())),
      }),
      Some("kid"),
      strategy_with_a_panicking_decider(crate::kernel::supervision::FailureKind::Initialization),
    )
  }));
  let child = match spawned {
    | Ok(child) => child,
    | Err(_) => panic!("the decider panic escaped the executor"),
  };

  assert!(system.is_actor_terminated(&child), "the child whose start failed is still alive");
  assert!(
    system.is_actor_terminated(&supervisor),
    "the supervising parent was not stopped by the grandparent's strategy"
  );
  assert_eq!(
    system.executor.borrow().parent_entry_count(),
    1,
    "a stopped supervisor left an entry in the parent registry"
  );
  assert_eq!(
    decider_failures(&events),
    1,
    "the decider failure was not logged exactly once: {:?}",
    *events.lock().unwrap()
  );

  let replacement = system.downgrade().spawn_child(
    &grandparent,
    Props::new(|| NoOp),
    Some("supervisor"),
    crate::kernel::supervision::SupervisorStrategy::default_strategy(),
  );
  assert!(!system.is_actor_terminated(&replacement), "the name of the stopped supervisor was not reusable");

  let started_after = system.actor_of(Props::new(|| NoOp));
  assert!(!system.is_actor_terminated(&started_after), "the executor did not start an actor after the panic");
}

#[test]
fn a_panicking_decider_on_a_kill_stops_the_supervisor_and_tells_the_watcher() {
  let events = Arc::new(Mutex::new(Vec::new()));
  let observed = Arc::new(Mutex::new(Vec::<Terminated>::new()));
  let watched = Arc::new(Mutex::new(None));
  let supervisor_handle = Arc::new(Mutex::new(None));
  let mut system = with_catcher("PekkoCustomSpec");
  system.set_event_sink(Arc::new(RecordingSink { events: Arc::clone(&events) }));
  let _grandparent = system.actor_of(Props::new({
    let supervisor = Arc::clone(&supervisor_handle);
    move || StartsStoppingSupervisor { supervisor: Arc::clone(&supervisor) }
  }));
  let supervisor = supervisor_handle.lock().unwrap().clone().expect("the grandparent started its supervisor");
  let watcher = system.actor_of(Props::new({
    let observed = Arc::clone(&observed);
    let watched = Arc::clone(&watched);
    move || StopWatcher { observed: Arc::clone(&observed), watched: Arc::clone(&watched) }
  }));
  let child = system.downgrade().spawn_child(
    &supervisor,
    Props::new(|| NoOp),
    Some("kid"),
    strategy_with_a_panicking_decider(crate::kernel::supervision::FailureKind::Killed),
  );
  *watched.lock().unwrap() = Some(child.clone());
  system.watch(&watcher, &child);

  child.tell("Kill", watcher.clone());
  let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| system.run_until_idle()));

  assert!(outcome.is_ok(), "the decider panic escaped the executor");
  assert!(
    system.is_actor_terminated(&supervisor),
    "the supervising parent was not stopped by the grandparent's strategy"
  );
  assert!(system.is_actor_terminated(&child), "the killed child is still alive");
  assert_eq!(observed.lock().unwrap().len(), 1, "the watcher did not receive exactly one Terminated");
  assert_eq!(
    decider_failures(&events),
    1,
    "the decider failure was not logged exactly once: {:?}",
    *events.lock().unwrap()
  );
}

/// Records both creation results so a failed start cannot hide a name refusal
/// after the supervising actor restarts.
struct StartsKidUnderPanickingDecider {
  restarts: Arc<AtomicUsize>,
  started: Arc<Mutex<Vec<Result<ActorRef, crate::kernel::actor::InvalidActorNameException>>>>,
  kid: Arc<Mutex<Option<ActorRef>>>,
}

impl Actor for StartsKidUnderPanickingDecider {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(text) = message.downcast_ref::<&'static str>() else {
      return;
    };
    if *text == "go" {
      let outcome = context.actor_of(
        Props::new(|| FailsStartUntil {
          attempts: Arc::new(AtomicUsize::new(0)),
          failures: 1,
          received: Arc::new(Mutex::new(Vec::new())),
        }),
        "bob",
      );
      if let Ok(kid) = &outcome {
        *self.kid.lock().unwrap() = Some(kid.clone());
      }
      self.started.lock().unwrap().push(outcome);
    } else if *text == "respawn" {
      let outcome = context.actor_of(Props::new(|| NoOp), "bob");
      self.started.lock().unwrap().push(outcome);
    }
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.restarts.fetch_add(1, Ordering::SeqCst);
  }

  fn supervisor_strategy(&self) -> crate::kernel::supervision::SupervisorStrategy {
    strategy_with_a_panicking_decider(crate::kernel::supervision::FailureKind::Initialization)
  }
}

#[test]
fn a_restarted_supervisor_reuses_the_name_of_a_child_that_failed_to_start() {
  let events = Arc::new(Mutex::new(Vec::new()));
  let restarts = Arc::new(AtomicUsize::new(0));
  let started = Arc::new(Mutex::new(Vec::new()));
  let kid_handle = Arc::new(Mutex::new(None));
  let mut system = with_catcher("PekkoCustomSpec");
  system.set_event_sink(Arc::new(RecordingSink { events: Arc::clone(&events) }));
  let parent = system.actor_of(Props::new({
    let restarts = Arc::clone(&restarts);
    let started = Arc::clone(&started);
    let kid = Arc::clone(&kid_handle);
    move || StartsKidUnderPanickingDecider {
      restarts: Arc::clone(&restarts),
      started: Arc::clone(&started),
      kid: Arc::clone(&kid),
    }
  }));

  parent.tell("go", parent.clone());
  let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| system.run_until_idle()));
  assert!(outcome.is_ok(), "the decider panic escaped the executor");

  parent.tell("respawn", parent.clone());
  system.run_until_idle();

  assert_eq!(restarts.load(Ordering::SeqCst), 1, "the supervising parent was not restarted exactly once");
  {
    let started = started.lock().unwrap();
    assert_eq!(started.len(), 2, "both creation requests must be observed");
    assert!(started[0].is_ok(), "the initial name was refused: {:?}", started[0]);
    assert!(started[1].is_ok(), "bob was refused after the supervisor restarted: {:?}", started[1]);
    assert_ne!(started[0], started[1], "the replacement must be a new child");
  }
  let kid = kid_handle.lock().unwrap().clone().expect("the child was started before the decider panicked");
  assert!(system.is_actor_terminated(&kid), "the default restart hook left the uninitialized child alive");
  assert_eq!(
    decider_failures(&events),
    1,
    "the decider failure was not logged exactly once: {:?}",
    *events.lock().unwrap()
  );
}

struct RecordsRestartOrder {
  order: Arc<Mutex<Vec<&'static str>>>,
  label: &'static str,
}

impl Actor for RecordsRestartOrder {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {
    panic!("restart the parent");
  }

  fn post_stop(&mut self) {
    self.order.lock().unwrap().push(self.label);
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.order.lock().unwrap().push("restarted");
  }
}

#[test]
fn default_pre_restart_stops_all_children_before_the_parent_post_stop() {
  let order = Arc::new(Mutex::new(Vec::new()));
  let mut system = with_catcher("PekkoCustomSpec");
  let parent = system.actor_of(Props::new({
    let order = Arc::clone(&order);
    move || RecordsRestartOrder { order: Arc::clone(&order), label: "parent" }
  }));
  let mut children = Vec::new();
  for label in ["first child", "second child"] {
    let order = Arc::clone(&order);
    children.push(system.downgrade().spawn_child(
      &parent,
      Props::new(move || RecordsRestartOrder { order: Arc::clone(&order), label }),
      Some(label),
      crate::kernel::supervision::SupervisorStrategy::default_strategy(),
    ));
  }

  parent.tell("boom", parent.clone());
  system.run_until_idle();

  let order = order.lock().unwrap();
  assert_eq!(order.len(), 4, "both children must stop before the parent restarts: {order:?}");
  assert!(order[..2].contains(&"first child"), "the first child stopped too late: {order:?}");
  assert!(order[..2].contains(&"second child"), "the second child stopped too late: {order:?}");
  assert_eq!(&order[2..], &["parent", "restarted"]);
  assert!(children.iter().all(|child| system.is_actor_terminated(child)), "a child survived the default hook");
  assert!(!system.is_actor_terminated(&parent), "the parent must restart rather than stop");
}

#[test]
fn a_kill_restart_without_a_catcher_stops_children_before_post_stop() {
  let order = Arc::new(Mutex::new(Vec::new()));
  let mut system = system("PekkoCustomSpec");
  let root = system.actor_of(Props::new(|| NoOp));
  let parent = system.downgrade().spawn_child(
    &root,
    Props::new({
      let order = Arc::clone(&order);
      move || RecordsRestartOrder { order: Arc::clone(&order), label: "parent" }
    }),
    Some("parent"),
    crate::kernel::supervision::SupervisorStrategy::one_for_one(3),
  );
  let child = system.downgrade().spawn_child(
    &parent,
    Props::new({
      let order = Arc::clone(&order);
      move || RecordsRestartOrder { order: Arc::clone(&order), label: "child" }
    }),
    Some("child"),
    crate::kernel::supervision::SupervisorStrategy::default_strategy(),
  );

  parent.tell("Kill", root);
  system.run_until_idle();

  assert_eq!(*order.lock().unwrap(), alloc::vec!["child", "parent", "restarted"]);
  assert!(system.is_actor_terminated(&child));
  assert!(!system.is_actor_terminated(&parent));
}

struct RestartWithChildTerminationHook {
  notices: Arc<Mutex<Vec<ActorRef>>>,
}

impl Actor for RestartWithChildTerminationHook {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {
    panic!("restart with a child-terminated hook");
  }

  fn supervisor_strategy(&self) -> crate::kernel::supervision::SupervisorStrategy {
    let notices = Arc::clone(&self.notices);
    crate::kernel::supervision::SupervisorStrategy::one_for_one(3)
      .with_child_terminated(move |_context, child, _siblings| notices.lock().unwrap().push(child.clone()))
  }
}

#[test]
fn default_restart_delivers_child_termination_to_the_restored_parent() {
  let notices = Arc::new(Mutex::new(Vec::new()));
  let mut system = with_catcher("PekkoCustomSpec");
  let parent = system.actor_of(Props::new({
    let notices = Arc::clone(&notices);
    move || RestartWithChildTerminationHook { notices: Arc::clone(&notices) }
  }));
  let child = system.downgrade().spawn_child(
    &parent,
    Props::new(|| NoOp),
    Some("child"),
    crate::kernel::supervision::SupervisorStrategy::default_strategy(),
  );

  parent.tell("boom", parent.clone());
  system.run_until_idle();

  assert_eq!(*notices.lock().unwrap(), alloc::vec![child.clone()]);
  assert!(system.is_actor_terminated(&child));
  assert!(!system.is_actor_terminated(&parent));
}

struct ChangesBehaviorBeforeRestart;

impl Actor for ChangesBehaviorBeforeRestart {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("restart after installing a behavior in the hook");
    }
    context.sender().tell("initial", context.self_ref());
  }

  fn pre_restart(&mut self, context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    context.change_behavior(|context, _message| {
      context.sender().tell("old hook", context.self_ref());
    });
  }
}

#[test]
fn a_behavior_installed_by_pre_restart_does_not_survive_recreation() {
  let replies = Arc::new(Mutex::new(Vec::new()));
  let mut system = with_catcher("PekkoCustomSpec");
  let observer = system.function_ref("observer", {
    let replies = Arc::clone(&replies);
    move |_sender: &ActorRef, message: Message| {
      if let Some(text) = message.downcast_ref::<&'static str>() {
        replies.lock().unwrap().push(*text);
      }
    }
  });
  let parent = system.actor_of(Props::new(|| ChangesBehaviorBeforeRestart));

  parent.tell("boom", observer.clone());
  system.run_until_idle();
  parent.tell("ping", observer);
  system.run_until_idle();

  assert_eq!(*replies.lock().unwrap(), alloc::vec!["initial"]);
}

struct KeepsChildrenOnRestart {
  restarts: Arc<AtomicUsize>,
  attempts: Arc<AtomicUsize>,
  received: Arc<Mutex<Vec<&'static str>>>,
  failed_child: Arc<Mutex<Option<ActorRef>>>,
}

impl Actor for KeepsChildrenOnRestart {
  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    let attempts = Arc::clone(&self.attempts);
    let received = Arc::clone(&self.received);
    let child = context
      .actor_of(
        Props::new(move || FailsStartUntil {
          attempts: Arc::clone(&attempts),
          failures: 1,
          received: Arc::clone(&received),
        }),
        "bob",
      )
      .expect("bob must be available before the failed start");
    *self.failed_child.lock().unwrap() = Some(child);
  }

  fn pre_restart(&mut self, _context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {}

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.restarts.fetch_add(1, Ordering::SeqCst);
  }

  fn supervisor_strategy(&self) -> crate::kernel::supervision::SupervisorStrategy {
    strategy_with_a_panicking_decider(crate::kernel::supervision::FailureKind::Initialization)
  }
}

struct StatefulChild {
  value: usize,
  observations: Arc<Mutex<Vec<(ActorRef, usize)>>>,
}

impl Actor for StatefulChild {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    match message.downcast_ref::<&'static str>() {
      | Some(&"increment") => self.value += 1,
      | Some(&"read") => self.observations.lock().unwrap().push((context.self_ref(), self.value)),
      | _ => {},
    }
  }
}

#[test]
fn an_empty_pre_restart_recovers_only_the_uninitialized_child_and_keeps_the_healthy_child_state() {
  let restarts = Arc::new(AtomicUsize::new(0));
  let attempts = Arc::new(AtomicUsize::new(0));
  let received = Arc::new(Mutex::new(Vec::new()));
  let failed_child = Arc::new(Mutex::new(None));
  let healthy_creations = Arc::new(AtomicUsize::new(0));
  let observations = Arc::new(Mutex::new(Vec::new()));
  let mut system = with_catcher("PekkoCustomSpec");
  let parent = system.actor_of(Props::new({
    let restarts = Arc::clone(&restarts);
    let attempts = Arc::clone(&attempts);
    let received = Arc::clone(&received);
    let failed_child = Arc::clone(&failed_child);
    move || KeepsChildrenOnRestart {
      restarts: Arc::clone(&restarts),
      attempts: Arc::clone(&attempts),
      received: Arc::clone(&received),
      failed_child: Arc::clone(&failed_child),
    }
  }));
  let healthy = system.downgrade().spawn_child(
    &parent,
    Props::new({
      let creations = Arc::clone(&healthy_creations);
      let observations = Arc::clone(&observations);
      move || {
        creations.fetch_add(1, Ordering::SeqCst);
        StatefulChild { value: 0, observations: Arc::clone(&observations) }
      }
    }),
    Some("healthy"),
    crate::kernel::supervision::SupervisorStrategy::default_strategy(),
  );
  healthy.tell("increment", parent.clone());
  healthy.tell("read", parent.clone());
  system.run_until_idle();
  assert_eq!(*observations.lock().unwrap(), alloc::vec![(healthy.clone(), 1)]);

  parent.tell("go", parent.clone());
  system.run_until_idle();

  assert_eq!(restarts.load(Ordering::SeqCst), 1, "the parent must restart after its decider fails");
  healthy.tell("read", parent.clone());
  let recovered = failed_child.lock().unwrap().clone().expect("the failed creation returned its child handle");
  recovered.tell("recovered", parent.clone());
  system.run_until_idle();
  assert_eq!(healthy_creations.load(Ordering::SeqCst), 1, "the healthy child's behavior was recreated");
  assert_eq!(*observations.lock().unwrap(), alloc::vec![(healthy.clone(), 1), (healthy, 1)]);
  assert_eq!(*received.lock().unwrap(), alloc::vec!["recovered"], "the retained child still has no behavior");
  assert_eq!(attempts.load(Ordering::SeqCst), 2, "only the failed child must be created again");
  let duplicate = system.executor.borrow_mut().spawn_named_child(&parent, Props::new(|| NoOp), "bob");
  assert!(duplicate.is_err(), "a recovered live child must still reserve bob");
}

#[test]
fn a_failed_parent_recreation_does_not_retry_its_uninitialized_child() {
  let creations = Arc::new(AtomicUsize::new(0));
  let restarts = Arc::new(AtomicUsize::new(0));
  let attempts = Arc::new(AtomicUsize::new(0));
  let received = Arc::new(Mutex::new(Vec::new()));
  let failed_child = Arc::new(Mutex::new(None));
  let mut system = with_catcher("PekkoCustomSpec");
  let parent = system.actor_of(Props::new({
    let creations = Arc::clone(&creations);
    let restarts = Arc::clone(&restarts);
    let attempts = Arc::clone(&attempts);
    let received = Arc::clone(&received);
    let failed_child = Arc::clone(&failed_child);
    move || {
      if creations.fetch_add(1, Ordering::SeqCst) > 0 {
        panic!("the parent recipe fails on restart");
      }
      KeepsChildrenOnRestart {
        restarts: Arc::clone(&restarts),
        attempts: Arc::clone(&attempts),
        received: Arc::clone(&received),
        failed_child: Arc::clone(&failed_child),
      }
    }
  }));

  parent.tell("go", parent.clone());
  system.run_until_idle();

  let child = failed_child.lock().unwrap().clone().expect("the failed child was allocated");
  assert_eq!(creations.load(Ordering::SeqCst), 2);
  assert_eq!(attempts.load(Ordering::SeqCst), 1, "a failed parent restart must not begin child recovery");
  assert_eq!(restarts.load(Ordering::SeqCst), 0);
  assert!(system.is_actor_terminated(&parent));
  assert!(system.is_actor_terminated(&child));
}

/// Stops itself and then starts a child in the same delivery.
struct StopsThenStartsChild {
  worker: Arc<Mutex<Option<ActorRef>>>,
  self_liveness: Arc<Mutex<Vec<bool>>>,
}

impl Actor for StopsThenStartsChild {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() != Some(&"go") {
      return;
    }
    let me = context.self_ref();
    context.stop(&me);
    let worker = context.actor_of(Props::new(|| NoOp), "worker").expect("the worker name must be free");
    *self.worker.lock().unwrap() = Some(worker);
    self.self_liveness.lock().unwrap().push(!context.is_terminated(&me));
  }
}

#[test]
fn a_stop_requested_before_a_child_starts_still_stops_the_actor() {
  let worker = Arc::new(Mutex::new(None));
  let self_liveness = Arc::new(Mutex::new(Vec::new()));
  let mut system = with_catcher("PekkoCustomSpec");
  let actor = system.actor_of(Props::new({
    let worker = Arc::clone(&worker);
    let self_liveness = Arc::clone(&self_liveness);
    move || StopsThenStartsChild { worker: Arc::clone(&worker), self_liveness: Arc::clone(&self_liveness) }
  }));

  actor.tell("go", actor.clone());
  system.run_until_idle();

  assert_eq!(*self_liveness.lock().unwrap(), alloc::vec![true], "the actor was not alive during its delivery");
  assert!(system.is_actor_terminated(&actor), "the stop requested before the nested start was lost");
  let worker = worker.lock().unwrap().clone().expect("the nested start created a child");
  assert!(system.is_actor_terminated(&worker), "the child of the stopped actor is still alive");
}

#[test]
#[should_panic(expected = "cannot create children")]
fn a_weak_named_child_of_a_stopped_parent_is_refused() {
  let mut system = with_catcher("PekkoCustomSpec");
  let parent = system.actor_of(Props::new(|| NoOp));
  system.stop(&parent);

  system.downgrade().spawn_child(
    &parent,
    Props::new(|| NoOp),
    Some("kid"),
    crate::kernel::supervision::SupervisorStrategy::default_strategy(),
  );
}

#[test]
fn a_weak_unnamed_child_of_a_stopped_parent_is_refused() {
  let creations = Arc::new(AtomicUsize::new(0));
  let mut system = with_catcher("PekkoCustomSpec");
  let parent = system.actor_of(Props::new(|| NoOp));
  system.stop(&parent);

  let attempted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    let creations = Arc::clone(&creations);
    system.downgrade().spawn_child(
      &parent,
      Props::new(move || {
        creations.fetch_add(1, Ordering::SeqCst);
        NoOp
      }),
      None,
      crate::kernel::supervision::SupervisorStrategy::default_strategy(),
    );
  }));

  assert_eq!(creations.load(Ordering::SeqCst), 0, "the refused request still built the child");
  let payload = attempted.expect_err("a stopped parent must refuse an unnamed weak child");
  let message = payload
    .downcast_ref::<alloc::string::String>()
    .map(alloc::string::String::as_str)
    .or_else(|| payload.downcast_ref::<&'static str>().copied())
    .unwrap_or_default();
  assert!(message.contains("cannot create children"), "unexpected refusal message: {message}");

  let started = system.actor_of(Props::new(|| NoOp));
  assert!(!system.is_actor_terminated(&started), "the executor refused to start actors after the refused request");
}

#[test]
fn a_stopped_actor_leaves_no_parent_entry_behind() {
  let mut system = with_catcher("PekkoCustomSpec");
  for _ in 0..3 {
    let actor = system.actor_of(Props::new(|| NoOp));
    system.stop(&actor);
  }

  assert_eq!(system.executor.borrow().parent_entry_count(), 0, "the parent registry kept stopped actors");
}

/// Counts its `post_restart` calls so a test can tell a restart from a stop.
struct CountsRestarts {
  post_restarts: Arc<AtomicUsize>,
}

impl Actor for CountsRestarts {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("simulated failure of the child");
    }
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.post_restarts.fetch_add(1, Ordering::SeqCst);
  }
}

/// A parent whose strategy changes between its first and later instances.
struct ChangesStrategyOnRestart {
  strategy: crate::kernel::supervision::SupervisorStrategy,
  kid: Arc<Mutex<Option<ActorRef>>>,
  kid_post_restarts: Arc<AtomicUsize>,
}

impl Actor for ChangesStrategyOnRestart {
  fn pre_restart(&mut self, _context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {}

  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let post_restarts = Arc::clone(&self.kid_post_restarts);
    if let Ok(kid) =
      context.actor_of(Props::new(move || CountsRestarts { post_restarts: Arc::clone(&post_restarts) }), "kid")
    {
      *self.kid.lock().unwrap() = Some(kid);
    }
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"panic") {
      panic!("simulated failure of the parent");
    }
  }

  fn supervisor_strategy(&self) -> crate::kernel::supervision::SupervisorStrategy {
    self.strategy.clone()
  }
}

#[test]
fn a_child_follows_the_strategy_of_its_restarted_parent() {
  let creations = Arc::new(AtomicUsize::new(0));
  let kid = Arc::new(Mutex::new(None));
  let kid_post_restarts = Arc::new(AtomicUsize::new(0));
  let mut system = with_catcher("PekkoCustomSpec");
  let parent = system.actor_of(Props::new({
    let creations = Arc::clone(&creations);
    let kid = Arc::clone(&kid);
    let kid_post_restarts = Arc::clone(&kid_post_restarts);
    move || {
      let creation = creations.fetch_add(1, Ordering::SeqCst);
      let strategy = if creation == 0 {
        crate::kernel::supervision::SupervisorStrategy::one_for_one(3)
      } else {
        crate::kernel::supervision::SupervisorStrategy::stopping_strategy()
      };
      ChangesStrategyOnRestart { strategy, kid: Arc::clone(&kid), kid_post_restarts: Arc::clone(&kid_post_restarts) }
    }
  }));

  let kid_handle = kid.lock().unwrap().clone().expect("the parent created its kid");
  parent.tell("panic", parent.clone());
  system.run_until_idle();
  assert!(!system.is_actor_terminated(&parent), "the parent did not restart");

  kid_handle.tell("boom", kid_handle.clone());
  system.run_until_idle();

  assert_eq!(
    kid_post_restarts.load(Ordering::SeqCst),
    0,
    "the child used the strategy from before the parent restarted"
  );
  assert!(system.is_actor_terminated(&kid_handle), "the child was not stopped by the new parent strategy");
}

/// Starts a child on every start attempt and fails the first attempt after the
/// child is in place, so the retry has to stop that child first.
struct StartsVictimStopperThenFailsOnce {
  attempts: Arc<AtomicUsize>,
  children: Arc<Mutex<Vec<ActorRef>>>,
  victim: Arc<Mutex<Option<ActorRef>>>,
}

impl Actor for StartsVictimStopperThenFailsOnce {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
    let victim = Arc::clone(&self.victim);
    let child = context.actor_of_with_strategy(
      Props::new(move || StopsVictimWhenItsChildStops { victim: Arc::clone(&victim) }),
      crate::kernel::supervision::SupervisorStrategy::default_strategy(),
    );
    self.children.lock().unwrap().push(child);
    if attempt == 1 {
      panic!("simulated initialization failure");
    }
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

/// Starts a leaf in `pre_start` and stops the victim from its child-terminated
/// hook, so stopping this actor moves the executor's cell table.
struct StopsVictimWhenItsChildStops {
  victim: Arc<Mutex<Option<ActorRef>>>,
}

impl Actor for StopsVictimWhenItsChildStops {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let _ = context
      .actor_of_with_strategy(Props::new(|| NoOp), crate::kernel::supervision::SupervisorStrategy::default_strategy());
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn supervisor_strategy(&self) -> crate::kernel::supervision::SupervisorStrategy {
    let victim = Arc::clone(&self.victim);
    crate::kernel::supervision::SupervisorStrategy::one_for_one(3).with_child_terminated(
      move |context, _child, _siblings| {
        if let Some(victim) = victim.lock().unwrap().clone() {
          context.stop(&victim);
        }
      },
    )
  }
}

#[test]
fn a_retried_start_finds_the_cell_again_after_a_child_stop_moves_the_table() {
  let attempts = Arc::new(AtomicUsize::new(0));
  let children = Arc::new(Mutex::new(Vec::new()));
  let victim = Arc::new(Mutex::new(None));
  let mut system = with_catcher("PekkoCustomSpec");
  let parent = system.actor_of(Props::new(|| NoOp));
  let victim_ref = system.actor_of(Props::new(|| NoOp));
  *victim.lock().unwrap() = Some(victim_ref.clone());

  let failing = system.downgrade().spawn_child(
    &parent,
    Props::new({
      let attempts = Arc::clone(&attempts);
      let children = Arc::clone(&children);
      let victim = Arc::clone(&victim);
      move || StartsVictimStopperThenFailsOnce {
        attempts: Arc::clone(&attempts),
        children: Arc::clone(&children),
        victim: Arc::clone(&victim),
      }
    }),
    None,
    crate::kernel::supervision::SupervisorStrategy::one_for_one_restarting_initialization_failures(3),
  );

  assert_eq!(attempts.load(Ordering::SeqCst), 2, "the failed start was not retried exactly once");
  assert!(!system.is_actor_terminated(&failing), "the second start did not succeed");
  let children = children.lock().unwrap();
  assert_eq!(children.len(), 2, "each attempt should have started its own child");
  assert!(system.is_actor_terminated(&children[0]), "the child of the failed attempt survived the retry");
  assert!(!system.is_actor_terminated(&children[1]), "the child of the retried attempt is not alive");
  assert!(system.is_actor_terminated(&victim_ref), "the stop issued from the child-terminated hook did not happen");
}
