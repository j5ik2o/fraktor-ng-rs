// Ported from Pekko v1.7.1:
//   actor-tests/src/test/scala/org/apache/pekko/event/EventBusSpec.scala
//
// The original file holds one abstract suite of 13 cases that each concrete bus
// runs once (LookupEventBus, ScanningEventBus, ActorEventBus), plus three cases
// that only ActorEventBus runs. The port keeps all 42 executed cases and the
// original names; the 13 shared bodies are generic over a fixture, so the three
// buses share them here the same way they share them in Pekko.
//
// Two differences need a note:
//   * Pekko's `mapSize` is a size hint for the classifier index and is not
//     observable from a test, so the port does not name it.
//   * `ActorEventBusSpec` switches `pekko.actor.debug.event-stream` on through
//     configuration. The goal does not read configuration text, so the port
//     passes `debug = true` to the bus instead, which is the same on/off face.
//
// The original shares one subscriber, one event, and one classifier across the
// 13 abstract cases because ScalaTest builds them once per suite. The port
// gives every case its own fixture and its own subscriber, so no case depends
// on the order of another. The one case whose original body relies on the
// previous case's subscription is noted where it is ported.

use core::cmp::Ordering;
use core::time::Duration;

use fraktor_actor::kernel::actor::Logging::Debug as LoggingDebug;
use fraktor_actor::kernel::actor::Logging::LogEvent;
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::event::{ActorEventBus, EventClass, LookupEventBus, ScanningEventBus};
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::implicit_sender::ImplicitSender;
use fraktor_testkit::test_kit::TestProbe;

// Pekko's `expectMsg` and `expectTerminated` use the test kit's
// `single-expect-default`; the port spells that wait out explicitly where a
// bare `TestProbe` or `TestKit` takes a duration.
fn default_timeout() -> Duration {
  Duration::from_secs(3)
}

// Original: `EventBusSpec.TestActorWrapperActor`, which forwards every message
// it receives to the test actor (`testActor.forward(x)`). The port keeps the
// same actor: the bus sends the event to the wrapper, and the wrapper hands the
// same `Message` to the test actor.
struct TestActorWrapperActor {
  test_actor: ActorRef,
}

impl TestActorWrapperActor {
  fn new(test_actor: ActorRef) -> Self {
    Self { test_actor }
  }
}

impl Actor for TestActorWrapperActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    context.tell(&self.test_actor, message);
  }
}

// Original: `ActorEventBusSpec.Notification(ref, payload)`.
//
// `ref` is called `reference` because `ref` is a Rust keyword.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Notification {
  reference: ActorRef,
  payload: i32,
}

// A subscriber of the Lookup and Scanning buses.
//
// Pekko's two buses use a `Procedure[Int]` as the subscriber and order two
// subscribers with `pekko.util.Helpers.compareIdentityHash`. A Rust closure has
// no identity to order by, so the port gives each subscriber a unique id and
// orders by that id, which is the same "identity, not equality" order.
#[derive(Clone, Debug)]
struct TestSubscriber {
  id: u64,
  target: ActorRef,
}

impl PartialEq for TestSubscriber {
  fn eq(&self, other: &Self) -> bool {
    self.id == other.id
  }
}

impl Eq for TestSubscriber {}

impl PartialOrd for TestSubscriber {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for TestSubscriber {
  fn cmp(&self, other: &Self) -> Ordering {
    self.id.cmp(&other.id)
  }
}

// The shape of `EventBusSpec`'s abstract suite.
//
// The original declares the bus's `Event`, `Classifier`, and `Subscriber` as
// abstract type members and writes 13 cases against them. The port spells the
// same three types as associated types of this trait, so one generic body can
// stand for each original case.
trait EventBusUnderTest {
  type Event: Clone + PartialEq + core::fmt::Debug + 'static;
  type Classifier: Clone + PartialEq + core::fmt::Debug;
  type Subscriber: Clone + core::fmt::Debug;

  fn test_actor(&self) -> ActorRef;

  // Original: `createEvents(numberOfEvents)`; the port makes the index explicit
  // because the cases only ever ask for `head` and `drop(1).head`.
  fn create_event(&mut self, index: usize) -> Self::Event;

  // Original: `classifierFor(event)`.
  fn classifier_for(&self, event: &Self::Event) -> Self::Classifier;

  // Original: `createSubscriber(pipeTo)`.
  fn create_subscriber(&mut self, pipe_to: ActorRef) -> Self::Subscriber;

  fn subscribe(&mut self, subscriber: &Self::Subscriber, classifier: &Self::Classifier) -> bool;

  fn unsubscribe(&mut self, subscriber: &Self::Subscriber, classifier: &Self::Classifier) -> bool;

  fn publish(&mut self, event: &Self::Event);

  // Original: `disposeSubscriber(system, subscriber)`.
  fn dispose_subscriber(&mut self, subscriber: &Self::Subscriber);

  // Original: `expectMsg(event)`.
  fn expect_event(&mut self, event: &Self::Event);

