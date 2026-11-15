// Ported from Pekko v1.7.1:
//   actor-tests/src/test/scala/org/apache/pekko/event/EventStreamSpec.scala
//
// The original file holds one suite of 17 cases. The port keeps every case and
// its name, and it keeps the class hierarchy (A, B2, B3, C) and the trait
// hierarchy (T, AT, ATT, BT, BTT, CC, CCATBT) the original declares. Rust has
// no run-time class hierarchy, so each case declares the super-type relation
// explicitly on the stream before it subscribes: Pekko reads the relation from
// `Class.isAssignableFrom`, the port from the declared subclasses.
//
// Five differences need a note:
//   * The original names loggers, the log level, and the unhandled-message
//     switch in configuration text. The goal does not read configuration text,
//     so the port passes the same choices programmatically: the loggers and the
//     level go to `start_default_loggers`, the unhandled forwarder is started by
//     `start_unhandled_message_forwarder`, and the debug flag is the second
//     argument of `EventStream::new`.
//   * The two null cases (subscribe and unsubscribe) pass `None`, which is the
//     owner decision that maps Scala's `null` to `Option`; the rejection is the
//     `NullSubscriber` whose message is "subscriber is null".
//   * Cases 13 to 17 reach the stream as `sys.eventStream` of a freshly started
//     system, whose start-up already ran the unsubscriber with the configured
//     debug flag. The port's `ActorSystem::new` does not start an unsubscriber,
//     so those cases build `EventStream::new(<system>, true)` and call
//     `start_unsubscriber` themselves; the state the cases observe is the same.
//     Case 15 likewise starts the unsubscriber explicitly before it asks
//     `init_unsubscriber` to initialize it a second time.
//   * Case 14 uses an actor of the same system as the stream. Pekko watches
//     across systems; the port's `watch` is limited to one system, which is the
//     documented limit of the port, and the case only asks that a subscriber
//     which is already terminated is unsubscribed at once.
//   * `within` runs its body with the waits clamped to the time that is left, so
//     the final `expectNoMessage()` inside it checks up to the end of the scope,
//     as Pekko's `remainingOrDefault` does.
//
// Two helpers stand in for original helpers: `expect_msg_all_of` cannot be
// ported as value equality over heterogeneous messages, so case 4 takes both
// messages with `receive_n` and looks each one up by type; and
// `expectMsgType[AT]` cannot downcast to a declared parent in Rust, so the
// cases assert the concrete event the message carries (`CCATBT`), which is the
// value the original's cast yields.

use core::any::Any;
use core::time::Duration;

use fraktor_actor::kernel::actor::Logging::{
  Debug, Error, Info, InitializeLogger, LogEvent, LogLevel, LoggerInitialized, Warning,
};
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props, UnhandledMessage};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::event::{EventClass, EventStream};
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::implicit_sender::ImplicitSender;
use fraktor_testkit::test_kit::TestProbe;

// Pekko's argument-less expects use `single-expect-default`; the port spells
// that wait out.
fn default_timeout() -> Duration {
  Duration::from_secs(3)
}

fn one_second() -> Duration {
  Duration::from_secs(1)
}

fn poll_interval() -> Duration {
  Duration::from_millis(100)
}

// Original: `EventStreamSpec.M(i: Int)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct M(i32);

// Original: `EventStreamSpec.SetTarget(ref: ActorRef)`.
//
// The field is called `target` because `ref` is a Rust keyword.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SetTarget {
  target: ActorRef,
}

// Original: `EventStreamSpec.MyLog`. The logger subscribes to `SetTarget` and
// `UnhandledMessage` when it is initialized, answers "OK" when it learns where
// to forward, and forwards every log event and unhandled message to that
// destination.
//
// Pekko starts the destination at `context.system.deadLetters`; nothing is
// forwarded before `SetTarget` arrives, so the port keeps no destination until
// then.
struct MyLog {
  destination: Option<ActorRef>,
}

impl MyLog {
  fn new() -> Self {
    Self { destination: None }
  }
}

