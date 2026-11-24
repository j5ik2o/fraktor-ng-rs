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
// - A stop releases the stash (Pekko hands it to dead letters; where it goes is
//   pinned by the executor's unit test, because the port's dead letters do not
//   publish a `DeadLetter` event yet), and a new actor built from the same
//   recipe starts with an empty stash.
// - Terminating the system releases every stash while the system value is
//   still held, as it releases the queues.
// - An unstashed `Terminated` is re-received, and an `unwatch` of its target
//   right after `unstashAll` cancels it, because Pekko's `enqueueFirst` queues
//   it again as a DeathWatch notice (`terminatedQueuedFor`). The first case is
//   the runtime contract of ActorWithStashSpec's "re-receive unstashed
//   Terminated messages"; that node stays unported in the ledger.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props, Terminated};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

const SPEC_SYSTEM_NAME: &str = "ContextStashTest";

fn text_of(message: &Message) -> &'static str {
  message.downcast_ref::<&'static str>().copied().unwrap_or("<not a text>")
}

// Stashes every message until `go`, then unstashes and tells the probe every
// text it receives. A `ping` while stashing answers `pong`, so a test knows the
// messages before it have been stashed. A `crash` fails the actor; the
// restarted instance starts in the forwarding mode, so it shows what the
// restart put back.
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
      | "ping" if !self.forwarding => context.tell(&self.probe, "pong"),
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

// A payload that counts how many times it has been dropped, so a test can see
// that the stash holding it was released.
struct ReleaseCounter(Arc<AtomicUsize>);

impl Drop for ReleaseCounter {
  fn drop(&mut self) {
    self.0.fetch_add(1, Ordering::SeqCst);
  }
}

#[test]
fn a_stop_releases_the_stash_and_a_new_actor_does_not_inherit_it() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let stasher = spec.system_mut().actor_of(Stasher::props(probe.ref_handle()));
  probe.watch(&stasher);
  let sender = spec.system().dead_letters();
  let released = Arc::new(AtomicUsize::new(0));

  stasher.tell(ReleaseCounter(released.clone()), sender.clone());
  stasher.tell("a", sender.clone());
  stasher.tell("ping", sender.clone());
  probe.expect_msg("pong");
  assert_eq!(released.load(Ordering::SeqCst), 0, "the payload is held by the stash");

  spec.system_mut().stop(&stasher);
  probe.expect_msg_terminated_within(core::time::Duration::from_secs(3));
  assert_eq!(released.load(Ordering::SeqCst), 1, "the stop released the stash");

  // A new actor of the same recipe starts with an empty stash.
  let next = spec.system_mut().actor_of(Stasher::props(probe.ref_handle()));
  next.tell("b", sender.clone());
  next.tell("go", sender);
  probe.expect_msg("b");
  probe.expect_no_message(core::time::Duration::from_millis(200));
  spec.terminate();
}

#[test]
fn terminating_the_system_releases_the_stash_while_the_system_is_held() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let stasher = spec.system_mut().actor_of(Stasher::props(probe.ref_handle()));
  let sender = spec.system().dead_letters();
  let released = Arc::new(AtomicUsize::new(0));

  stasher.tell(ReleaseCounter(released.clone()), sender.clone());
  stasher.tell("ping", sender);
  probe.expect_msg("pong");
  assert_eq!(released.load(Ordering::SeqCst), 0, "the payload is held by the stash");

  spec.terminate();

  assert_eq!(released.load(Ordering::SeqCst), 1, "the terminate released the stash");
  drop(spec);
}

// Watches a child, stops it, and stashes the first `Terminated` for it before
// unstashing it at once, as ActorWithStashSpec's TerminatedMessageStashingActor
// does. Every `Terminated` it handles is reported to the probe. With
// `unwatch_after_unstash`, it unwatches the child right after `unstash_all`.
struct TerminatedStasher {
  probe: ActorRef,
  unwatch_after_unstash: bool,
  watched: Option<ActorRef>,
  stashed: bool,
}

impl TerminatedStasher {
  fn props(probe: ActorRef, unwatch_after_unstash: bool) -> Props {
    Props::new(move || TerminatedStasher { probe: probe.clone(), unwatch_after_unstash, watched: None, stashed: false })
  }
}

struct Idle;

impl Actor for Idle {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

impl Actor for TerminatedStasher {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let watched = context.actor_of(Props::new(|| Idle), "watched").expect("the child name is free");
    context.watch(&watched);
    context.stop(&watched);
    self.watched = Some(watched);
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if !message.is::<Terminated>() {
      return;
    }
    if !self.stashed {
      context.stash();
      self.stashed = true;
      context.unstash_all();
      if self.unwatch_after_unstash {
        let watched = self.watched.clone().expect("the child was started in pre_start");
        context.unwatch(&watched);
      }
    }
    context.tell(&self.probe, "terminated");
  }
}

#[test]
fn an_unstashed_terminated_is_received_again() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  spec.system_mut().actor_of(TerminatedStasher::props(probe.ref_handle(), false));

  probe.expect_msg("terminated");
  probe.expect_msg("terminated");
  spec.terminate();
}

#[test]
fn an_unwatch_after_unstash_all_cancels_the_unstashed_terminated() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  spec.system_mut().actor_of(TerminatedStasher::props(probe.ref_handle(), true));

  probe.expect_msg("terminated");
  probe.expect_no_message(core::time::Duration::from_millis(200));
  spec.terminate();
}