  // Original: `expectNoMessage()`.
  fn expect_no_message(&mut self);
}

// Original: `LookupEventBusSpec.MyLookupEventBus` with `LookupClassification`.
struct LookupBusFixture {
  spec: FraktorSpec,
  bus: LookupEventBus<i32, String, TestSubscriber>,
  next_subscriber_id: u64,
}

impl LookupBusFixture {
  fn new() -> Self {
    let spec = FraktorSpec::new("LookupEventBusSpec");
    let bus = LookupEventBus::new(
      |event: &i32| event.to_string(),
      |event: &i32, subscriber: &TestSubscriber| subscriber.target.tell(*event, subscriber.target.clone()),
    );
    Self { spec, bus, next_subscriber_id: 0 }
  }
}

impl EventBusUnderTest for LookupBusFixture {
  type Event = i32;
  type Classifier = String;
  type Subscriber = TestSubscriber;

  fn test_actor(&self) -> ActorRef {
    self.spec.test_actor()
  }

  fn create_event(&mut self, index: usize) -> i32 {
    index as i32
  }

  fn classifier_for(&self, event: &i32) -> String {
    event.to_string()
  }

  fn create_subscriber(&mut self, pipe_to: ActorRef) -> TestSubscriber {
    let id = self.next_subscriber_id;
    self.next_subscriber_id += 1;
    TestSubscriber { id, target: pipe_to }
  }

  fn subscribe(&mut self, subscriber: &TestSubscriber, classifier: &String) -> bool {
    self.bus.subscribe(subscriber.clone(), classifier.clone())
  }

  fn unsubscribe(&mut self, subscriber: &TestSubscriber, classifier: &String) -> bool {
    self.bus.unsubscribe(subscriber, classifier)
  }

  fn publish(&mut self, event: &i32) {
    self.bus.publish(event);
  }

  fn dispose_subscriber(&mut self, _subscriber: &TestSubscriber) {}

  fn expect_event(&mut self, event: &i32) {
    let found: i32 = self.spec.expect_msg_type();
    assert_eq!(found, *event);
  }

  fn expect_no_message(&mut self) {
    expect_no_message(&mut self.spec);
  }
}

impl Drop for LookupBusFixture {
  fn drop(&mut self) {
    self.spec.terminate();
  }
}

// Original: `ScanningEventBusSpec.MyScanningEventBus` with
// `ScanningClassification`.
struct ScanningBusFixture {
  spec: FraktorSpec,
  bus: ScanningEventBus<i32, String, TestSubscriber>,
  next_subscriber_id: u64,
}

impl ScanningBusFixture {
  fn new() -> Self {
    let spec = FraktorSpec::new("ScanningEventBusSpec");
    let bus = ScanningEventBus::new(
      |classifier: &String, event: &i32| event.to_string() == *classifier,
      |event: &i32, subscriber: &TestSubscriber| subscriber.target.tell(*event, subscriber.target.clone()),
    );
    Self { spec, bus, next_subscriber_id: 0 }
  }
}

impl EventBusUnderTest for ScanningBusFixture {
  type Event = i32;
  type Classifier = String;
  type Subscriber = TestSubscriber;

  fn test_actor(&self) -> ActorRef {
    self.spec.test_actor()
  }

  fn create_event(&mut self, index: usize) -> i32 {
    index as i32
  }

  fn classifier_for(&self, event: &i32) -> String {
    event.to_string()
  }

  fn create_subscriber(&mut self, pipe_to: ActorRef) -> TestSubscriber {
    let id = self.next_subscriber_id;
    self.next_subscriber_id += 1;
    TestSubscriber { id, target: pipe_to }
  }

  fn subscribe(&mut self, subscriber: &TestSubscriber, classifier: &String) -> bool {
    self.bus.subscribe(subscriber.clone(), classifier.clone())
  }

  fn unsubscribe(&mut self, subscriber: &TestSubscriber, classifier: &String) -> bool {
    self.bus.unsubscribe(subscriber, classifier)
  }

  fn publish(&mut self, event: &i32) {
    self.bus.publish(event);
  }

  fn dispose_subscriber(&mut self, _subscriber: &TestSubscriber) {}

  fn expect_event(&mut self, event: &i32) {
    let found: i32 = self.spec.expect_msg_type();
    assert_eq!(found, *event);
  }

  fn expect_no_message(&mut self) {
    expect_no_message(&mut self.spec);
  }
}

impl Drop for ScanningBusFixture {
  fn drop(&mut self) {
    self.spec.terminate();
  }
}

// Original: `ActorEventBusSpec.MyActorEventBus` with `ManagedActorClassification`
// and `ActorClassifier`, constructed with `pekko.actor.debug.event-stream = on`.
struct ActorBusFixture {
  spec: FraktorSpec,
  bus: ActorEventBus<Notification>,
}

impl ActorBusFixture {
  fn new() -> Self {
    let spec = FraktorSpec::new("ActorEventBusSpec");
    let bus = ActorEventBus::new(
      spec.system().downgrade(),
      spec.system().event_stream(),
      "ActorEventBusSpec.MyActorEventBus",
      true,
      |event: &Notification| event.reference.clone(),
    );
    Self { spec, bus }
  }
}

impl EventBusUnderTest for ActorBusFixture {
  type Event = Notification;
  type Classifier = ActorRef;
  type Subscriber = ActorRef;

