// Ported from Pekko (Apache Pekko, actor-tests):
//   actor-tests/src/test/scala/org/apache/pekko/actor/DeadLetterSupressionSpec.scala
//   (Pekko v2.0.0, commit 9fbd2283b1cf701c90a1223bf621942c191db61f)
// Original test names are kept as comments next to the ported cases.
//
// The original marks `SuppressedMsg` with the `DeadLetterSuppression` trait.
// Rust cannot test trait membership at run time, so the port registers the
// type with the system (`register_dead_letter_suppression`), which is the
// same declaration style the event stream uses for subclass links.
//
// The original compares whole `DeadLetter` values with `expectMsg`. The port
// takes the event with `expect_msg_type` and compares the message, the
// sender, and the recipient field by field.

use core::time::Duration;

use fraktor_actor::kernel::actor::DeadLetter;
use fraktor_actor::kernel::actor::DeadLetterSuppression;
use fraktor_actor::kernel::actor::SuppressedDeadLetter;
use fraktor_actor::kernel::actor::Terminated;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::event::EventClass;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_actors::echo_actor_props;
use fraktor_testkit::test_kit::TestProbe;

// Original: `case object NormalMsg`
#[derive(Clone, Debug, PartialEq, Eq)]
struct NormalMsg;

// Original: `case object SuppressedMsg extends DeadLetterSuppression`
#[derive(Clone, Debug, PartialEq, Eq)]
struct SuppressedMsg;

impl DeadLetterSuppression for SuppressedMsg {}

fn no_message_window() -> Duration {
  Duration::from_millis(100)
}

fn subscribe(spec: &FraktorSpec, probe: &TestProbe, class: EventClass) {
  spec.system().event_stream().subscribe(Some(probe.ref_handle()), class).expect("the probe is not null");
}

fn expect_dead_letter(probe: &mut TestProbe) -> DeadLetter {
  probe.expect_msg_type::<DeadLetter>()
}

fn expect_suppressed(probe: &mut TestProbe) -> SuppressedDeadLetter {
  probe.expect_msg_type::<SuppressedDeadLetter>()
}

fn assert_normal_dead_letter(event: &DeadLetter, sender: &ActorRef, recipient: &ActorRef) {
  assert_eq!(event.message().downcast_ref::<NormalMsg>(), Some(&NormalMsg), "the message is NormalMsg");
  assert_eq!(event.sender(), sender, "the sender is the test actor");
  assert_eq!(event.recipient(), recipient, "the recipient matches");
}

fn assert_suppressed(event: &SuppressedDeadLetter, sender: &ActorRef, recipient: &ActorRef) {
  assert_eq!(event.message().downcast_ref::<SuppressedMsg>(), Some(&SuppressedMsg), "the message is SuppressedMsg");
  assert_eq!(event.sender(), sender, "the sender is the test actor");
  assert_eq!(event.recipient(), recipient, "the recipient is the dead letters handle");
}