impl Actor for MyLog {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(initialize) = message.downcast_ref::<InitializeLogger>() {
      let bus = initialize.bus();
      bus.subscribe(Some(context.self_ref()), EventClass::of::<SetTarget>()).expect("the logger is not null");
      bus.subscribe(Some(context.self_ref()), EventClass::of::<UnhandledMessage>()).expect("the logger is not null");
      let sender = context.sender();
      context.tell(&sender, LoggerInitialized);
      return;
    }
    if let Some(set_target) = message.downcast_ref::<SetTarget>() {
      self.destination = Some(set_target.target.clone());
      let target = set_target.target.clone();
      context.tell(&target, Message::new("OK"));
      return;
    }
    if message.is::<Debug>()
      || message.is::<Info>()
      || message.is::<Warning>()
      || message.is::<Error>()
      || message.is::<UnhandledMessage>()
    {
      if let Some(destination) = self.destination.clone() {
        context.tell(&destination, message);
      }
    }
  }
}

// Original: the actor of case 13 that forwards every message to a probe.
struct ForwardToProbe {
  target: ActorRef,
}

impl ForwardToProbe {
  fn new(target: ActorRef) -> Self {
    Self { target }
  }
}

impl Actor for ForwardToProbe {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    self.target.forward(message, context);
  }
}

// Original: the actor of case 14 that ignores every message.
struct IgnoringActor;

impl Actor for IgnoringActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

// Original: `EventStreamSpec.A`, `B2`, `B3`, `C`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct A;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct B2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct B3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct C;

// Original: `EventStreamSpec.T`, `AT`, `ATT`, `BT`, `BTT`, `CC`, `CCATBT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct T;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct At;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Att;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Bt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Btt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CcAtBt;

// Original: the class and trait hierarchy that `classOf` answers in Pekko.
// `CcAtBt` extends `CC`, `ATT`, and `BTT`; the declarations repeat the original
// parents instead of only the direct ones, as Scala writes them.
fn declare_event_stream_hierarchy(stream: &EventStream) {
  stream.declare_subclass::<B2, A>();
  stream.declare_subclass::<B3, A>();
  stream.declare_subclass::<C, B2>();
  stream.declare_subclass::<At, T>();
  stream.declare_subclass::<Att, At>();
  stream.declare_subclass::<Bt, T>();
  stream.declare_subclass::<Btt, Bt>();
  stream.declare_subclass::<CcAtBt, Cc>();
  stream.declare_subclass::<CcAtBt, Att>();
  stream.declare_subclass::<CcAtBt, Btt>();
}

// Original: `expectMsg(event)`.
fn expect_event<E: Any + Clone + PartialEq + core::fmt::Debug>(spec: &mut FraktorSpec, expected: &E) {
  let received: E = spec.expect_msg_type();
  assert_eq!(&received, expected);
}

// Original: `fishForDebugMessage(a, messagePrefix)`.
fn fish_for_debug_message(probe: &mut TestProbe, message_prefix: &str) {
  probe.fish_for_message(default_timeout(), |message| match message.downcast_ref::<Debug>() {
    | Some(debug) => debug.message().starts_with(message_prefix),
    | None => false,
  });
}

// Original: `verifyLevel(bus, level)`.
//
// The original builds one `Seq[LogEvent]`; a Rust `publish` names the concrete
// type, so the port publishes each event on its own and then takes the events
// whose level the logger accepted, in the same order the original publishes.
fn verify_level(bus: &EventStream, spec: &mut FraktorSpec, level: LogLevel) {
  let debug = Debug::new(String::new(), 0, "debug");
  let info = Info::new(String::new(), 0, "info");
  let warning = Warning::new(String::new(), 0, "warning");
  let error = Error::new(String::new(), 0, "error");
  bus.publish(debug);
  bus.publish(info);
  bus.publish(warning);
  bus.publish(error);
  if LogLevel::Debug <= level {
    let event: Debug = spec.expect_msg_type();
    assert_eq!(event.message(), "debug");
  }
  if LogLevel::Info <= level {
    let event: Info = spec.expect_msg_type();
    assert_eq!(event.message(), "info");
  }
  if LogLevel::Warning <= level {
    let event: Warning = spec.expect_msg_type();
    assert_eq!(event.message(), "warning");
  }
  if LogLevel::Error <= level {
    let event: Error = spec.expect_msg_type();
    assert_eq!(event.message(), "error");
  }
}

