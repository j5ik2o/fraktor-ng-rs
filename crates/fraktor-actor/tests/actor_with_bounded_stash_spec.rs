// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/actor/ActorWithBoundedStashSpec.scala
//
// The original's five cases under "An Actor with Stash must" are kept as
// one test each. The original gives the dispatcher and mailbox definitions
// as HOCON text; the port gives the same definitions as typed
// `DispatchSettings` values (design issue-192-specs.md section 1.2 and the
// faithfulness note E2 in section 5): dispatcher-1 and mailbox-1 are a
// bounded mailbox of capacity 10 with a stash capacity of 20,
// dispatcher-2 and mailbox-2 are a bounded mailbox of capacity 100 with a
// stash capacity of 20, and the aliases chain aliased-2 -> aliased-1 ->
// dispatcher-1. The original's `try { stash() } catch {
// StashOverflowException }` is the port's `ActorContext::try_stash`.

use std::sync::Arc;

use fraktor_actor::adaptor::{ChannelBoundary, UnwindCatcher};
use fraktor_actor::kernel::actor::{Actor, ActorContext, DeadLetter, DispatchSettings, MailboxDefinition, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::setup::ActorSystemSetup;
use fraktor_testkit::test_kit::TestKit;

const HELLOS: [&str; 21] = [
  "hello1", "hello2", "hello3", "hello4", "hello5", "hello6", "hello7", "hello8", "hello9", "hello10", "hello11",
  "hello12", "hello13", "hello14", "hello15", "hello16", "hello17", "hello18", "hello19", "hello20", "hello21",
];

fn text_of(message: &Message) -> &str {
  message.downcast_ref::<&'static str>().copied().unwrap_or("")
}

fn settings() -> DispatchSettings {
  let mut settings = DispatchSettings::new();
  settings.define_dispatcher("my-dispatcher-1", MailboxDefinition::bounded(10, 20));
  settings.define_dispatcher("my-dispatcher-2", MailboxDefinition::bounded(100, 20));
  settings.define_mailbox("my-mailbox-1", MailboxDefinition::bounded(10, 20));
  settings.define_mailbox("my-mailbox-2", MailboxDefinition::bounded(100, 20));
  settings.alias_dispatcher("my-aliased-dispatcher-1", "my-dispatcher-1");
  settings.alias_dispatcher("my-aliased-dispatcher-2", "my-aliased-dispatcher-1");
  settings
}

fn kit() -> TestKit {
  let setup = ActorSystemSetup::empty().with_setup(settings());
  let system = ActorSystem::with_setup_and_failure_catcher(
    "ActorWithBoundedStashSpec",
    Arc::new(ChannelBoundary::new()),
    setup,
    Arc::new(UnwindCatcher),
  );
  TestKit::new(system)
}

// Original: `StashingActor`. Stashes every "hello" and answers "ok"; on
// "world" it switches to a behaviour that stashes everything it receives
// and unstashes. The unstashed messages are therefore stashed again.
struct StashingActor {
  after_world: bool,
}

impl Actor for StashingActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if self.after_world {
      context.stash();
      return;
    }
    let text = text_of(&message);
    if text == "world" {
      self.after_world = true;
      context.unstash_all();
    } else if text.starts_with("hello") {
      context.stash();
      let sender = context.sender();
      context.tell(&sender, "ok");
    }
  }
}

// Original: `StashingActorWithOverflow`. Counts the stashed messages; the
// twenty-first stash overflows, is answered with "STASHOVERFLOW", and the
// actor stops itself.
struct StashingActorWithOverflow {
  stashed: usize,
}

impl Actor for StashingActorWithOverflow {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if !text_of(&message).starts_with("hello") {
      return;
    }
    self.stashed += 1;
    let sender = context.sender();
    match context.try_stash() {
      | Ok(()) => context.tell(&sender, "ok"),
      | Err(_) => {
        if self.stashed == 21 {
          context.tell(&sender, "STASHOVERFLOW");
          let me = context.self_ref();
          context.stop(&me);
        } else {
          context.tell(&sender, "Unexpected StashOverflowException");
        }
      },
    }
  }
}

