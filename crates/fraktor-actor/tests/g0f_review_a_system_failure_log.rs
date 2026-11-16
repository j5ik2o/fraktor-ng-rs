// Reproduction for review thread A:
//   the system failure log (Error) does not reach the EventStream.
//
// The bot example (Devin, actor_system_value.rs:56 / CodeRabbit
// actor_system_value.rs:55) says: subscribe to `Error` on the actor system's
// event stream, then fail an actor; Pekko publishes the failure as an `Error`
// on the very stream the subscription sits on, so the subscriber sees it. The
// claim is that the port publishes the failure to a sink that is not the event
// stream, so the `Error` never arrives.
//
// This test checks only that claim: an actor that fails at start is killed, and
// a probe subscribed to `Error` on `system.event_stream()` either receives the
// `Error` (green: the claim does not hold) or does not (red: the claim holds).

use core::time::Duration;

use fraktor_actor::kernel::actor::Logging::Error;
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::implicit_sender::ImplicitSender;

// The actor whose failure the parent logs as an `Error`.
struct IgnoringActor;

impl Actor for IgnoringActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

fn fault_wait() -> Duration {
  Duration::from_secs(3)
}

#[test]
fn g0f_review_a_system_failure_error_reaches_the_event_stream() {
  let mut spec = FraktorSpec::new("G0fReviewA");
  let stream = spec.system().event_stream();

  // Subscribe the test actor to `Error` on the system's own event stream, which
  // is the subscription the bot's example names.
  assert!(stream.subscribe(Some(spec.test_actor()), EventClass::of::<Error>()).expect("the test actor is not null"));

  let victim = spec.system_mut().actor_of(Props::new(|| IgnoringActor));
  let sender = ImplicitSender::self_actor_ref(&spec);
  victim.tell("Kill", sender);
  spec.system_mut().run_until_idle();

  // Pekko logs an `Error` for the killed child, and the subscription on the
  // stream sees it.
  spec.within(fault_wait(), |spec| {
    spec.expect_msg_type::<Error>();
  });
  spec.terminate();
}