// Original: `"manage subscriptions" in { ... }`
#[test]
fn an_event_stream_must_manage_subscriptions() {
  let mut spec = FraktorSpec::new("EventStreamSpec");
  let bus = EventStream::new(spec.system().downgrade(), true);
  bus.start_unsubscriber();
  let subscriber = spec.test_actor();

  bus.subscribe(Some(subscriber.clone()), EventClass::of::<M>()).expect("the test actor is not null");
  bus.publish(M(42));
  spec.within(default_timeout(), |spec| {
    expect_event(spec, &M(42));
    bus.unsubscribe_all(Some(subscriber.clone())).expect("the test actor is not null");
    bus.publish(M(13));
    spec.test_kit_mut().expect_no_message(default_timeout());
  });
  spec.terminate();
}

// Original: `"not allow null as subscriber" in { ... }`
#[test]
fn an_event_stream_must_not_allow_null_as_subscriber() {
  let mut spec = FraktorSpec::new("EventStreamSpec");
  let bus = EventStream::new(spec.system().downgrade(), true);

  let error = bus.subscribe(None, EventClass::of::<M>()).expect_err("a null subscriber is refused");
  assert_eq!(error.to_string(), "subscriber is null");
  spec.terminate();
}

// Original: `"not allow null as unsubscriber" in { ... }`
#[test]
fn an_event_stream_must_not_allow_null_as_unsubscriber() {
  let mut spec = FraktorSpec::new("EventStreamSpec");
  let bus = EventStream::new(spec.system().downgrade(), true);

  let error = bus.unsubscribe(None, EventClass::of::<M>()).expect_err("a null subscriber is refused");
  assert_eq!(error.to_string(), "subscriber is null");
  let error = bus.unsubscribe_all(None).expect_err("a null subscriber is refused");
  assert_eq!(error.to_string(), "subscriber is null");
  spec.terminate();
}

// Original: `"be able to log unhandled messages" in { ... }`
//
// The original publishes into the stream of a second system and compares the
// published message and the Debug event with `expectMsgAllOf`. The port takes
// both messages under one deadline and looks each one up by type, which is the
// same "one of each, nothing else" contract without depending on the order the
// stream delivers them in.
#[test]
fn an_event_stream_must_be_able_to_log_unhandled_messages() {
  let mut spec = FraktorSpec::new("EventStreamSpec");
  let mut other = FraktorSpec::new("EventStreamSpecUnhandled");
  let stream = other.system().event_stream();
  stream.start_unhandled_message_forwarder();
  let dead_letters = other.system().dead_letters();
  let subscriber = spec.test_actor();
  stream.subscribe(Some(subscriber.clone()), EventClass::any()).expect("the test actor is not null");

  stream.publish(UnhandledMessage::new(42, dead_letters.clone(), dead_letters.clone()));
  let messages = spec.test_kit_mut().receive_n(2, default_timeout());
  assert_eq!(messages.len(), 2);

  let unhandled = messages
    .iter()
    .find_map(|message| message.downcast_ref::<UnhandledMessage>())
    .expect("the unhandled message was not delivered");
  assert_eq!(unhandled.message().downcast_ref::<i32>(), Some(&42));
  assert_eq!(unhandled.sender(), &dead_letters);
  assert_eq!(unhandled.recipient(), &dead_letters);

  let debug =
    messages.iter().find_map(|message| message.downcast_ref::<Debug>()).expect("the debug event was not delivered");
  assert_eq!(debug.level(), LogLevel::Debug);
  assert_eq!(debug.log_source(), dead_letters.path().to_string());
  assert_eq!(debug.message(), format!("unhandled message from {}: 42", dead_letters.path()));

  stream.unsubscribe_all(Some(subscriber)).expect("the test actor is not null");
  spec.terminate();
  other.terminate();
}

// Original: `"manage log levels" in { ... }`
#[test]
fn an_event_stream_must_manage_log_levels() {
  let mut spec = FraktorSpec::new("EventStreamSpec");
  let bus = EventStream::new(spec.system().downgrade(), false);
  let logger = spec.system_mut().actor_of(Props::new(MyLog::new));
  bus.start_default_loggers(&[logger], LogLevel::Info);

  bus.publish(SetTarget { target: spec.test_actor() });
  spec.expect_msg("OK");
  spec.within(default_timeout(), |spec| {
    verify_level(&bus, spec, LogLevel::Info);
    bus.set_log_level(LogLevel::Warning);
    verify_level(&bus, spec, LogLevel::Warning);
    bus.set_log_level(LogLevel::Debug);
    verify_level(&bus, spec, LogLevel::Debug);
    bus.set_log_level(LogLevel::Error);
    verify_level(&bus, spec, LogLevel::Error);
  });
  spec.terminate();
}

