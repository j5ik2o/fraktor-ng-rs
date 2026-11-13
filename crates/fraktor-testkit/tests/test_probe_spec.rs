// Ported from Pekko v1.7.1:
//   testkit/src/test/scala/org/apache/pekko/testkit/TestProbeSpec.scala
//
// Original test names are kept as comments next to each ported case.

use core::sync::atomic::{AtomicUsize, Ordering};
use core::time::Duration;

use std::sync::Arc;

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::supervision::SupervisorStrategy;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_actors::echo_actor_props;
use fraktor_testkit::test_kit::{expected_messages::ClassOf, AutoPilot, AutoPilotControl, TestProbe};

const CUSTOM_SPEC_SYSTEM_NAME: &str = "PekkoCustomSpec";

fn zero() -> Duration {
  Duration::ZERO
}

fn five_seconds() -> Duration {
  Duration::from_secs(5)
}

fn one_second() -> Duration {
  Duration::from_secs(1)
}

struct TestProbeSpecCase {
  fraktor_spec: FraktorSpec,
}

impl TestProbeSpecCase {
  fn new() -> Self {
    Self { fraktor_spec: FraktorSpec::new(CUSTOM_SPEC_SYSTEM_NAME) }
  }

  fn test_probe(&mut self) -> TestProbe {
    TestProbe::new(self.fraktor_spec.system_mut())
  }

  fn test_probe_named(&mut self, name: &str) -> TestProbe {
    TestProbe::with_name(self.fraktor_spec.system_mut(), name)
  }

  fn system(&mut self) -> &mut ActorSystem {
    self.fraktor_spec.system_mut()
  }

  fn test_actor(&self) -> ActorRef {
    self.fraktor_spec.test_actor()
  }

  fn expect_msg(&mut self, expected: &str) {
    self.fraktor_spec.expect_msg(expected);
  }

  fn ignore_msg(&mut self, filter: impl Fn(&Message) -> bool + Send + Sync + 'static) {
    self.fraktor_spec.test_kit_mut().ignore_msg(filter);
  }
}

impl Drop for TestProbeSpecCase {
  fn drop(&mut self) {
    self.fraktor_spec.terminate();
  }
}

// The original helper `assertFailureMessageContains`: the block must fail with
// an assertion whose message contains the hint. Pekko's AssertionError becomes
// a Rust panic, so the message is read from the panic payload.
fn assert_failure_message_contains(expected_hint: &str, block: impl FnOnce() + std::panic::UnwindSafe) {
  let outcome = std::panic::catch_unwind(block);
  let payload = outcome.expect_err("expected the block to fail, but it returned");
  let message = payload
    .downcast_ref::<String>()
    .cloned()
    .or_else(|| payload.downcast_ref::<&str>().map(|text| (*text).to_string()))
    .unwrap_or_else(|| "the failure carried no message".to_string());
  assert!(
    message.contains(expected_hint),
    "failure message did not contain hint! Was: {message}, expected to contain {expected_hint}"
  );
}

struct EmptyActor;

impl Actor for EmptyActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

// The inline `TestActor.AutoPilot` of the original: forward every message to
// the target, and stop piloting once `stop` arrives.
struct ForwardUntil {
  target: ActorRef,
  stop: &'static str,
}

impl AutoPilot for ForwardUntil {
  fn run(&mut self, sender: &ActorRef, message: Message) -> AutoPilotControl {
    if message == self.stop {
      return AutoPilotControl::NoAutoPilot;
    }
    self.target.tell(message, sender.clone());
    AutoPilotControl::KeepRunning
  }
}

// "A TestProbe" must { "reply to futures" in { ... } }
#[test]
fn reply_to_futures() {
  let mut spec = TestProbeSpecCase::new();
  let mut probe = spec.test_probe();
  let future = probe.ref_handle().ask("hello");
  probe.expect_msg_within(zero(), "hello");
  probe.last_message().sender().tell("world", probe.ref_handle());
  assert!(future.is_completed());
  assert_eq!(future.await_result(five_seconds()), "world");
}

// "A TestProbe" must { "reply to messages" in { ... } }
#[test]
fn reply_to_messages() {
  let mut spec = TestProbeSpecCase::new();
  let mut tk1 = spec.test_probe();
  let mut tk2 = spec.test_probe();
  tk1.ref_handle().tell("hello", tk2.ref_handle());
  tk1.expect_msg_within(zero(), "hello");
  tk1.last_message().sender().tell("world", tk1.ref_handle());
  tk2.expect_msg_within(zero(), "world");
}