  fn test_actor(&self) -> ActorRef {
    self.spec.test_actor()
  }

  // Original: `createEvents(n) = (0 until n).map(Notification(TestProbe().ref, _))`.
  // Every event carries a fresh probe's ref, so every event has its own
  // classifier, as the comment in the original asks for.
  fn create_event(&mut self, index: usize) -> Notification {
    let probe = TestProbe::with_name(self.spec.system_mut(), "event-classifier");
    Notification { reference: probe.ref_handle(), payload: index as i32 }
  }

  fn classifier_for(&self, event: &Notification) -> ActorRef {
    event.reference.clone()
  }

  fn create_subscriber(&mut self, pipe_to: ActorRef) -> ActorRef {
    let props = Props::new(move || TestActorWrapperActor::new(pipe_to.clone()));
    self.spec.system_mut().actor_of(props)
  }

  fn subscribe(&mut self, subscriber: &ActorRef, classifier: &ActorRef) -> bool {
    self.bus.subscribe(subscriber.clone(), classifier.clone())
  }

  fn unsubscribe(&mut self, subscriber: &ActorRef, classifier: &ActorRef) -> bool {
    self.bus.unsubscribe(subscriber, classifier)
  }

  fn publish(&mut self, event: &Notification) {
    self.bus.publish(event);
  }

  fn dispose_subscriber(&mut self, subscriber: &ActorRef) {
    self.spec.system_mut().stop(subscriber);
  }

  fn expect_event(&mut self, event: &Notification) {
    let found: Notification = self.spec.expect_msg_type();
    assert_eq!(found, *event);
  }