// Original: `"manage sub-channels using classes" in { ... }`
#[test]
fn an_event_stream_must_manage_sub_channels_using_classes() {
  let mut spec = FraktorSpec::new("EventStreamSpec");
  let bus = EventStream::new(spec.system().downgrade(), false);
  declare_event_stream_hierarchy(&bus);
  let a = A;
  let b2 = B2;
  let b3 = B3;
  let c = C;
  let subscriber = spec.test_actor();

  spec.within(default_timeout(), |spec| {
    assert!(bus.subscribe(Some(subscriber.clone()), EventClass::of::<B3>()).expect("the test actor is not null"));
    bus.publish(c);
    bus.publish(b3);
    expect_event(spec, &b3);

    assert!(bus.subscribe(Some(subscriber.clone()), EventClass::of::<A>()).expect("the test actor is not null"));
    bus.publish(c);
    expect_event(spec, &c);
    bus.publish(b2);
    expect_event(spec, &b2);

    // Pekko's `unsubscribe(B2)` removes the value from the B2 key although B2
    // was only reached through the subscription to A, and answers `true`.
    assert!(bus.unsubscribe(Some(subscriber.clone()), EventClass::of::<B2>()).expect("the test actor is not null"));
    bus.publish(c);
    bus.publish(b3);
    bus.publish(a);
    expect_event(spec, &b3);
    expect_event(spec, &a);
    spec.test_kit_mut().expect_no_message(default_timeout());
  });
  spec.terminate();
}

// Original: `"... (update on subscribe)" in { ... }`
#[test]
fn an_event_stream_must_manage_sub_channels_using_classes_and_traits_update_on_subscribe() {
  let mut spec = FraktorSpec::new("EventStreamSpec");
  let bus = EventStream::new(spec.system().downgrade(), false);
  declare_event_stream_hierarchy(&bus);
  let tm1 = Cc;
  let tm2 = CcAtBt;
  let mut a1 = TestProbe::new(spec.system_mut());
  let mut a2 = TestProbe::new(spec.system_mut());
  let mut a3 = TestProbe::new(spec.system_mut());
  let mut a4 = TestProbe::new(spec.system_mut());

  assert!(bus.subscribe(Some(a1.ref_handle()), EventClass::of::<At>()).expect("the probe is not null"));
  assert!(bus.subscribe(Some(a2.ref_handle()), EventClass::of::<Bt>()).expect("the probe is not null"));
  assert!(bus.subscribe(Some(a3.ref_handle()), EventClass::of::<Cc>()).expect("the probe is not null"));
  assert!(bus.subscribe(Some(a4.ref_handle()), EventClass::of::<CcAtBt>()).expect("the probe is not null"));
  bus.publish(tm1);
  bus.publish(tm2);
  // The original casts the received value to the subscribed trait; the port
  // asserts the concrete event, which is the value that cast yields.
  let received: CcAtBt = a1.expect_msg_type();
  assert_eq!(received, tm2);
  let received: CcAtBt = a2.expect_msg_type();
  assert_eq!(received, tm2);
  let received: Cc = a3.expect_msg_type();
  assert_eq!(received, tm1);
  // The second event is a CCATBT, which reaches a3 through its CC
  // subscription; the port keeps the concrete type the message carries.
  let received: CcAtBt = a3.expect_msg_type();
  assert_eq!(received, tm2);
  let received: CcAtBt = a4.expect_msg_type();
  assert_eq!(received, tm2);

  assert!(bus.unsubscribe(Some(a1.ref_handle()), EventClass::of::<At>()).expect("the probe is not null"));
  assert!(bus.unsubscribe(Some(a2.ref_handle()), EventClass::of::<Bt>()).expect("the probe is not null"));
  assert!(bus.unsubscribe(Some(a3.ref_handle()), EventClass::of::<Cc>()).expect("the probe is not null"));
  assert!(bus.unsubscribe(Some(a4.ref_handle()), EventClass::of::<CcAtBt>()).expect("the probe is not null"));
  spec.terminate();
}