// Original: "must suppress message from default dead-letters logging (sent to dead: <deadActor>)"
#[test]
fn suppress_message_sent_to_a_dead_actor() {
  let mut spec = FraktorSpec::new("DeadLetterSupressionSpec");
  spec.system_mut().register_dead_letter_suppression::<SuppressedMsg>();
  let test_actor = spec.test_actor();
  let dead_letters = spec.system().dead_letters();

  let dead_actor = spec.system_mut().actor_of(echo_actor_props());
  spec.test_kit_mut().watch(&dead_actor);
  dead_actor.tell("PoisonPill", test_actor.clone());
  let terminated: Terminated = spec.expect_msg_type();
  assert_eq!(terminated.target(), &dead_actor);

  let mut dead_listener = TestProbe::new(spec.system_mut());
  subscribe(&spec, &dead_listener, EventClass::of::<DeadLetter>());
  let mut suppressed_listener = TestProbe::new(spec.system_mut());
  subscribe(&spec, &suppressed_listener, EventClass::of::<SuppressedDeadLetter>());
  let mut all_listener = TestProbe::new(spec.system_mut());
  subscribe(&spec, &all_listener, EventClass::of::<fraktor_actor::kernel::actor::AllDeadLetters>());

  dead_actor.tell(SuppressedMsg, test_actor.clone());
  dead_actor.tell(NormalMsg, test_actor.clone());

  let event = expect_dead_letter(&mut dead_listener);
  assert_normal_dead_letter(&event, &test_actor, &dead_actor);
  dead_listener.expect_no_message(no_message_window());

  let event = expect_suppressed(&mut suppressed_listener);
  assert_suppressed(&event, &test_actor, &dead_letters);
  suppressed_listener.expect_no_message(no_message_window());

  let event = expect_suppressed(&mut all_listener);
  assert_suppressed(&event, &test_actor, &dead_letters);
  let event = expect_dead_letter(&mut all_listener);
  assert_normal_dead_letter(&event, &test_actor, &dead_actor);
  all_listener.expect_no_message(no_message_window());

  // unwrap for ActorSelection: the recipient ref is not the dead actor here,
  // so the original checks only the message.
  let selection = spec.system().actor_selection(&dead_actor.path());
  selection.tell(SuppressedMsg, test_actor.clone());
  selection.tell(NormalMsg, test_actor.clone());

  let event = expect_dead_letter(&mut dead_listener);
  assert_eq!(event.message().downcast_ref::<NormalMsg>(), Some(&NormalMsg), "the message is NormalMsg");
  let event = expect_suppressed(&mut suppressed_listener);
  assert_eq!(event.message().downcast_ref::<SuppressedMsg>(), Some(&SuppressedMsg), "the message is SuppressedMsg");
  dead_listener.expect_no_message(no_message_window());

  spec.terminate();
}

// Original: "must suppress message from default dead-letters logging (sent to dead: deadLetters)"
#[test]
fn suppress_message_sent_to_dead_letters() {
  let mut spec = FraktorSpec::new("DeadLetterSupressionSpec");
  spec.system_mut().register_dead_letter_suppression::<SuppressedMsg>();
  let test_actor = spec.test_actor();
  let dead_letters = spec.system().dead_letters();

  let mut dead_listener = TestProbe::new(spec.system_mut());
  subscribe(&spec, &dead_listener, EventClass::of::<DeadLetter>());
  let mut suppressed_listener = TestProbe::new(spec.system_mut());
  subscribe(&spec, &suppressed_listener, EventClass::of::<SuppressedDeadLetter>());
  let mut all_listener = TestProbe::new(spec.system_mut());
  subscribe(&spec, &all_listener, EventClass::of::<fraktor_actor::kernel::actor::AllDeadLetters>());

  dead_letters.tell(SuppressedMsg, test_actor.clone());
  dead_letters.tell(NormalMsg, test_actor.clone());

  let event = expect_dead_letter(&mut dead_listener);
  assert_normal_dead_letter(&event, &test_actor, &dead_letters);
  let event = expect_suppressed(&mut suppressed_listener);
  assert_suppressed(&event, &test_actor, &dead_letters);
  let event = expect_suppressed(&mut all_listener);
  assert_suppressed(&event, &test_actor, &dead_letters);
  let event = expect_dead_letter(&mut all_listener);
  assert_normal_dead_letter(&event, &test_actor, &dead_letters);
  dead_listener.expect_no_message(no_message_window());
  suppressed_listener.expect_no_message(no_message_window());
  all_listener.expect_no_message(no_message_window());

  // unwrap for ActorSelection
  let selection = spec.system().actor_selection(&dead_letters.path());
  selection.tell(SuppressedMsg, test_actor.clone());
  selection.tell(NormalMsg, test_actor.clone());

  let event = expect_dead_letter(&mut dead_listener);
  assert_normal_dead_letter(&event, &test_actor, &dead_letters);
  let event = expect_suppressed(&mut suppressed_listener);
  assert_suppressed(&event, &test_actor, &dead_letters);
  dead_listener.expect_no_message(no_message_window());

  spec.terminate();
}
