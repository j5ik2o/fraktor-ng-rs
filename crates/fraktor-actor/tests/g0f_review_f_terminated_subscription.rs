// Reproduction for review thread F:
//   the subscription of an already-terminated actor is not removed.
//
// The bot example (Devin actor_event_bus.rs:84 / CodeRabbit :78) says: stop an
// actor of the same system, then subscribe it to a classifier; the subscription
// stays in the table forever, because the termination callback never reports a
// past termination and `subscribe` does not check whether the actor is alive.
// Pekko's `ActorClassificationUnsubscriber` registers `context.watch(actor)`,
// which reports a termination that already happened at once, so the
// subscription is removed right away.
//
// The bus answers `unsubscribe` with `true` only while the table still holds the
// pair. After a terminated actor subscribed, a correct port has already dropped
// the subscription, so `unsubscribe` answers `false`; the port that keeps it
// answers `true`. That is the observable this test uses.

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::event::ActorEventBus;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::implicit_sender::ImplicitSender;

struct IgnoringActor;

impl Actor for IgnoringActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

#[derive(Clone, Debug)]
struct Notice;

#[test]
fn g0f_review_f_subscription_of_a_terminated_actor_is_removed() {
  let mut spec = FraktorSpec::new("G0fReviewF");

  // The classifier handle names the channel the pair is filed under.
  let classifier: ActorRef = spec.system_mut().actor_of(Props::new(|| IgnoringActor));
  let classify = {
    let classifier = classifier.clone();
    move |_event: &Notice| classifier.clone()
  };
  let mut bus =
    ActorEventBus::new(spec.system().downgrade(), spec.system().event_stream(), "G0fReviewF.Bus", false, classify);

  // A subscriber of the same system that is already terminated.
  let subscriber = spec.system_mut().actor_of(Props::new(|| IgnoringActor));
  let sender = ImplicitSender::self_actor_ref(&spec);
  subscriber.tell("PoisonPill", sender);
  spec.system_mut().run_until_idle();

  // Subscribe the already-terminated subscriber. The bus accepts the pair as
  // Pekko's `subscribe` does; the removal comes from observing the end of the
  // subscriber, not from refusing the subscription.
  let added = bus.subscribe(subscriber.clone(), classifier.clone());
  assert!(added, "the subscription is new");

  // The removal already happened inside `subscribe`: the bus reports a
  // subscriber that had stopped as terminated at once and drops the pair
  // synchronously, so the assertion below does not depend on driving the
  // executor. The call is kept only to drain any message left in flight.
  spec.system_mut().run_until_idle();

  // A terminated subscriber must already be gone from the table, so removing
  // the pair answers `false`.
  let removed = bus.unsubscribe(&subscriber, &classifier);
  assert!(!removed, "a terminated subscriber must have been unsubscribed automatically");
  spec.terminate();
}