// Original: `"... (update on unsubscribe)" in { ... }`
#[test]
fn an_event_stream_must_manage_sub_channels_using_classes_and_traits_update_on_unsubscribe() {
  let mut spec = FraktorSpec::new("EventStreamSpec");
  let bus = EventStream::new(spec.system().downgrade(), false);
  declare_event_stream_hierarchy(&bus);
  let tm1 = Cc;
  let tm2 = CcAtBt;
  let mut a1 = TestProbe::new(spec.system_mut());
  let mut a2 = TestProbe::new(spec.system_mut());
  let mut a3 = TestProbe::new(spec.system_mut());
  let mut a4 = TestProbe::new(spec.system_mut());

  assert!(bus.subscribe(Some(a1.ref_handle()), EventClass::of::<At>()).expect("the probe is not null"));
  assert!(bus.subscribe(Some(a2.ref_handle()), EventClass::of::<Bt>()).expect("the probe is not null"));
  assert!(bus.subscribe(Some(a3.ref_handle()), EventClass::of::<Cc>()).expect("the probe is not null"));
  assert!(bus.subscribe(Some(a4.ref_handle()), EventClass::of::<CcAtBt>()).expect("the probe is not null"));
  assert!(bus.unsubscribe(Some(a3.ref_handle()), EventClass::of::<Cc>()).expect("the probe is not null"));
  bus.publish(tm1);
  bus.publish(tm2);
  let received: CcAtBt = a1.expect_msg_type();
  assert_eq!(received, tm2);
  let received: CcAtBt = a2.expect_msg_type();
  assert_eq!(received, tm2);
  a3.expect_no_message(one_second());
  let received: CcAtBt = a4.expect_msg_type();
  assert_eq!(received, tm2);

  assert!(bus.unsubscribe(Some(a1.ref_handle()), EventClass::of::<At>()).expect("the probe is not null"));
  assert!(bus.unsubscribe(Some(a2.ref_handle()), EventClass::of::<Bt>()).expect("the probe is not null"));
  assert!(bus.unsubscribe(Some(a4.ref_handle()), EventClass::of::<CcAtBt>()).expect("the probe is not null"));
  spec.terminate();
}

// Original: `"... (update on unsubscribe all)" in { ... }`
#[test]
fn an_event_stream_must_manage_sub_channels_using_classes_and_traits_update_on_unsubscribe_all() {
  let mut spec = FraktorSpec::new("EventStreamSpec");
  let bus = EventStream::new(spec.system().downgrade(), false);
  declare_event_stream_hierarchy(&bus);
  let tm1 = Cc;
  let tm2 = CcAtBt;
  let mut a1 = TestProbe::new(spec.system_mut());
  let mut a2 = TestProbe::new(spec.system_mut());
  let mut a3 = TestProbe::new(spec.system_mut());
  let mut a4 = TestProbe::new(spec.system_mut());

  assert!(bus.subscribe(Some(a1.ref_handle()), EventClass::of::<At>()).expect("the probe is not null"));
  assert!(bus.subscribe(Some(a2.ref_handle()), EventClass::of::<Bt>()).expect("the probe is not null"));
  assert!(bus.subscribe(Some(a3.ref_handle()), EventClass::of::<Cc>()).expect("the probe is not null"));
  assert!(bus.subscribe(Some(a4.ref_handle()), EventClass::of::<CcAtBt>()).expect("the probe is not null"));
  bus.unsubscribe_all(Some(a3.ref_handle())).expect("the probe is not null");
  bus.publish(tm1);
  bus.publish(tm2);
  let received: CcAtBt = a1.expect_msg_type();
  assert_eq!(received, tm2);
  let received: CcAtBt = a2.expect_msg_type();
  assert_eq!(received, tm2);
  a3.expect_no_message(one_second());
  let received: CcAtBt = a4.expect_msg_type();
  assert_eq!(received, tm2);

  assert!(bus.unsubscribe(Some(a1.ref_handle()), EventClass::of::<At>()).expect("the probe is not null"));
  assert!(bus.unsubscribe(Some(a2.ref_handle()), EventClass::of::<Bt>()).expect("the probe is not null"));
  assert!(bus.unsubscribe(Some(a4.ref_handle()), EventClass::of::<CcAtBt>()).expect("the probe is not null"));
  spec.terminate();
}