// "A TestProbe" must { "properly send and reply to messages" in { ... } }
#[test]
fn properly_send_and_reply_to_messages() {
  let mut spec = TestProbeSpecCase::new();
  let mut probe1 = spec.test_probe();
  let mut probe2 = spec.test_probe();
  probe1.send(&probe2.ref_handle(), "hello");
  probe2.expect_msg_within(zero(), "hello");
  probe2.last_message().sender().tell("world", probe2.ref_handle());
  probe1.expect_msg_hint(zero(), "some hint here", "world");
}

// "A TestProbe" must { "create a child when invoking actorOf" in { ... } }
#[test]
fn create_a_child_when_invoking_actor_of() {
  let mut spec = TestProbeSpecCase::new();
  let mut probe = spec.test_probe();
  let child = probe.child_actor_of(echo_actor_props());
  assert_eq!(child.path().parent(), probe.ref_handle().path());

  let named_child = probe.child_actor_of_named(echo_actor_props(), "actorName");
  assert_eq!(named_child.path().name(), "actorName");
}

// "A TestProbe" must { "restart a failing child if the given supervisor says so" in { ... } }
#[test]
fn restart_a_failing_child_if_the_given_supervisor_says_so() {
  struct FailingActor {
    restarts: Arc<AtomicUsize>,
  }

  impl Actor for FailingActor {
    fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {
      panic!("simulated failure");
    }

    fn post_restart(&mut self) {
      self.restarts.fetch_add(1, Ordering::SeqCst);
    }
  }

  let mut spec = TestProbeSpecCase::new();
  let restarts = Arc::new(AtomicUsize::new(0));
  let restart_count = Arc::clone(&restarts);
  let mut probe = spec.test_probe();
  let child = probe.child_actor_of_with_supervisor(
    Props::new(move || FailingActor { restarts: Arc::clone(&restart_count) }),
    SupervisorStrategy::default_strategy(),
  );

  spec.fraktor_spec.await_assert(Duration::from_secs(3), Duration::from_millis(100), || {
    child.tell("hello", child.clone());
    assert!(restarts.load(Ordering::SeqCst) > 1, "the failing child was not restarted");
  });
}

// "A TestProbe" must {
//   "throw AssertionError containing hint in its message if max await time is exceeded" in { ... }
// }
#[test]
fn throw_assertion_error_containing_hint_if_max_await_time_is_exceeded() {
  let mut spec = TestProbeSpecCase::new();
  let mut probe = spec.test_probe();
  let hint = "some hint";

  assert_failure_message_contains(
    hint,
    std::panic::AssertUnwindSafe(|| {
      probe.expect_msg_hint(zero(), hint, "hello");
    }),
  );
}

// "A TestProbe" must {
//   "throw AssertionError containing hint in its message if received message doesn't match" in { ... }
// }
#[test]
fn throw_assertion_error_containing_hint_if_received_message_does_not_match() {
  let mut spec = TestProbeSpecCase::new();
  let mut probe = spec.test_probe();
  let hint = "some hint";
  probe.ref_handle().tell("hello", probe.ref_handle());

  assert_failure_message_contains(
    hint,
    std::panic::AssertUnwindSafe(|| {
      probe.expect_msg_hint(zero(), hint, "bye");
    }),
  );
}

// "A TestProbe" must { "have an AutoPilot" in { ... } }
#[test]
fn have_an_auto_pilot() {
  let mut spec = TestProbeSpecCase::new();
  let test_actor = spec.test_actor();
  let mut probe = spec.test_probe();
  probe.set_auto_pilot(ForwardUntil { target: test_actor, stop: "stop" });
  probe.ref_handle().tell("hallo", probe.ref_handle());
  probe.ref_handle().tell("welt", probe.ref_handle());
  probe.ref_handle().tell("stop", probe.ref_handle());
  spec.expect_msg("hallo");
  spec.expect_msg("welt");
  probe.expect_msg("hallo");
  probe.expect_msg("welt");
  probe.expect_msg("stop");
  probe.ref_handle().tell("hallo", probe.ref_handle());
  probe.expect_msg("hallo");
  let test_actor = spec.test_actor();
  test_actor.tell("end", test_actor.clone());
  spec.expect_msg("end");
}

