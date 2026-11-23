// Ported from Pekko (Apache Pekko, actor-tests):
//   https://raw.githubusercontent.com/apache/pekko/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/actor/routing/ListenerSpec.scala
//
// Original test name is kept as a comment next to the ported case.
//
// The original case uses `TestLatch` to wait for two `foo` and two `bar`
// deliveries and an `AtomicInteger` for the total `bar` count. The port
// replaces each latch with a bounded, per-recipient observation on the real
// listener actors' mailboxes: a1 and a2 each must receive `foo` and `bar`
// within a deadline through their own `TestProbe`, a3 must receive nothing
// within a deadline while still running, and every listener counts `bar` in a
// shared counter so that the count stays 2.
//
// The original mixes the listener management (`Listeners`, `Listen`,
// `Deafen`, `WithListeners`) into the broadcaster actor. The port renames that
// vocabulary to `Subscribers`, `Subscribe`, `Unsubscribe`, and `WithSubscribers`
// and calls the matching production API from the broadcaster; the management set
// and the broadcast itself stay in production code and are not
// re-implemented in the test.
//
// Two delivery paths are kept apart:
//   1. `WithSubscribers(_ ! "foo")` -> subscriber management callback -> each live
//      subscriber receives `foo`.
//   2. a plain `"foo"` to the broadcaster -> production `broadcast("bar")` -> each
//      live subscriber receives `bar`.
//
// The original does not assert the order of the listener set or the sender of
// the delivered messages, so neither is observed here.

use core::sync::atomic::{AtomicUsize, Ordering};
use core::time::Duration;

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::broadcast::{Subscribe, Subscribers, Unsubscribe, WithSubscribers};
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

const SPEC_SYSTEM_NAME: &str = "ListenerSpec";

fn expect_timeout() -> Duration {
  Duration::from_secs(5)
}

fn no_delivery_timeout() -> Duration {
  Duration::from_millis(100)
}

// Each probe must receive one `foo` and one `bar` in either order, matching
// the original case's two independent latches.
fn next_message(probe: &mut TestProbe) -> Message {
  probe.fish_for_message(expect_timeout(), |_| true)
}

fn expect_foo_and_bar_in_any_order(probe: &mut TestProbe) {
  let first = next_message(probe);
  let second = next_message(probe);
  let is_foo = |message: &Message| message == "foo";
  let is_bar = |message: &Message| message == "bar";
  let ordered = is_foo(&first) && is_bar(&second);
  let reversed = is_bar(&first) && is_foo(&second);
  assert!(ordered || reversed, "expected exactly one `foo` and one `bar`, got {first:?} then {second:?}");
}

// Original: `Props(new Actor with Listeners { def receive = listenerManagement.orElse {
// case "foo" => gossip("bar") } })`
//
// The broadcaster holds the production listener management for its whole life,
// as the original's `Actor with Listeners` trait keeps the listener set on the
// instance. It defers to the production listener management for the management
// messages and, for a plain `"foo"`, asks the production API to broadcast
// `"bar"` to the current listeners.
struct Broadcaster {
  subscribers: Subscribers,
}

impl Actor for Broadcaster {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if self.subscribers.handle_management(&message) {
      return;
    }
    if message == "foo" {
      self.subscribers.broadcast("bar", context);
    }
  }
}

// Original: `Props(new Actor { def receive = {
//   case "bar" => barCount.incrementAndGet; barLatch.countDown()
//   case "foo" => fooLatch.countDown() } })`
//
// The listener forwards every received message to its own probe so the test
// observes both paths per recipient, and increments the shared `bar` counter on
// `"bar"`. It holds no listener set and chooses no destination.
struct Listener {
  probe: ActorRef,
  bar_count: std::sync::Arc<AtomicUsize>,
}

impl Actor for Listener {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message == "bar" {
      self.bar_count.fetch_add(1, Ordering::SeqCst);
    }
    self.probe.tell(message, context.self_ref());
  }
}