// Original: `"... (update on publish)" in { ... }`
#[test]
fn an_event_stream_must_manage_sub_channels_using_classes_and_traits_update_on_publish() {
  let mut spec = FraktorSpec::new("EventStreamSpec");
  let bus = EventStream::new(spec.system().downgrade(), false);
  declare_event_stream_hierarchy(&bus);
  let tm1 = Cc;
  let tm2 = CcAtBt;
  let mut a1 = TestProbe::new(spec.system_mut());
  let mut a2 = TestProbe::new(spec.system_mut());

  assert!(bus.subscribe(Some(a1.ref_handle()), EventClass::of::<At>()).expect("the probe is not null"));
  assert!(bus.subscribe(Some(a2.ref_handle()), EventClass::of::<Bt>()).expect("the probe is not null"));
  bus.publish(tm1);
  bus.publish(tm2);
  let received: CcAtBt = a1.expect_msg_type();
  assert_eq!(received, tm2);
  let received: CcAtBt = a2.expect_msg_type();
  assert_eq!(received, tm2);

  assert!(bus.unsubscribe(Some(a1.ref_handle()), EventClass::of::<At>()).expect("the probe is not null"));
  assert!(bus.unsubscribe(Some(a2.ref_handle()), EventClass::of::<Bt>()).expect("the probe is not null"));
  spec.terminate();
}

// Original: `"... (unsubscribe classes used with trait)" in { ... }`
#[test]
fn an_event_stream_must_manage_sub_channels_using_classes_and_traits_unsubscribe_classes_used_with_trait() {
  let mut spec = FraktorSpec::new("EventStreamSpec");
  let bus = EventStream::new(spec.system().downgrade(), false);
  declare_event_stream_hierarchy(&bus);
  let tm1 = Cc;
  let tm2 = CcAtBt;
  let mut a1 = TestProbe::new(spec.system_mut());
  let mut a2 = TestProbe::new(spec.system_mut());
  let mut a3 = TestProbe::new(spec.system_mut());

  assert!(bus.subscribe(Some(a1.ref_handle()), EventClass::of::<At>()).expect("the probe is not null"));
  assert!(bus.subscribe(Some(a2.ref_handle()), EventClass::of::<Bt>()).expect("the probe is not null"));
  assert!(bus.subscribe(Some(a2.ref_handle()), EventClass::of::<Cc>()).expect("the probe is not null"));
  assert!(bus.subscribe(Some(a3.ref_handle()), EventClass::of::<Cc>()).expect("the probe is not null"));
  assert!(bus.unsubscribe(Some(a2.ref_handle()), EventClass::of::<Cc>()).expect("the probe is not null"));
  // The probe subscribed to CC, so unsubscribing it from CCATBT removes the
  // subscription CC gave it to that sub-channel, and answers `true`.
  assert!(bus.unsubscribe(Some(a3.ref_handle()), EventClass::of::<CcAtBt>()).expect("the probe is not null"));
  bus.publish(tm1);
  bus.publish(tm2);
  let received: CcAtBt = a1.expect_msg_type();
  assert_eq!(received, tm2);
  let received: CcAtBt = a2.expect_msg_type();
  assert_eq!(received, tm2);
  let received: Cc = a3.expect_msg_type();
  assert_eq!(received, tm1);

  assert!(bus.unsubscribe(Some(a1.ref_handle()), EventClass::of::<At>()).expect("the probe is not null"));
  assert!(bus.unsubscribe(Some(a2.ref_handle()), EventClass::of::<Bt>()).expect("the probe is not null"));
  assert!(bus.unsubscribe(Some(a3.ref_handle()), EventClass::of::<Cc>()).expect("the probe is not null"));
  spec.terminate();
}