// "A TestProbe" must { "be able to expect primitive types" in { ... } }
#[test]
fn be_able_to_expect_primitive_types() {
  let mut spec = TestProbeSpecCase::new();
  let test_actor = spec.test_actor();
  for _ in 1..=7 {
    test_actor.tell(42_i32, test_actor.clone());
  }
  assert_eq!(spec.fraktor_spec.expect_msg_type::<i32>(), 42);
  assert_eq!(spec.fraktor_spec.expect_msg_any_class_of(&[ClassOf::of::<i32>()]), 42);
  assert_eq!(spec.fraktor_spec.expect_msg_all_class_of(&[ClassOf::of::<i32>()]), vec![42]);
  assert_eq!(spec.fraktor_spec.expect_msg_all_conforming_of(&[ClassOf::of::<i32>()]), vec![42]);
  assert_eq!(spec.fraktor_spec.expect_msg_all_conforming_of_within(five_seconds(), &[ClassOf::of::<i32>()]), vec![42]);
  assert_eq!(spec.fraktor_spec.expect_msg_all_class_of(&[ClassOf::of::<i32>()]), vec![42]);
  assert_eq!(spec.fraktor_spec.expect_msg_all_class_of_within(five_seconds(), &[ClassOf::of::<i32>()]), vec![42]);
}

// "A TestProbe" must { "be able to fish for messages" in { ... } }
#[test]
fn be_able_to_fish_for_messages() {
  let mut spec = TestProbeSpecCase::new();
  let mut probe = spec.test_probe();
  probe.ref_handle().tell("hallo", probe.ref_handle());
  probe.ref_handle().tell("welt", probe.ref_handle());
  probe.ref_handle().tell("fishForMe", probe.ref_handle());
  probe.ref_handle().tell("done", probe.ref_handle());

  probe.fish_for_message(one_second(), |message: &Message| message == *"fishForMe");

  probe.expect_msg_within(one_second(), "done");
}

// "A TestProbe" must { "be able to fish for specific messages" in { ... } }
#[test]
fn be_able_to_fish_for_specific_messages() {
  let mut spec = TestProbeSpecCase::new();
  let mut probe = spec.test_probe();
  probe.ref_handle().tell("hallo", probe.ref_handle());
  probe.ref_handle().tell("welt", probe.ref_handle());
  probe.ref_handle().tell("fishForMe", probe.ref_handle());
  probe.ref_handle().tell("done", probe.ref_handle());

  let message: &'static str =
    probe.fish_for_specific_message(one_second(), |message: &Message| match message.downcast_ref::<&'static str>() {
      | Some(text) if *text == "fishForMe" => Some(*text),
      | _ => None,
    });

  assert_eq!(message, "fishForMe");

  probe.expect_msg_within(one_second(), "done");
}

// "A TestProbe" must { "be able to ignore primitive types" in { ... } }
#[test]
fn be_able_to_ignore_primitive_types() {
  let mut spec = TestProbeSpecCase::new();
  spec.ignore_msg(|message: &Message| message.downcast_ref::<i32>() == Some(&42));
  let test_actor = spec.test_actor();
  test_actor.tell(42_i32, test_actor.clone());
  test_actor.tell("pigdog", test_actor.clone());
  spec.expect_msg("pigdog");
}

// "A TestProbe" must { "watch actors when queue non-empty" in { ... } }
#[test]
fn watch_actors_when_queue_non_empty() {
  let mut spec = TestProbeSpecCase::new();
  let mut probe = spec.test_probe();
  let target = spec.system().actor_of(Props::new(|| EmptyActor));
  spec.system().stop(&target);
  probe.ref_handle().tell("hello", probe.ref_handle());
  probe.watch(&target);
  probe.expect_msg_within(one_second(), "hello");
  let terminated = probe.expect_msg_terminated_within(one_second());
  assert_eq!(terminated.target(), &target);
  assert!(!terminated.existence_confirmed());
  assert!(!terminated.address_terminated());
}

// "A TestProbe" must { "allow user-defined name" in { ... } }
#[test]
fn allow_user_defined_name() {
  let mut spec = TestProbeSpecCase::new();
  let probe = spec.test_probe_named("worker");
  assert!(probe.ref_handle().path().name().starts_with("worker"), "a TestProbe created with a name starts with it");
}

// "A TestProbe" must { "have reasonable default name" in { ... } }
#[test]
fn have_reasonable_default_name() {
  let mut spec = TestProbeSpecCase::new();
  let probe = spec.test_probe();
  assert!(
    probe.ref_handle().path().name().starts_with("testProbe"),
    "a TestProbe created without a name starts with testProbe"
  );
}

// "A TestProbe" must { "expectNoMessage should pull the thingy" in { ... } }
#[test]
fn expect_no_message_should_pull_the_thingy() {
  let mut spec = TestProbeSpecCase::new();
  let mut probe = spec.test_probe();

  probe.ref_handle().tell("nein", probe.ref_handle());
  probe.ref_handle().tell("nie", probe.ref_handle());

  spec.fraktor_spec.eventually(Duration::from_secs(3), Duration::from_millis(50), || {
    probe.expect_no_message(Duration::from_millis(100));
  });

  probe.ref_handle().tell("tak", probe.ref_handle());
  probe.expect_msg("tak");
}
