// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/java/org/apache/pekko/actor/AbstractFSMWithStashActorTest.java
// First ported from v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff).
// The file was added on each line by a port of the same fix: 32f6cb2c01 #3477
// on 1.x (JUnit 4) and 90b02d6068 #3475 on 2.0 (JUnit 5). The two commits'
// changes to `AbstractFSM.scala` are the same patch; the test files differ only
// in the JUnit 4 and JUnit 5 scaffolding, and the one case and what it observes
// are the same.
//
// The original's one `@Test` case is kept as one test, named after the case,
// with the original name as a comment.
//
// The original subclasses `AbstractFSMWithStash<String, String>`, an FSM that
// mixes in `UnrestrictedStash`. Its class comment is about javac accepting the
// subclass, which is a Java/Scala inheritance concern this port does not
// reproduce. The runtime contract it checks is kept: in `start` the machine
// stashes every event except `go`; `go` unstashes them all and goes to `next`,
// where the machine tells the probe each event it receives. The stashed `work`
// therefore reaches the probe only after the transition.
//
// The port builds the same machine on the production `Fsm` inside the `Props`
// factory, which is the constructor body of the original:
//
// - `startWith("start", "data")` is `start_with`.
// - `when(state, matchEvent(String.class, (event, data) -> ...))` is `when`
//   with a state function that answers every text event.
// - `stash()` and `unstashAll()` are `ActorContext::stash` and
//   `ActorContext::unstash_all`, which act on the message being handled and on
//   the actor's mailbox, as `UnrestrictedStash` does.
// - `goTo("next")` and `stay()` are `FsmState::goto` and `FsmState::stay`.
// - `initialize()` is `initialize`.
//
// The actor runs on the real executor, and the probe observes what the
// production `Fsm` delivers after the stash is put back.
//
// The test calls the production `Fsm`, `FsmState`, `ActorContext::stash`, and
// `ActorContext::unstash_all`. Until they exist the file does not compile: that
// compile error is the intended red for this node.

use fraktor_actor::kernel::actor::{ActorContext, Fsm, FsmState, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

const SPEC_SYSTEM_NAME: &str = "AbstractFSMWithStashActorTest";

// Original: `public static class MyFSM extends AbstractFSMWithStash<String, String>`.
fn my_fsm(probe: ActorRef) -> Fsm<String, String> {
  let mut fsm = Fsm::new();
  fsm.start_with(String::from("start"), String::from("data"));
  fsm.when(String::from("start"), |context: &mut ActorContext<'_>, event: &Message, _data: &String| {
    let event = event.downcast_ref::<&'static str>()?;
    if *event == "go" {
      context.unstash_all();
      Some(FsmState::goto(String::from("next")))
    } else {
      context.stash();
      Some(FsmState::stay())
    }
  });
  fsm.when(String::from("next"), move |context: &mut ActorContext<'_>, event: &Message, _data: &String| {
    event.downcast_ref::<&'static str>()?;
    probe.tell(event.clone(), context.self_ref());
    Some(FsmState::stay())
  });
  fsm.initialize();
  fsm
}

// "canCreateFSMWithStash"
#[test]
fn can_create_fsm_with_stash() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());

  let probe_ref = probe.ref_handle();
  let fsm = spec.system_mut().actor_of(Props::new(move || my_fsm(probe_ref.clone())));
  // `ActorRef.noSender()` is the system's dead letters.
  fsm.tell("work", spec.system().dead_letters());
  fsm.tell("go", spec.system().dead_letters());

  probe.expect_msg("work");

  spec.terminate();
}