  fn expect_no_message(&mut self) {
    expect_no_message(&mut self.spec);
  }
}

impl Drop for ActorBusFixture {
  fn drop(&mut self) {
    self.spec.terminate();
  }
}

// Pekko's `expectNoMessage()` waits `expect-no-message-default` dilated by the
// test time factor; the port's `TestKit::expect_no_message` takes that duration.
fn expect_no_message(spec: &mut FraktorSpec) {
  let settings = spec.test_kit_settings();
  let max = settings.dilated(settings.expect_no_message_default_timeout());
  spec.test_kit_mut().expect_no_message(max);
}

// Original: `"allow subscribers" in { bus.subscribe(subscriber, classifier) should ===(true) }`
fn shared_allow_subscribers<B: EventBusUnderTest>(fixture: &mut B) {
  let event = fixture.create_event(0);
  let classifier = fixture.classifier_for(&event);
  let pipe_to = fixture.test_actor();
  let subscriber = fixture.create_subscriber(pipe_to);
  assert!(fixture.subscribe(&subscriber, &classifier));
}

// Original: `"allow to unsubscribe already existing subscriber" in { ... }`
//
// The original body relies on the subscriber being subscribed by the previous
// case, because the suite shares it. The port keeps each case independent and
// subscribes first, which tests the same fact: unsubscribing an existing
// subscription answers `true`.
fn shared_allow_to_unsubscribe_already_existing_subscriber<B: EventBusUnderTest>(fixture: &mut B) {
  let event = fixture.create_event(0);
  let classifier = fixture.classifier_for(&event);
  let pipe_to = fixture.test_actor();
  let subscriber = fixture.create_subscriber(pipe_to);
  assert!(fixture.subscribe(&subscriber, &classifier));
  assert!(fixture.unsubscribe(&subscriber, &classifier));
}

// Original: `"not allow to unsubscribe non-existing subscriber" in { ... }`
fn shared_not_allow_to_unsubscribe_non_existing_subscriber<B: EventBusUnderTest>(fixture: &mut B) {
  let event = fixture.create_event(0);
  let classifier = fixture.classifier_for(&event);
  let pipe_to = fixture.test_actor();
  let subscriber = fixture.create_subscriber(pipe_to);
  assert!(!fixture.unsubscribe(&subscriber, &classifier));
  fixture.dispose_subscriber(&subscriber);
}

// Original: `"not allow for the same subscriber to subscribe to the same channel twice" in { ... }`
fn shared_not_allow_for_the_same_subscriber_to_subscribe_to_the_same_channel_twice<B: EventBusUnderTest>(
  fixture: &mut B,
) {
  let event = fixture.create_event(0);
  let classifier = fixture.classifier_for(&event);
  let pipe_to = fixture.test_actor();
  let subscriber = fixture.create_subscriber(pipe_to);
  assert!(fixture.subscribe(&subscriber, &classifier));
  assert!(!fixture.subscribe(&subscriber, &classifier));
  assert!(fixture.unsubscribe(&subscriber, &classifier));
}

// Original: `"not allow for the same subscriber to unsubscribe to the same channel twice" in { ... }`
fn shared_not_allow_for_the_same_subscriber_to_unsubscribe_to_the_same_channel_twice<B: EventBusUnderTest>(
  fixture: &mut B,
) {
  let event = fixture.create_event(0);
  let classifier = fixture.classifier_for(&event);
  let pipe_to = fixture.test_actor();
  let subscriber = fixture.create_subscriber(pipe_to);
  assert!(fixture.subscribe(&subscriber, &classifier));
  assert!(fixture.unsubscribe(&subscriber, &classifier));
  assert!(!fixture.unsubscribe(&subscriber, &classifier));
}

// Original: `"allow to add multiple subscribers" in { ... }`
fn shared_allow_to_add_multiple_subscribers<B: EventBusUnderTest>(fixture: &mut B) {
  let range = 0..10;
  let mut subscribers = Vec::new();
  let mut classifiers = Vec::new();
  for index in range {
    let event = fixture.create_event(index);
    let pipe_to = fixture.test_actor();
    subscribers.push(fixture.create_subscriber(pipe_to));
    classifiers.push(fixture.classifier_for(&event));
  }
  for (subscriber, classifier) in subscribers.iter().zip(classifiers.iter()) {
    assert!(fixture.subscribe(subscriber, classifier));
  }
  for (subscriber, classifier) in subscribers.iter().zip(classifiers.iter()) {
    assert!(fixture.unsubscribe(subscriber, classifier));
  }
  for subscriber in &subscribers {
    fixture.dispose_subscriber(subscriber);
  }
}

// Original: `"publishing events without any subscribers shouldn't be a problem" in { bus.publish(event) }`
fn shared_publishing_events_without_any_subscribers_should_not_be_a_problem<B: EventBusUnderTest>(fixture: &mut B) {
  let event = fixture.create_event(0);
  fixture.publish(&event);
}

// Original: `"publish the given event to the only subscriber" in { ... }`
fn shared_publish_the_given_event_to_the_only_subscriber<B: EventBusUnderTest>(fixture: &mut B) {
  let event = fixture.create_event(0);
  let classifier = fixture.classifier_for(&event);
  let pipe_to = fixture.test_actor();
  let subscriber = fixture.create_subscriber(pipe_to);
  fixture.subscribe(&subscriber, &classifier);
  fixture.publish(&event);
  fixture.expect_event(&event);
  fixture.expect_no_message();
  fixture.unsubscribe(&subscriber, &classifier);
}

// Original: `"publish to the only subscriber multiple times" in { ... }`
fn shared_publish_to_the_only_subscriber_multiple_times<B: EventBusUnderTest>(fixture: &mut B) {
  let event = fixture.create_event(0);
  let classifier = fixture.classifier_for(&event);
  let pipe_to = fixture.test_actor();
  let subscriber = fixture.create_subscriber(pipe_to);
  fixture.subscribe(&subscriber, &classifier);
  fixture.publish(&event);
  fixture.publish(&event);
  fixture.publish(&event);
  fixture.expect_event(&event);
  fixture.expect_event(&event);
  fixture.expect_event(&event);
  fixture.expect_no_message();
  fixture.unsubscribe(&subscriber, &classifier);
}

// Original: `"publish the given event to all intended subscribers" in { ... }`
fn shared_publish_the_given_event_to_all_intended_subscribers<B: EventBusUnderTest>(fixture: &mut B) {
  let range = 0..10;
  let event = fixture.create_event(0);
  let classifier = fixture.classifier_for(&event);
  let mut subscribers = Vec::new();
  for _ in range {
    let pipe_to = fixture.test_actor();
    subscribers.push(fixture.create_subscriber(pipe_to));
  }
  for subscriber in &subscribers {
    assert!(fixture.subscribe(subscriber, &classifier));
  }
  fixture.publish(&event);
  for _ in 0..10 {
    fixture.expect_event(&event);
  }
  for subscriber in &subscribers {
    assert!(fixture.unsubscribe(subscriber, &classifier));
    fixture.dispose_subscriber(subscriber);
  }
}

// Original: `"not publish the given event to any other subscribers than the intended ones" in { ... }`
fn shared_not_publish_the_given_event_to_any_other_subscribers_than_the_intended_ones<B: EventBusUnderTest>(
  fixture: &mut B,
) {
  let event = fixture.create_event(0);
  let classifier = fixture.classifier_for(&event);
  // Original: `val otherClassifier = getClassifierFor(events.drop(1).head)`
  let other_event = fixture.create_event(1);
  let other_classifier = fixture.classifier_for(&other_event);
  let pipe_to = fixture.test_actor();
  let subscriber = fixture.create_subscriber(pipe_to);
  let other_pipe_to = fixture.test_actor();
  let other_subscriber = fixture.create_subscriber(other_pipe_to);
  fixture.subscribe(&subscriber, &classifier);
  fixture.subscribe(&other_subscriber, &other_classifier);
  fixture.publish(&event);
  fixture.expect_event(&event);
  fixture.unsubscribe(&subscriber, &classifier);
  fixture.unsubscribe(&other_subscriber, &other_classifier);
  fixture.expect_no_message();
}

// Original: `"not publish the given event to a former subscriber" in { ... }`
fn shared_not_publish_the_given_event_to_a_former_subscriber<B: EventBusUnderTest>(fixture: &mut B) {
  let event = fixture.create_event(0);
  let classifier = fixture.classifier_for(&event);
  let pipe_to = fixture.test_actor();
  let subscriber = fixture.create_subscriber(pipe_to);
  fixture.subscribe(&subscriber, &classifier);
  fixture.unsubscribe(&subscriber, &classifier);
  fixture.publish(&event);
  fixture.expect_no_message();
}

// Original: `"cleanup subscriber" in { disposeSubscriber(system, subscriber) }`
fn shared_cleanup_subscriber<B: EventBusUnderTest>(fixture: &mut B) {
  let pipe_to = fixture.test_actor();
  let subscriber = fixture.create_subscriber(pipe_to);
  fixture.dispose_subscriber(&subscriber);
}

// ---------------------------------------------------------------------------
// LookupEventBusSpec
// ---------------------------------------------------------------------------

// "LookupEventBus" must { "allow subscribers" in { ... } }
#[test]
fn lookup_event_bus_must_allow_subscribers() {
  let mut fixture = LookupBusFixture::new();
  shared_allow_subscribers(&mut fixture);
}

// "LookupEventBus" must { "allow to unsubscribe already existing subscriber" in { ... } }
#[test]
fn lookup_event_bus_must_allow_to_unsubscribe_already_existing_subscriber() {
  let mut fixture = LookupBusFixture::new();
  shared_allow_to_unsubscribe_already_existing_subscriber(&mut fixture);
}

// "LookupEventBus" must { "not allow to unsubscribe non-existing subscriber" in { ... } }
#[test]
fn lookup_event_bus_must_not_allow_to_unsubscribe_non_existing_subscriber() {
  let mut fixture = LookupBusFixture::new();
  shared_not_allow_to_unsubscribe_non_existing_subscriber(&mut fixture);
}

// "LookupEventBus" must { "not allow for the same subscriber to subscribe to the same channel twice" in { ... } }
#[test]
fn lookup_event_bus_must_not_allow_for_the_same_subscriber_to_subscribe_to_the_same_channel_twice() {
  let mut fixture = LookupBusFixture::new();
  shared_not_allow_for_the_same_subscriber_to_subscribe_to_the_same_channel_twice(&mut fixture);
}

// "LookupEventBus" must { "not allow for the same subscriber to unsubscribe to the same channel twice" in { ... } }
#[test]
fn lookup_event_bus_must_not_allow_for_the_same_subscriber_to_unsubscribe_to_the_same_channel_twice() {
  let mut fixture = LookupBusFixture::new();
  shared_not_allow_for_the_same_subscriber_to_unsubscribe_to_the_same_channel_twice(&mut fixture);
}

// "LookupEventBus" must { "allow to add multiple subscribers" in { ... } }
#[test]
fn lookup_event_bus_must_allow_to_add_multiple_subscribers() {
  let mut fixture = LookupBusFixture::new();
  shared_allow_to_add_multiple_subscribers(&mut fixture);
}

// "LookupEventBus" must { "publishing events without any subscribers shouldn't be a problem" in { ... } }
#[test]
fn lookup_event_bus_must_not_have_a_problem_publishing_events_without_any_subscribers() {
  let mut fixture = LookupBusFixture::new();
  shared_publishing_events_without_any_subscribers_should_not_be_a_problem(&mut fixture);
}

// "LookupEventBus" must { "publish the given event to the only subscriber" in { ... } }
#[test]
fn lookup_event_bus_must_publish_the_given_event_to_the_only_subscriber() {
  let mut fixture = LookupBusFixture::new();
  shared_publish_the_given_event_to_the_only_subscriber(&mut fixture);
}

// "LookupEventBus" must { "publish to the only subscriber multiple times" in { ... } }
#[test]
fn lookup_event_bus_must_publish_to_the_only_subscriber_multiple_times() {
  let mut fixture = LookupBusFixture::new();
  shared_publish_to_the_only_subscriber_multiple_times(&mut fixture);
}

// "LookupEventBus" must { "publish the given event to all intended subscribers" in { ... } }
#[test]
fn lookup_event_bus_must_publish_the_given_event_to_all_intended_subscribers() {
  let mut fixture = LookupBusFixture::new();
  shared_publish_the_given_event_to_all_intended_subscribers(&mut fixture);
}

// "LookupEventBus" must { "not publish the given event to any other subscribers than the intended ones" in { ... } }
#[test]
fn lookup_event_bus_must_not_publish_the_given_event_to_any_other_subscribers_than_the_intended_ones() {
  let mut fixture = LookupBusFixture::new();
  shared_not_publish_the_given_event_to_any_other_subscribers_than_the_intended_ones(&mut fixture);
}

// "LookupEventBus" must { "not publish the given event to a former subscriber" in { ... } }
#[test]
fn lookup_event_bus_must_not_publish_the_given_event_to_a_former_subscriber() {
  let mut fixture = LookupBusFixture::new();
  shared_not_publish_the_given_event_to_a_former_subscriber(&mut fixture);
}

// "LookupEventBus" must { "cleanup subscriber" in { ... } }
#[test]
fn lookup_event_bus_must_cleanup_subscriber() {
  let mut fixture = LookupBusFixture::new();
  shared_cleanup_subscriber(&mut fixture);
}

// ---------------------------------------------------------------------------
// ScanningEventBusSpec
// ---------------------------------------------------------------------------

// "ScanningEventBus" must { "allow subscribers" in { ... } }
#[test]
fn scanning_event_bus_must_allow_subscribers() {
  let mut fixture = ScanningBusFixture::new();
  shared_allow_subscribers(&mut fixture);
}

// "ScanningEventBus" must { "allow to unsubscribe already existing subscriber" in { ... } }
#[test]
fn scanning_event_bus_must_allow_to_unsubscribe_already_existing_subscriber() {
  let mut fixture = ScanningBusFixture::new();
  shared_allow_to_unsubscribe_already_existing_subscriber(&mut fixture);
}

// "ScanningEventBus" must { "not allow to unsubscribe non-existing subscriber" in { ... } }
#[test]
fn scanning_event_bus_must_not_allow_to_unsubscribe_non_existing_subscriber() {
  let mut fixture = ScanningBusFixture::new();
  shared_not_allow_to_unsubscribe_non_existing_subscriber(&mut fixture);
}

// "ScanningEventBus" must { "not allow for the same subscriber to subscribe to the same channel twice" in { ... } }
#[test]
fn scanning_event_bus_must_not_allow_for_the_same_subscriber_to_subscribe_to_the_same_channel_twice() {
  let mut fixture = ScanningBusFixture::new();
  shared_not_allow_for_the_same_subscriber_to_subscribe_to_the_same_channel_twice(&mut fixture);
}

// "ScanningEventBus" must { "not allow for the same subscriber to unsubscribe to the same channel twice" in { ... } }
#[test]
fn scanning_event_bus_must_not_allow_for_the_same_subscriber_to_unsubscribe_to_the_same_channel_twice() {
  let mut fixture = ScanningBusFixture::new();
  shared_not_allow_for_the_same_subscriber_to_unsubscribe_to_the_same_channel_twice(&mut fixture);
}

// "ScanningEventBus" must { "allow to add multiple subscribers" in { ... } }
#[test]
fn scanning_event_bus_must_allow_to_add_multiple_subscribers() {
  let mut fixture = ScanningBusFixture::new();
  shared_allow_to_add_multiple_subscribers(&mut fixture);
}

// "ScanningEventBus" must { "publishing events without any subscribers shouldn't be a problem" in { ... } }
#[test]
fn scanning_event_bus_must_not_have_a_problem_publishing_events_without_any_subscribers() {
  let mut fixture = ScanningBusFixture::new();
  shared_publishing_events_without_any_subscribers_should_not_be_a_problem(&mut fixture);
}

// "ScanningEventBus" must { "publish the given event to the only subscriber" in { ... } }
#[test]
fn scanning_event_bus_must_publish_the_given_event_to_the_only_subscriber() {
  let mut fixture = ScanningBusFixture::new();
  shared_publish_the_given_event_to_the_only_subscriber(&mut fixture);
}

// "ScanningEventBus" must { "publish to the only subscriber multiple times" in { ... } }
#[test]
fn scanning_event_bus_must_publish_to_the_only_subscriber_multiple_times() {
  let mut fixture = ScanningBusFixture::new();
  shared_publish_to_the_only_subscriber_multiple_times(&mut fixture);
}

// "ScanningEventBus" must { "publish the given event to all intended subscribers" in { ... } }
#[test]
fn scanning_event_bus_must_publish_the_given_event_to_all_intended_subscribers() {
  let mut fixture = ScanningBusFixture::new();
  shared_publish_the_given_event_to_all_intended_subscribers(&mut fixture);
}

// "ScanningEventBus" must { "not publish the given event to any other subscribers than the intended ones" in { ... } }
#[test]
fn scanning_event_bus_must_not_publish_the_given_event_to_any_other_subscribers_than_the_intended_ones() {
  let mut fixture = ScanningBusFixture::new();
  shared_not_publish_the_given_event_to_any_other_subscribers_than_the_intended_ones(&mut fixture);
}

// "ScanningEventBus" must { "not publish the given event to a former subscriber" in { ... } }
#[test]
fn scanning_event_bus_must_not_publish_the_given_event_to_a_former_subscriber() {
  let mut fixture = ScanningBusFixture::new();
  shared_not_publish_the_given_event_to_a_former_subscriber(&mut fixture);
}

// "ScanningEventBus" must { "cleanup subscriber" in { ... } }
#[test]
fn scanning_event_bus_must_cleanup_subscriber() {
  let mut fixture = ScanningBusFixture::new();
  shared_cleanup_subscriber(&mut fixture);
}

// ---------------------------------------------------------------------------
// ActorEventBusSpec: the 13 shared cases
// ---------------------------------------------------------------------------

// "ActorEventBus" must { "allow subscribers" in { ... } }
#[test]
fn actor_event_bus_must_allow_subscribers() {
  let mut fixture = ActorBusFixture::new();
  shared_allow_subscribers(&mut fixture);
}

// "ActorEventBus" must { "allow to unsubscribe already existing subscriber" in { ... } }
#[test]
fn actor_event_bus_must_allow_to_unsubscribe_already_existing_subscriber() {
  let mut fixture = ActorBusFixture::new();
  shared_allow_to_unsubscribe_already_existing_subscriber(&mut fixture);
}

// "ActorEventBus" must { "not allow to unsubscribe non-existing subscriber" in { ... } }
#[test]
fn actor_event_bus_must_not_allow_to_unsubscribe_non_existing_subscriber() {
  let mut fixture = ActorBusFixture::new();
  shared_not_allow_to_unsubscribe_non_existing_subscriber(&mut fixture);
}

// "ActorEventBus" must { "not allow for the same subscriber to subscribe to the same channel twice" in { ... } }
#[test]
fn actor_event_bus_must_not_allow_for_the_same_subscriber_to_subscribe_to_the_same_channel_twice() {
  let mut fixture = ActorBusFixture::new();
  shared_not_allow_for_the_same_subscriber_to_subscribe_to_the_same_channel_twice(&mut fixture);
}

// "ActorEventBus" must { "not allow for the same subscriber to unsubscribe to the same channel twice" in { ... } }
#[test]
fn actor_event_bus_must_not_allow_for_the_same_subscriber_to_unsubscribe_to_the_same_channel_twice() {
  let mut fixture = ActorBusFixture::new();
  shared_not_allow_for_the_same_subscriber_to_unsubscribe_to_the_same_channel_twice(&mut fixture);
}

// "ActorEventBus" must { "allow to add multiple subscribers" in { ... } }
#[test]
fn actor_event_bus_must_allow_to_add_multiple_subscribers() {
  let mut fixture = ActorBusFixture::new();
  shared_allow_to_add_multiple_subscribers(&mut fixture);
}

// "ActorEventBus" must { "publishing events without any subscribers shouldn't be a problem" in { ... } }
#[test]
fn actor_event_bus_must_not_have_a_problem_publishing_events_without_any_subscribers() {
  let mut fixture = ActorBusFixture::new();
  shared_publishing_events_without_any_subscribers_should_not_be_a_problem(&mut fixture);
}

// "ActorEventBus" must { "publish the given event to the only subscriber" in { ... } }
#[test]
fn actor_event_bus_must_publish_the_given_event_to_the_only_subscriber() {
  let mut fixture = ActorBusFixture::new();
  shared_publish_the_given_event_to_the_only_subscriber(&mut fixture);
}

// "ActorEventBus" must { "publish to the only subscriber multiple times" in { ... } }
#[test]
fn actor_event_bus_must_publish_to_the_only_subscriber_multiple_times() {
  let mut fixture = ActorBusFixture::new();
  shared_publish_to_the_only_subscriber_multiple_times(&mut fixture);
}

// "ActorEventBus" must { "publish the given event to all intended subscribers" in { ... } }
#[test]
fn actor_event_bus_must_publish_the_given_event_to_all_intended_subscribers() {
  let mut fixture = ActorBusFixture::new();
  shared_publish_the_given_event_to_all_intended_subscribers(&mut fixture);
}

// "ActorEventBus" must { "not publish the given event to any other subscribers than the intended ones" in { ... } }
#[test]
fn actor_event_bus_must_not_publish_the_given_event_to_any_other_subscribers_than_the_intended_ones() {
  let mut fixture = ActorBusFixture::new();
  shared_not_publish_the_given_event_to_any_other_subscribers_than_the_intended_ones(&mut fixture);
}

// "ActorEventBus" must { "not publish the given event to a former subscriber" in { ... } }
#[test]
fn actor_event_bus_must_not_publish_the_given_event_to_a_former_subscriber() {
  let mut fixture = ActorBusFixture::new();
  shared_not_publish_the_given_event_to_a_former_subscriber(&mut fixture);
}

// "ActorEventBus" must { "cleanup subscriber" in { ... } }
#[test]
fn actor_event_bus_must_cleanup_subscriber() {
  let mut fixture = ActorBusFixture::new();
  shared_cleanup_subscriber(&mut fixture);
}

// ---------------------------------------------------------------------------
// ActorEventBusSpec: the three cases that only this bus runs
// ---------------------------------------------------------------------------

// Original:
//   "must unsubscribe subscriber when it terminates" in { ... }
//
// Original text of the expected debug event:
//   s"actor $a has terminated, unsubscribing it from $bus"
// The port spells the actor with `ActorPath`'s `Display` and the bus with the
// name it was constructed with.
fn expect_unsubscribed_by_unsubscriber(probe: &mut TestProbe, subscriber: &ActorRef, bus_name: &str) {
  let expected = format!("actor {} has terminated, unsubscribing it from {}", subscriber.path(), bus_name);
  let found = probe.fish_for_message(default_timeout(), |message| {
    message.downcast_ref::<LoggingDebug>().is_some_and(|event| event.message() == expected.as_str())
  });
  assert!(found.downcast_ref::<LoggingDebug>().is_some(), "expected a Debug event carrying {expected:?}");
}

// Original:
//   "must unregister subscriber only after it unsubscribes from all of it's subscriptions" in { ... }
//
// Original text of the expected debug event:
//   s"unregistered watch of $a in $bus"
fn expect_unregistered_from_unsubscriber(probe: &mut TestProbe, subscriber: &ActorRef, bus_name: &str) {
  let expected = format!("unregistered watch of {} in {}", subscriber.path(), bus_name);
  let found = probe.fish_for_message(default_timeout(), |message| {
    message.downcast_ref::<LoggingDebug>().is_some_and(|event| event.message() == expected.as_str())
  });
  assert!(found.downcast_ref::<LoggingDebug>().is_some(), "expected a Debug event carrying {expected:?}");
}

// "ActorEventBus" must { "unsubscribe subscriber when it terminates" in { ... } }
#[test]
fn actor_event_bus_must_unsubscribe_subscriber_when_it_terminates() {
  let mut fixture = ActorBusFixture::new();
  let dead_letters = fixture.spec.system().dead_letters();
  let a1 = fixture.create_subscriber(dead_letters);
  let test_actor = fixture.test_actor();
  let subs = fixture.create_subscriber(test_actor);

  let mut probe = TestProbe::new(fixture.spec.system_mut());
  let _ = fixture.spec.system().event_stream().subscribe(Some(probe.ref_handle()), EventClass::of::<LoggingDebug>());

  fixture.subscribe(&subs, &a1);

  let m1 = Notification { reference: a1.clone(), payload: 1 };
  fixture.publish(&m1);
  fixture.expect_event(&m1);

  // Original: `watch(subs); subs ! PoisonPill; expectTerminated(subs)`
  fixture.spec.test_kit_mut().watch(&subs);
  let sender = ImplicitSender::self_actor_ref(&fixture.spec);
  subs.tell("PoisonPill", sender);
  fixture.spec.test_kit_mut().expect_msg_terminated(default_timeout());

  // Original: `expectUnsubscribedByUnsubscriber(p, subs)`
  expect_unsubscribed_by_unsubscriber(&mut probe, &subs, fixture.bus.name());

  let m2 = Notification { reference: a1.clone(), payload: 2 };
  fixture.publish(&m2);
  fixture.expect_no_message();

  fixture.dispose_subscriber(&subs);
  fixture.dispose_subscriber(&a1);
}

// "ActorEventBus" must { "keep subscriber even if its subscription-actors have died" in { ... } }
#[test]
fn actor_event_bus_must_keep_subscriber_even_if_its_subscription_actors_have_died() {
  // Deaths of monitored actors should not influence the subscription.
  // For example: one might still want to monitor messages classified to A
  // even though it died, and handle these in some way.
  let mut fixture = ActorBusFixture::new();
  let dead_letters = fixture.spec.system().dead_letters();
  let a1 = fixture.create_subscriber(dead_letters);
  let test_actor = fixture.test_actor();
  let subs = fixture.create_subscriber(test_actor);

  assert!(fixture.subscribe(&subs, &a1));

  let m1 = Notification { reference: a1.clone(), payload: 1 };
  fixture.publish(&m1);
  fixture.expect_event(&m1);

  // Original: `watch(a1); a1 ! PoisonPill; expectTerminated(a1)`
  fixture.spec.test_kit_mut().watch(&a1);
  let sender = ImplicitSender::self_actor_ref(&fixture.spec);
  a1.tell("PoisonPill", sender);
  fixture.spec.test_kit_mut().expect_msg_terminated(default_timeout());

  // Even though a1 has terminated, classification still applies.
  let m2 = Notification { reference: a1.clone(), payload: 2 };
  fixture.publish(&m2);
  fixture.expect_event(&m2);

  fixture.dispose_subscriber(&subs);
  fixture.dispose_subscriber(&a1);
}

// "ActorEventBus" must { "unregister subscriber only after it unsubscribes from all of it's subscriptions" in { ... } }
#[test]
fn actor_event_bus_must_unregister_subscriber_only_after_it_unsubscribes_from_all_of_its_subscriptions() {
  let mut fixture = ActorBusFixture::new();
  let dead_letters = fixture.spec.system().dead_letters();
  let a1 = fixture.create_subscriber(dead_letters.clone());
  let a2 = fixture.create_subscriber(dead_letters);
  let test_actor = fixture.test_actor();
  let subs = fixture.create_subscriber(test_actor);

  let mut probe = TestProbe::new(fixture.spec.system_mut());
  let _ = fixture.spec.system().event_stream().subscribe(Some(probe.ref_handle()), EventClass::of::<LoggingDebug>());

  assert!(fixture.subscribe(&subs, &a1));
  assert!(fixture.subscribe(&subs, &a2));

  let m1_1 = Notification { reference: a1.clone(), payload: 1 };
  let m2_1 = Notification { reference: a2.clone(), payload: 1 };
  fixture.publish(&m1_1);
  fixture.publish(&m2_1);
  fixture.expect_event(&m1_1);
  fixture.expect_event(&m2_1);

  fixture.unsubscribe(&subs, &a1);
  let m1_2 = Notification { reference: a1.clone(), payload: 2 };
  fixture.publish(&m1_2);
  fixture.expect_no_message();
  let m2_2 = Notification { reference: a2.clone(), payload: 2 };
  fixture.publish(&m2_2);
  fixture.expect_event(&m2_2);

  fixture.unsubscribe(&subs, &a2);
  expect_unregistered_from_unsubscriber(&mut probe, &subs, fixture.bus.name());
  let m1_3 = Notification { reference: a1.clone(), payload: 3 };
  let m2_3 = Notification { reference: a2.clone(), payload: 3 };
  fixture.publish(&m1_3);
  fixture.publish(&m2_3);
  fixture.expect_no_message();

  fixture.dispose_subscriber(&subs);
  fixture.dispose_subscriber(&a1);
  fixture.dispose_subscriber(&a2);
}
