// Kernel tests for the unhandled notification of Issue #207.
//
// Original (Pekko v2.0.0, SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   * `Props.empty` and `EmptyActor` (actor/src/main/scala/org/apache/pekko/actor/Props.scala)
//   * `Actor.unhandled` and `UnhandledMessage`
//     (actor/src/main/scala/org/apache/pekko/actor/Actor.scala)
//   * the `UnhandledMessage` arm of `TestEventListener`
//     (testkit/src/main/scala/org/apache/pekko/testkit/TestEventListener.scala)
//   * `PekkoSpecSpec` "warn about unhandled messages"
//     (testkit/src/test/scala/org/apache/pekko/testkit/PekkoSpecSpec.scala)
//
// This file covers the kernel API that #210 needs for the PekkoSpecSpec case.
// It is not a port of a test class, so it carries no ledger node.

use core::any::Any;
use core::time::Duration;
use std::sync::Arc;
use std::sync::Mutex;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::Logging::EventSink;
use fraktor_actor::kernel::actor::Logging::LogEvent;
use fraktor_actor::kernel::actor::Logging::LogLevel;
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props, UnhandledMessage};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::executor::ShardBoundary;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::event_filter::EventFilter;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

/// A message type that the empty actor cannot know about.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Custom(u8);

/// An actor that reports every message it receives as unhandled.
///
/// Mirrors what `Props::empty` builds, but calls `unhandled` from a behaviour
/// of its own so the context entry is exercised directly.
struct Rejector;

impl Actor for Rejector {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    context.unhandled(message);
  }
}

/// Records the warnings an executor publishes to the event sink.
///
/// The sink stands in for `TestEventListener`, which turns an `UnhandledMessage`
/// into a warning. The kernel publishes the same warning to the sink, so a test
/// can read the source and the text.
#[derive(Default)]
struct RecordingSink {
  warnings: Mutex<Vec<(String, String)>>,
}

impl EventSink for RecordingSink {
  fn publish(&self, event: &dyn LogEvent) {
    if event.level() == LogLevel::Warning {
      self.warnings.lock().unwrap().push((event.log_source().to_string(), event.message().to_string()));
    }
  }

  fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
    self
  }
}

/// One message of the bounded input of P1 to P3.
///
/// The set holds every message type the property covers: `i32` from -2 to 2,
/// three `&'static str` values (one of them empty), and one type the kernel
/// does not know.
#[derive(Clone, Debug)]
enum Input {
  Number(i32),
  Text(&'static str),
  Unknown(Custom),
}

impl Input {
  /// Every message of the bounded input, in the order a test sends them.
  fn all() -> Vec<Input> {
    let mut inputs: Vec<Input> = (-2_i32..=2).map(Input::Number).collect();
    inputs.extend(["alpha", "", "with spaces"].map(Input::Text));
    inputs.push(Input::Unknown(Custom(7)));
    inputs
  }

  fn tell(&self, target: &ActorRef, sender: &ActorRef) {
    match self {
      | Input::Number(value) => target.tell(*value, sender.clone()),
      | Input::Text(text) => target.tell(*text, sender.clone()),
      | Input::Unknown(custom) => target.tell(custom.clone(), sender.clone()),
    }
  }