// "Listener" must { "listen" in { ... } }
#[test]
fn listen() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let bar_count = std::sync::Arc::new(AtomicUsize::new(0));

  let mut a1_probe = TestProbe::new(spec.system_mut());
  let mut a2_probe = TestProbe::new(spec.system_mut());
  let mut a3_probe = TestProbe::new(spec.system_mut());

  let broadcast = spec.system_mut().actor_of(Props::new(|| Broadcaster { subscribers: Subscribers::new() }));

  let new_listener = |spec: &mut FraktorSpec, probe: ActorRef| {
    let bar_count = bar_count.clone();
    spec.system_mut().actor_of(Props::new(move || Listener { probe: probe.clone(), bar_count: bar_count.clone() }))
  };

  let a1 = new_listener(&mut spec, a1_probe.ref_handle());
  let a2 = new_listener(&mut spec, a2_probe.ref_handle());
  let a3 = new_listener(&mut spec, a3_probe.ref_handle());

  let sender = spec.test_actor();
  broadcast.tell(Subscribe::new(a1.clone()), sender.clone());
  broadcast.tell(Subscribe::new(a2.clone()), sender.clone());
  broadcast.tell(Subscribe::new(a3.clone()), sender.clone());

  broadcast.tell(Unsubscribe::new(a3.clone()), sender.clone());

  // The callback is stored in the mailbox's `WithSubscribers` message, so it
  // must be `'static` and own its sender instead of borrowing `sender`.
  let foo_sender = sender.clone();
  broadcast
    .tell(WithSubscribers::new(move |listener: &ActorRef| listener.tell("foo", foo_sender.clone())), sender.clone());
  broadcast.tell("foo", sender);

  // Each path delivers one message to each listener that stays registered.
  expect_foo_and_bar_in_any_order(&mut a1_probe);
  expect_foo_and_bar_in_any_order(&mut a2_probe);

  // The deafened listener stays alive (deafened, not stopped) and receives
  // nothing from either path.
  a3_probe.expect_no_message(no_delivery_timeout());
  a3.tell("still alive", spec.test_actor());
  assert_eq!(next_message(&mut a3_probe), "still alive");

  // No delivery beyond the expected ones: a1 and a2 have consumed exactly one
  // `foo` and one `bar`, so their queues are empty.
  a1_probe.expect_no_message(no_delivery_timeout());
  a2_probe.expect_no_message(no_delivery_timeout());

  // The shared counter counts one `bar` per live listener on the real listeners.
  assert_eq!(bar_count.load(Ordering::SeqCst), 2, "expected bar delivered to exactly the two live listeners");

  for actor in [&broadcast, &a1, &a2, &a3] {
    spec.system_mut().stop(actor);
  }
  spec.terminate();
}

// Auxiliary case, kept separate from the original `listen` case and counted
// apart in the ledger: the original subscribes each listener once. Registering
// the same listener twice must leave one entry in the set, so the two delivery
// paths reach it once each. A set that kept the duplicate would deliver an
// extra `foo` and an extra `bar`, which the trailing `expect_no_message` on the
// probe rejects.
#[test]
fn subscribe_same_listener_twice_delivers_once_per_path() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let bar_count = std::sync::Arc::new(AtomicUsize::new(0));

  let mut a1_probe = TestProbe::new(spec.system_mut());

  let broadcast = spec.system_mut().actor_of(Props::new(|| Broadcaster { subscribers: Subscribers::new() }));

  let bar_count_for_listener = bar_count.clone();
  let a1_probe_ref = a1_probe.ref_handle();
  let a1 = spec
    .system_mut()
    .actor_of(Props::new(move || Listener { probe: a1_probe_ref.clone(), bar_count: bar_count_for_listener.clone() }));

  let sender = spec.test_actor();
  broadcast.tell(Subscribe::new(a1.clone()), sender.clone());
  broadcast.tell(Subscribe::new(a1.clone()), sender.clone());

  let foo_sender = sender.clone();
  broadcast
    .tell(WithSubscribers::new(move |listener: &ActorRef| listener.tell("foo", foo_sender.clone())), sender.clone());
  broadcast.tell("foo", sender);

  // Path 1 and path 2 each reach the listener once; the order is not fixed.
  expect_foo_and_bar_in_any_order(&mut a1_probe);
  assert_eq!(bar_count.load(Ordering::SeqCst), 1, "a duplicate registration must not deliver `bar` twice");

  // No delivery beyond the expected ones: a duplicate entry would leave an
  // extra `foo` and an extra `bar` in the queue, which this check rejects.
  a1_probe.expect_no_message(no_delivery_timeout());

  for actor in [&broadcast, &a1] {
    spec.system_mut().stop(actor);
  }
  spec.terminate();
}
