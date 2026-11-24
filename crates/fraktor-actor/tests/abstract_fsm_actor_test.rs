// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/java/org/apache/pekko/actor/AbstractFSMActorTest.java
// First ported from v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff).
// v2.0.0 changes the file only for JUnit 5 and newer Java syntax (upstream
// ec27d6dd8c #2724, 96b716b6fb #3191, 4ae53394ac #3295); the case is unchanged.
//
// The original's one `@Test` case is kept as one test, named after the case,
// with the original name as a comment.
//
// The original subclasses `AbstractFSM<String, String>` and sets the machine up
// in the constructor. The port builds the same machine on the production `Fsm`
// inside the `Props` factory, which is the constructor body of the original:
//
// - `onTransition(this::logTransition)` is `on_transition`, whose handler gets
//   the actor's context so it can name the actor as the sender, as
//   `getSelf()` does in the original.
// - `startWith("start", "data")` is `start_with`.
// - `when("start", matchEventEquals("next", (newState, data) -> goTo(newState)))`
//   is `when` with a state function that answers only the event `"next"` and
//   goes to the state the event names.
// - `when("next", AbstractFSM.NullFunction())` is `when` with a state function
//   that answers no event.
// - `initialize()` is `initialize`; the original's `initialize` reports the
//   transition into the initial state, `start -> start`.
//
// The actor runs on the real executor, and the probe observes the transition
// notices the production `Fsm` sends.
//
// The test calls the production `Fsm` and `FsmState`. Until they exist the
// file does not compile: that compile error is the intended red for this node.

use fraktor_actor::kernel::actor::{ActorContext, Fsm, FsmState, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

const SPEC_SYSTEM_NAME: &str = "AbstractFSMActorTest";

// Original: `public static class MyFSM extends AbstractFSM<String, String>`.
fn my_fsm(probe: ActorRef) -> Fsm<String, String> {
  let mut fsm = Fsm::new();
  // Original: `logTransition`, which tells the probe
  // `String.format("Transitioning from %1$s to %2$s.", s1, s2)` with
  // `getSelf()` as the sender.
  fsm.on_transition(move |context: &mut ActorContext<'_>, from: &String, to: &String| {
    probe.tell(format!("Transitioning from {from} to {to}."), context.self_ref());
  });
  fsm.start_with(String::from("start"), String::from("data"));
  fsm.when(String::from("start"), |_context: &mut ActorContext<'_>, event: &Message, _data: &String| {
    let new_state = event.downcast_ref::<&'static str>().filter(|event| **event == "next")?;
    Some(FsmState::goto(String::from(*new_state)))
  });
  fsm.when(String::from("next"), |_context: &mut ActorContext<'_>, _event: &Message, _data: &String| None);
  fsm.initialize();
  fsm
}

// "canCreateFSM"
#[test]
fn can_create_fsm() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());

  let probe_ref = probe.ref_handle();
  let fsm = spec.system_mut().actor_of(Props::new(move || my_fsm(probe_ref.clone())));
  let initial: String = probe.expect_msg_type();
  assert_eq!(initial, "Transitioning from start to start.");

  // `ActorRef.noSender()` is the system's dead letters.
  fsm.tell("next", spec.system().dead_letters());

  let transition: String = probe.expect_msg_type();
  assert_eq!(transition, "Transitioning from start to next.");

  spec.terminate();
}