// Original: `"... (subscribe after publish)" in { ... }`
#[test]
fn an_event_stream_must_manage_sub_channels_using_classes_and_traits_subscribe_after_publish() {
  let mut spec = FraktorSpec::new("EventStreamSpec");
  let bus = EventStream::new(spec.system().downgrade(), false);
  declare_event_stream_hierarchy(&bus);
  let tm1 = CcAtBt;
  let mut a1 = TestProbe::new(spec.system_mut());
  let mut a2 = TestProbe::new(spec.system_mut());

  assert!(bus.subscribe(Some(a1.ref_handle()), EventClass::of::<At>()).expect("the probe is not null"));
  bus.publish(tm1);
  let received: CcAtBt = a1.expect_msg_type();
  assert_eq!(received, tm1);
  a2.expect_no_message(one_second());
  assert!(bus.subscribe(Some(a2.ref_handle()), EventClass::of::<Btt>()).expect("the probe is not null"));
  bus.publish(tm1);
  let received: CcAtBt = a1.expect_msg_type();
  assert_eq!(received, tm1);
  let received: CcAtBt = a2.expect_msg_type();
  assert_eq!(received, tm1);

  assert!(bus.unsubscribe(Some(a1.ref_handle()), EventClass::of::<At>()).expect("the probe is not null"));
  assert!(bus.unsubscribe(Some(a2.ref_handle()), EventClass::of::<Btt>()).expect("the probe is not null"));
  spec.terminate();
}

// Original: `"unsubscribe an actor on its termination" in { ... }`
#[test]
fn an_event_stream_must_unsubscribe_an_actor_on_its_termination() {
  let mut spec = FraktorSpec::new("EventStreamSpecUnsubscribeOnTerminated");
  let stream = EventStream::new(spec.system().downgrade(), true);
  stream.start_unsubscriber();
  let mut a1 = TestProbe::new(spec.system_mut());
  let mut a2 = TestProbe::new(spec.system_mut());
  let a1_ref = a1.ref_handle();
  let target = spec.system_mut().actor_of(Props::new(move || ForwardToProbe::new(a1_ref.clone())));
  let tm = A;

  assert!(stream.subscribe(Some(a2.ref_handle()), EventClass::any()).expect("the probe is not null"));
  assert!(stream.subscribe(Some(target.clone()), EventClass::of::<A>()).expect("the actor is not null"));
  assert!(!stream.subscribe(Some(target.clone()), EventClass::of::<A>()).expect("the actor is not null"));

  let sender = ImplicitSender::self_actor_ref(&spec);
  target.tell("PoisonPill", sender);
  fish_for_debug_message(&mut a2, &format!("unsubscribing {} from all channels", target.path()));
  fish_for_debug_message(&mut a2, &format!("unwatching {}", target.path()));

  stream.publish(tm);
  a1.expect_no_message(one_second());
  let received: A = a2.expect_msg_type();
  assert_eq!(received, tm);
  spec.terminate();
}

// Original: `"unsubscribe the actor, when it subscribes already in terminated state" in { ... }`
#[test]
fn an_event_stream_must_unsubscribe_the_actor_when_it_subscribes_already_in_terminated_state() {
  let mut spec = FraktorSpec::new("EventStreamSpecUnsubscribeTerminated");
  let stream = EventStream::new(spec.system().downgrade(), true);
  stream.start_unsubscriber();
  let mut probe = TestProbe::new(spec.system_mut());

  let terminated = spec.system_mut().actor_of(Props::new(|| IgnoringActor));
  spec.test_kit_mut().watch(&terminated);
  let sender = ImplicitSender::self_actor_ref(&spec);
  terminated.tell("PoisonPill", sender);
  spec.test_kit_mut().expect_msg_terminated(default_timeout());

  assert!(stream.subscribe(Some(probe.ref_handle()), EventClass::any()).expect("the probe is not null"));
  // The actor is terminated; the subscription is removed again right away.
  assert!(stream.subscribe(Some(terminated.clone()), EventClass::of::<A>()).expect("the actor is not null"));
  fish_for_debug_message(&mut probe, &format!("unsubscribing {} from all channels", terminated.path()));

  // Once unsubscribed, the same terminated actor can subscribe again.
  let resubscribed = spec.await_assert(default_timeout(), poll_interval(), || {
    stream.subscribe(Some(terminated.clone()), EventClass::of::<A>()).expect("the actor is not null")
  });
  assert!(resubscribed);
  fish_for_debug_message(&mut probe, &format!("unsubscribing {} from all channels", terminated.path()));
  spec.terminate();
}