fn expect_dead_letter(kit: &mut TestKit, expected: &'static str, sender: &ActorRef, recipient: &ActorRef) {
  let dead = kit.expect_msg_type::<DeadLetter>();
  assert_eq!(dead.message().downcast_ref::<&'static str>().copied(), Some(expected), "dead letter message");
  assert_eq!(&dead.sender(), sender, "dead letter sender for {expected}");
  assert_eq!(&dead.recipient(), recipient, "dead letter recipient for {expected}");
}

// Original: `testDeadLetters`.
fn test_dead_letters(kit: &mut TestKit, stasher: ActorRef) {
  let test_actor = kit.test_actor();
  kit.system().event_stream().subscribe(Some(test_actor.clone()), EventClass::of::<DeadLetter>()).expect("subscribe");
  for text in &HELLOS[..11] {
    stasher.tell(*text, test_actor.clone());
    kit.expect_msg("ok");
  }
  stasher.tell("world", test_actor.clone());
  expect_dead_letter(kit, "hello1", &test_actor, &stasher);
  stasher.tell("PoisonPill", test_actor.clone());
  for text in &HELLOS[1..11] {
    expect_dead_letter(kit, text, &test_actor, &stasher);
  }
}

// Original: `testStashOverflowException`.
fn test_stash_overflow(kit: &mut TestKit, stasher: ActorRef) {
  let test_actor = kit.test_actor();
  kit.system().event_stream().subscribe(Some(test_actor.clone()), EventClass::of::<DeadLetter>()).expect("subscribe");
  for text in &HELLOS[..20] {
    stasher.tell(*text, test_actor.clone());
    kit.expect_msg("ok");
  }
  stasher.tell("hello21", test_actor.clone());
  kit.expect_msg("STASHOVERFLOW");
  for text in &HELLOS[..20] {
    expect_dead_letter(kit, text, &test_actor, &stasher);
  }
}

// Original: "end up in DeadLetters in case of a capacity violation when configured via dispatcher"
#[test]
fn bounded_stash_dead_letters_on_unstash_overflow_via_dispatcher() {
  let mut kit = kit();
  let stasher = kit.system_mut().actor_of(Props::new(|| StashingActor { after_world: false }).with_dispatcher("my-dispatcher-1"));
  test_dead_letters(&mut kit, stasher);
}

// Original: "end up in DeadLetters in case of a capacity violation when configured via mailbox"
#[test]
fn bounded_stash_dead_letters_on_unstash_overflow_via_mailbox() {
  let mut kit = kit();
  let stasher = kit.system_mut().actor_of(Props::new(|| StashingActor { after_world: false }).with_mailbox("my-mailbox-1"));
  test_dead_letters(&mut kit, stasher);
}

// Original: "throw a StashOverflowException in case of a stash capacity violation when configured via dispatcher"
#[test]
fn bounded_stash_overflows_at_the_twenty_first_stash_via_dispatcher() {
  let mut kit = kit();
  let stasher =
    kit.system_mut().actor_of(Props::new(|| StashingActorWithOverflow { stashed: 0 }).with_dispatcher("my-dispatcher-2"));
  test_stash_overflow(&mut kit, stasher);
}

// Original: "throw a StashOverflowException in case of a stash capacity violation when configured via mailbox"
#[test]
fn bounded_stash_overflows_at_the_twenty_first_stash_via_mailbox() {
  let mut kit = kit();
  let stasher =
    kit.system_mut().actor_of(Props::new(|| StashingActorWithOverflow { stashed: 0 }).with_mailbox("my-mailbox-2"));
  test_stash_overflow(&mut kit, stasher);
}

// Original: "get stash capacity from aliased dispatchers"
#[test]
fn bounded_stash_resolves_stash_capacity_through_dispatcher_aliases() {
  let mut kit = kit();
  let stasher =
    kit.system_mut().actor_of(Props::new(|| StashingActor { after_world: false }).with_dispatcher("my-aliased-dispatcher-2"));
  test_dead_letters(&mut kit, stasher);
}
