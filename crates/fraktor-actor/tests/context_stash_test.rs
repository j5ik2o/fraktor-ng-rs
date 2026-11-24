// Regression cases for `ActorContext::stash` and `ActorContext::unstash_all`.
//
// These are not ported from a Pekko test file and are not counted in the
// ledger. They pin the parts of Pekko's `UnrestrictedStash` contract that the
// two ported FSM cases (AbstractFSMActorTest and AbstractFSMWithStashActorTest)
// do not reach:
//
// - `unstashAll` prepends the stash to the mailbox, so stashed messages are
//   handled before messages that were already queued for the actor, whether
//   they wait at the shard boundary (sent from outside) or in the shard's local
//   queue (sent by an actor of the same shard).
// - The stash is first in, first out.
// - `preRestart` returns the stash to the mailbox, so the new instance
//   handles it.
// - The stash belongs to one actor: a new actor built from the same recipe
//   after a stop starts with an empty stash. Where a stopped actor's stash
//   goes (dead letters) is pinned by the executor's unit test, because the
//   port's dead letters do not publish a `DeadLetter` event yet.

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

const SPEC_SYSTEM_NAME: &str = "ContextStashTest";

fn text_of(message: &Message) -> &'static str {
  message.downcast_ref::<&'static str>().copied().unwrap_or("<not a text>")
}

// Stashes every text until `go`, then unstashes and tells the probe every text
// it receives. A `crash` fails the actor; the restarted instance starts in the
// forwarding mode, so it shows what the restart put back.
struct Stasher {
  probe: ActorRef,
  forwarding: bool,
}

impl Stasher {
  fn props(probe: ActorRef) -> Props {
    Props::new(move || Stasher { probe: probe.clone(), forwarding: false })
  }
}

impl Actor for Stasher {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    match text_of(&message) {
      | "crash" => panic!("crash (expected)"),
      | "go" if !self.forwarding => {
        self.forwarding = true;
        context.unstash_all();
      },
      | _ if !self.forwarding => context.stash(),
      | _ => context.tell(&self.probe, message),
    }
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.forwarding = true;
  }
}

// Sends the texts it is given to `target` in one delivery, so they all land in
// the shard's local queue before `target` handles the first one.
struct Burst {
  target: ActorRef,
  texts: &'static [&'static str],
}

impl Actor for Burst {
  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    for text in self.texts {
      context.tell(&self.target, *text);
    }
  }
}

fn expect_texts(probe: &mut TestProbe, expected: &[&str]) {
  for text in expected {
    probe.expect_msg(*text);
  }
}

#[test]
fn unstash_all_puts_the_stash_ahead_of_messages_waiting_at_the_boundary() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let stasher = spec.system_mut().actor_of(Stasher::props(probe.ref_handle()));
  let sender = spec.system().dead_letters();

  for text in ["a", "b", "go", "c"] {
    stasher.tell(text, sender.clone());
  }

  expect_texts(&mut probe, &["a", "b", "c"]);
  spec.terminate();
}

#[test]
fn unstash_all_puts_the_stash_ahead_of_messages_in_the_local_queue() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let stasher = spec.system_mut().actor_of(Stasher::props(probe.ref_handle()));
  let burst = spec.system_mut().actor_of(Props::new({
    let target = stasher.clone();
    move || Burst { target: target.clone(), texts: &["a", "b", "go", "c", "d"] }
  }));

  burst.tell("start", spec.system().dead_letters());

  expect_texts(&mut probe, &["a", "b", "c", "d"]);
  spec.terminate();
}

#[test]
fn unstash_all_keeps_the_stash_first_in_first_out_across_rounds() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let stasher = spec.system_mut().actor_of(Stasher::props(probe.ref_handle()));
  let sender = spec.system().dead_letters();

  for text in ["1", "2", "3", "4", "5", "go"] {
    stasher.tell(text, sender.clone());
  }

  expect_texts(&mut probe, &["1", "2", "3", "4", "5"]);
  spec.terminate();
}

#[test]
fn a_restart_returns_the_stash_to_the_new_instance_before_later_messages() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let stasher = spec.system_mut().actor_of(Stasher::props(probe.ref_handle()));
  let sender = spec.system().dead_letters();

  for text in ["a", "b", "crash", "c"] {
    stasher.tell(text, sender.clone());
  }

  expect_texts(&mut probe, &["a", "b", "c"]);
  spec.terminate();
}

#[test]
fn a_new_actor_does_not_inherit_a_stopped_actors_stash() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let stasher = spec.system_mut().actor_of(Stasher::props(probe.ref_handle()));
  probe.watch(&stasher);
  let sender = spec.system().dead_letters();

  stasher.tell("a", sender.clone());
  spec.system_mut().stop(&stasher);
  probe.expect_msg_terminated_within(core::time::Duration::from_secs(3));

  // A new actor of the same recipe starts with an empty stash.
  let next = spec.system_mut().actor_of(Stasher::props(probe.ref_handle()));
  next.tell("b", sender.clone());
  next.tell("go", sender);
  probe.expect_msg("b");
  probe.expect_no_message(core::time::Duration::from_millis(200));
  spec.terminate();
}