// Original: `"not allow initializing a TerminatedUnsubscriber twice" in { ... }`
#[test]
fn an_event_stream_must_not_allow_initializing_a_terminated_unsubscriber_twice() {
  let mut spec = FraktorSpec::new("MustNotAllowDoubleInitOfTerminatedUnsubscriber");
  let stream = EventStream::new(spec.system().downgrade(), false);
  // Pekko's `ActorSystem` starts the unsubscriber during start-up; the port's
  // `ActorSystem::new` does not, so the test starts it and then asks for a
  // second initialization, which is the state the original observes.
  stream.start_unsubscriber();
  let probe = TestProbe::new(spec.system_mut());

  let used_as_unsubscriber = stream.init_unsubscriber(probe.ref_handle());
  assert!(!used_as_unsubscriber);
  spec.terminate();
}

// Original: `"unwatch an actor from unsubscriber when that actor unsubscribes from the stream" in { ... }`
#[test]
fn an_event_stream_must_unwatch_an_actor_from_unsubscriber_when_that_actor_unsubscribes_from_the_stream() {
  let mut spec = FraktorSpec::new("MustUnregisterDuringUnsubscribe");
  let stream = EventStream::new(spec.system().downgrade(), true);
  stream.start_unsubscriber();
  let mut a1 = TestProbe::new(spec.system_mut());
  let mut a2 = TestProbe::new(spec.system_mut());
  let a2_ref = a2.ref_handle();

  assert!(stream.subscribe(Some(a1.ref_handle()), EventClass::of::<Debug>()).expect("the probe is not null"));
  assert!(stream.subscribe(Some(a2_ref.clone()), EventClass::of::<A>()).expect("the probe is not null"));
  fish_for_debug_message(&mut a1, &format!("watching {}", a2_ref.path()));

  stream.unsubscribe_all(Some(a2_ref.clone())).expect("the probe is not null");
  fish_for_debug_message(&mut a1, &format!("unwatching {}", a2_ref.path()));
  spec.terminate();
}

// Original: `"unwatch an actor from unsubscriber when that actor unsubscribes from channels it subscribed" in { ... }`
#[test]
fn an_event_stream_must_unwatch_an_actor_from_unsubscriber_when_that_actor_unsubscribes_from_channels_it_subscribed() {
  let mut spec = FraktorSpec::new("MustUnregisterWhenNoMoreChannelSubscriptions");
  let stream = EventStream::new(spec.system().downgrade(), true);
  stream.start_unsubscriber();
  declare_event_stream_hierarchy(&stream);
  let mut a1 = TestProbe::new(spec.system_mut());
  let mut a2 = TestProbe::new(spec.system_mut());
  let a2_ref = a2.ref_handle();

  assert!(stream.subscribe(Some(a1.ref_handle()), EventClass::of::<Debug>()).expect("the probe is not null"));
  assert!(stream.subscribe(Some(a2_ref.clone()), EventClass::of::<A>()).expect("the probe is not null"));
  assert!(stream.subscribe(Some(a2_ref.clone()), EventClass::of::<T>()).expect("the probe is not null"));
  // The unsubscriber starts to watch each time; watching is idempotent.
  fish_for_debug_message(&mut a1, &format!("watching {}", a2_ref.path()));
  fish_for_debug_message(&mut a1, &format!("watching {}", a2_ref.path()));

  assert!(stream.unsubscribe(Some(a2_ref.clone()), EventClass::of::<A>()).expect("the probe is not null"));
  fish_for_debug_message(&mut a1, &format!("unsubscribing {} from channel {}", a2_ref.path(), EventClass::of::<A>()));
  a1.expect_no_message(one_second());

  assert!(stream.unsubscribe(Some(a2_ref.clone()), EventClass::of::<T>()).expect("the probe is not null"));
  // The two entries are produced on different paths, so their order is not
  // fixed; both must arrive.
  let first: Debug = a1.expect_msg_type();
  let second: Debug = a1.expect_msg_type();
  let entries = [first.message().to_string(), second.message().to_string()];
  let channel_t = format!("unsubscribing {} from channel {}", a2_ref.path(), EventClass::of::<T>());
  let unwatching = format!("unwatching {}, since has no subscriptions", a2_ref.path());
  assert!(entries.iter().any(|entry| entry.starts_with(&channel_t)));
  assert!(entries.iter().any(|entry| entry.starts_with(&unwatching)));
  a1.expect_no_message(one_second());

  assert!(!stream.unsubscribe(Some(a2_ref.clone()), EventClass::of::<T>()).expect("the probe is not null"));
  spec.terminate();
}