  /// Checks that `message` is this input with its type kept.
  fn assert_same(&self, message: &Message) {
    match self {
      | Input::Number(value) => assert_eq!(message.downcast_ref::<i32>(), Some(value)),
      | Input::Text(text) => assert_eq!(message.downcast_ref::<&'static str>(), Some(text)),
      | Input::Unknown(custom) => assert_eq!(message.downcast_ref::<Custom>(), Some(custom)),
    }
  }

  /// The text the warning gives for this input: the `Debug` form of
  /// `Message`, which quotes a string and names no value for an unknown type.
  fn warning_form(&self) -> String {
    match self {
      | Input::Number(value) => format!("{value}"),
      | Input::Text(text) => format!("{text:?}"),
      | Input::Unknown(_) => String::from("<message of an unknown type>"),
    }
  }
}

/// The three senders of the bounded input: dead letters, the test actor, and
/// another actor.
fn senders(spec: &mut FraktorSpec) -> [ActorRef; 3] {
  let dead_letters = spec.system().dead_letters();
  let test_actor = spec.test_actor();
  let other = TestProbe::new(spec.system_mut());
  [dead_letters, test_actor, other.ref_handle()]
}

fn subscribe_to_unhandled(spec: &mut FraktorSpec) -> TestProbe {
  let subscriber = TestProbe::new(spec.system_mut());
  spec
    .system()
    .event_stream()
    .subscribe(Some(subscriber.ref_handle()), EventClass::of::<UnhandledMessage>())
    .expect("the probe is not null");
  subscriber
}

/// Takes one `UnhandledMessage` from `probe`, checks its three fields, and
/// checks that the same delivery produced no second report (I1).
///
/// A duplicate made while this delivery runs is queued before the probe hands
/// over the first report, so the zero-length wait below observes it without
/// adding a pause to the test.
fn assert_unhandled(probe: &mut TestProbe, expected: &Input, sender: &ActorRef, recipient: &ActorRef) {
  assert_next_unhandled(probe, expected, sender, recipient);
  probe.expect_no_message(Duration::ZERO);
}

/// Takes one `UnhandledMessage` from `probe` and checks its three fields,
/// leaving later reports in the queue.
fn assert_next_unhandled(probe: &mut TestProbe, expected: &Input, sender: &ActorRef, recipient: &ActorRef) {
  let unhandled: UnhandledMessage = probe.expect_msg_type();
  expected.assert_same(unhandled.message());
  assert_eq!(unhandled.sender(), sender);
  assert_eq!(unhandled.recipient(), recipient);
}

/// Sends every pair of the bounded input (3 senders x 9 messages) to `target`
/// and checks one report per pair.
fn assert_every_pair_is_reported_once(spec: &mut FraktorSpec, target: &ActorRef) {
  let mut subscriber = subscribe_to_unhandled(spec);
  let senders = senders(spec);
  for input in Input::all() {
    for sender in &senders {
      input.tell(target, sender);
      assert_unhandled(&mut subscriber, &input, sender, target);
    }
  }

  // P4: the same message from the same sender, twice in a row, is two reports.
  let repeated = Input::Number(0);
  repeated.tell(target, &senders[0]);
  repeated.tell(target, &senders[0]);
  spec.system_mut().run_until_idle();
  assert_next_unhandled(&mut subscriber, &repeated, &senders[0], target);
  assert_unhandled(&mut subscriber, &repeated, &senders[0], target);
}

/// P1 and P4: every message that reaches an empty actor is reported once, with
/// its sender and recipient, and the actor keeps running for the next one.
#[test]
fn props_empty_reports_every_message_once_for_every_bounded_input() {
  let mut spec = FraktorSpec::new("UnhandledNotificationSpecP1");
  let empty = spec.system_mut().actor_of(Props::empty());

  assert_every_pair_is_reported_once(&mut spec, &empty);

  spec.terminate();
}

/// P2: an actor that calls `ActorContext::unhandled` reports the message with
/// the sender of the delivery and itself as the recipient, for the same input
/// as P1.
#[test]
fn actor_context_unhandled_reports_the_message_and_sender() {
  let mut spec = FraktorSpec::new("UnhandledNotificationSpecP2");
  let rejector = spec.system_mut().actor_of(Props::new(|| Rejector));

  assert_every_pair_is_reported_once(&mut spec, &rejector);

  spec.terminate();
}

/// P3: the event sink receives one warning per report, naming the recipient as
/// its source and the sender and message in its text.
#[test]
fn unhandled_is_reported_as_a_warning_to_the_event_sink() {
  let mut spec = FraktorSpec::new("UnhandledNotificationSpecP3");
  let recording = Arc::new(RecordingSink::default());
  let sink: Arc<dyn EventSink> = recording.clone();
  spec.system().set_event_sink(sink);
  let empty = spec.system_mut().actor_of(Props::empty());
  let senders = senders(&mut spec);

  let mut expected = Vec::new();
  for input in Input::all() {
    for sender in &senders {
      input.tell(&empty, sender);
      expected.push((
        empty.path().to_string(),
        format!("unhandled message from {}: {}", sender.path(), input.warning_form()),
      ));
    }
  }
  spec.system_mut().run_until_idle();

  assert_eq!(*recording.warnings.lock().unwrap(), expected);

  spec.terminate();
}

/// Records what each behaviour of [`Pusher`] received.
type Trace = Arc<Mutex<Vec<String>>>;

fn number_of(message: &Message) -> i32 {
  *message.downcast_ref::<i32>().expect("the test sends i32 only")
}

/// Pushes a behaviour on its first message; the pushed behaviour reports every
/// later message as unhandled.
struct Pusher {
  trace: Trace,
}

impl Actor for Pusher {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let number = number_of(&message);
    self.trace.lock().unwrap().push(format!("base:{number}"));
    if number == 0 {
      let trace = Arc::clone(&self.trace);
      context.push_behavior(move |context: &mut ActorContext<'_>, message: Message| {
        trace.lock().unwrap().push(format!("pushed:{}", number_of(&message)));
        context.unhandled(message);
      });
    }
  }
}

/// I3: `unhandled` leaves the behaviour stack as it was. The pushed behaviour
/// that calls it answers the next message too.
#[test]
fn unhandled_keeps_the_pushed_behavior() {
  let mut spec = FraktorSpec::new("UnhandledNotificationSpecI3Stack");
  let mut subscriber = subscribe_to_unhandled(&mut spec);
  let trace = Trace::default();
  let pusher = spec.system_mut().actor_of(Props::new({
    let trace = Arc::clone(&trace);
    move || Pusher { trace: Arc::clone(&trace) }
  }));
  let sender = spec.system().dead_letters();

  for number in 0..3 {
    pusher.tell(number, sender.clone());
  }
  spec.system_mut().run_until_idle();

  assert_eq!(*trace.lock().unwrap(), ["base:0", "pushed:1", "pushed:2"]);
  assert_next_unhandled(&mut subscriber, &Input::Number(1), &sender, &pusher);
  assert_unhandled(&mut subscriber, &Input::Number(2), &sender, &pusher);

  spec.terminate();
}

/// Where a [`Stasher`] calls `unhandled` relative to `stash`.
#[derive(Clone, Copy)]
enum Order {
  /// `stash`, then `unhandled`: the stash must survive the report (P6).
  StashFirst,
  /// `unhandled`, then `sender` and `stash`: the message being handled and
  /// its sender must survive the report (P7).
  UnhandledFirst,
}

/// Stashes every number and reports it as unhandled; `"release"` returns the
/// stash, and the numbers are then recorded instead.
///
/// A number that comes back before `"release"` is recorded and not stashed
/// again. Without this bound, an `unhandled` that returned the stash would
/// loop forever (and the memory would grow until the process is killed);
/// with it, such a fault ends as an assertion failure.
struct Stasher {
  trace: Trace,
  order: Order,
  stashed: Vec<i32>,
  released: bool,
}

impl Stasher {
  fn props(trace: &Trace, order: Order) -> Props {
    let trace = Arc::clone(trace);
    Props::new(move || Stasher { trace: Arc::clone(&trace), order, stashed: Vec::new(), released: false })
  }

  fn record(&self, entry: String) {
    self.trace.lock().unwrap().push(entry);
  }
}

impl Actor for Stasher {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message == "release" {
      self.released = true;
      context.unstash_all();
      return;
    }
    let number = number_of(&message);
    if self.released {
      self.record(format!("replayed:{number}"));
    } else if self.stashed.contains(&number) {
      self.record(format!("came back before release:{number}"));
    } else {
      self.stashed.push(number);
      match self.order {
        | Order::StashFirst => {
          context.stash();
          context.unhandled(message);
        },
        | Order::UnhandledFirst => {
          context.unhandled(message);
          self.record(format!("sender after unhandled:{}", context.sender().path()));
          context.stash();
        },
      }
    }
  }
}

/// Sends `1`, `2`, `3` and `"release"` from `sender` to a [`Stasher`] and
/// checks the three reports.
fn run_stasher(spec: &mut FraktorSpec, order: Order, sender: &ActorRef) -> Vec<String> {
  let mut subscriber = subscribe_to_unhandled(spec);
  let trace = Trace::default();
  let actor = spec.system_mut().actor_of(Stasher::props(&trace, order));

  for number in 1..=3 {
    actor.tell(number, sender.clone());
  }
  actor.tell("release", sender.clone());
  spec.system_mut().run_until_idle();

  for number in 1..=3 {
    assert_next_unhandled(&mut subscriber, &Input::Number(number), sender, &actor);
  }
  subscriber.expect_no_message(Duration::ZERO);
  let entries = trace.lock().unwrap().clone();
  entries
}

/// P6 (I3): `unhandled` after `stash` leaves the stash as it was.
/// `unstash_all` returns every stashed message, in order, and none comes
/// back earlier.
#[test]
fn unhandled_keeps_the_stash() {
  let mut spec = FraktorSpec::new("UnhandledNotificationSpecI3Stash");
  let sender = spec.system().dead_letters();

  let trace = run_stasher(&mut spec, Order::StashFirst, &sender);

  assert_eq!(trace, ["replayed:1", "replayed:2", "replayed:3"]);
  spec.terminate();
}

/// P7 (I3): `unhandled` leaves the message being handled and its sender. A
/// `stash` called after it still stashes the current message, and `sender`
/// still names the sender of the delivery.
#[test]
fn unhandled_keeps_the_current_message_and_sender() {
  let mut spec = FraktorSpec::new("UnhandledNotificationSpecI3Current");
  let sender = spec.test_actor();

  let trace = run_stasher(&mut spec, Order::UnhandledFirst, &sender);

  let after = format!("sender after unhandled:{}", sender.path());
  assert_eq!(trace, [after.as_str(), after.as_str(), after.as_str(), "replayed:1", "replayed:2", "replayed:3"]);
  spec.terminate();
}

/// The `EventFilter.warning` path that the PekkoSpecSpec case uses: the report
/// reaches the spec's event sink as one warning, so `intercept` counts exactly
/// one.
#[test]
fn an_unhandled_message_is_one_warning_for_event_filter() {
  let mut spec = FraktorSpec::new("UnhandledNotificationSpecFilter");
  let empty = spec.system_mut().actor_of(Props::empty());
  let sender = spec.system().dead_letters();

  EventFilter::warning().intercept(spec.system_mut(), || {
    empty.tell(42_i32, sender);
  });

  spec.terminate();
}

/// I5: without a subscriber or an event sink the notification is dropped and
/// the actor still runs.
#[test]
fn unhandled_without_subscribers_or_a_sink_does_not_panic() {
  let boundary = Arc::new(ChannelBoundary::new());
  let mut system = ActorSystem::new("UnhandledNotificationSpecNoSink", Arc::clone(&boundary) as Arc<dyn ShardBoundary>);
  let empty = system.actor_of(Props::empty());

  empty.tell(42_i32, system.dead_letters());
  system.run_until_idle();

  system.terminate();
}
