// Ported from Pekko (Apache Pekko, actor-tests):
//   actor-tests/src/test/scala/org/apache/pekko/actor/DeadLetterSuspensionSpec.scala
//   (Pekko v2.0.0, commit 9fbd2283b1cf701c90a1223bf621942c191db61f)
// Original test name is kept as a comment next to the ported case.
//
// The original builds its system from a configuration string
// (`pekko.log-dead-letters = 4`, `pekko.log-dead-letters-suspend-duration =
// 2s`). The port has no configuration-string constructor, so the test hands
// the same values to `start_dead_letter_listener` as a settings value.
//
// The original waits out the suspend duration with `Thread.sleep(2050)`. The
// port installs a manual clock and advances it past the deadline, which
// keeps the case deterministic: the listener reads the clock the test
// installed, and the resume happens on the first event after the deadline.

use core::num::NonZeroU32;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use core::time::Duration;
use std::sync::Arc;

use fraktor_actor::kernel::actor::Actor;
use fraktor_actor::kernel::actor::ActorContext;
use fraktor_actor::kernel::actor::DeadLetterLogLimit;
use fraktor_actor::kernel::actor::DeadLetterSettings;
use fraktor_actor::kernel::actor::Dropped;
use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::actor::Terminated;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::Clock;
use fraktor_testkit::event_filter::EventFilter;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_actors::echo_actor_props;

/// A clock the test advances by hand, in milliseconds.
struct ManualClock {
  now_ms: AtomicU64,
}

impl ManualClock {
  fn new() -> Self {
    Self { now_ms: AtomicU64::new(0) }
  }

  fn advance(&self, duration: Duration) {
    self.now_ms.fetch_add(duration.as_millis() as u64, Ordering::SeqCst);
  }
}

impl Clock for ManualClock {
  fn monotonic(&self) -> Duration {
    Duration::from_millis(self.now_ms.load(Ordering::SeqCst))
  }
}

// Original: `Dropping` publishes a `Dropped` event for every number it gets.
struct Dropping;

impl Actor for Dropping {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(number) = message.downcast_ref::<i32>() {
      let dropped = Dropped::new(*number, "Don't like numbers", context.dead_letters(), context.self_ref());
      context.publish_event(dropped);
    }
  }
}

// Original: `Unandled` reports every number it gets as unhandled.
struct UnhandledActor;

impl Actor for UnhandledActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(number) = message.downcast_ref::<i32>() {
      context.unhandled(*number);
    }
  }
}

fn path(reference: &ActorRef) -> String {
  reference.path().to_string()
}

// Original: "must suspend dead-letters logging when reaching
// 'pekko.log-dead-letters', and then re-enable"
#[test]
fn suspend_dead_letters_logging_at_the_limit_and_re_enable() {
  let mut spec = FraktorSpec::new("DeadLetterSuspensionSpec");
  let clock = Arc::new(ManualClock::new());
  spec.system_mut().set_clock(clock.clone());
  let limit = DeadLetterLogLimit::Count(NonZeroU32::new(4).expect("4 is not zero"));
  spec.system_mut().start_dead_letter_listener(DeadLetterSettings::new(limit, Some(Duration::from_secs(2))));

  let test_actor = spec.test_actor();
  let dead_actor = spec.system_mut().actor_of(echo_actor_props());
  spec.test_kit_mut().watch(&dead_actor);
  dead_actor.tell("PoisonPill", test_actor.clone());
  let terminated: Terminated = spec.expect_msg_type();
  assert_eq!(terminated.target(), &dead_actor);

  let dropping_actor = spec.system_mut().actor_of(Props::new(|| Dropping));
  let unhandled_actor = spec.system_mut().actor_of(Props::new(|| UnhandledActor));

  let type_name = core::any::type_name::<i32>();
  let dead_expected = |count: u32| {
    format!(
      "Message [{type_name}] from {} to {} was not delivered. [{count}] dead letters encountered",
      path(&test_actor),
      path(&dead_actor)
    )
  };
  let dropped_expected = |count: u32| {
    format!(
      "Message [{type_name}] to {} was dropped. Don't like numbers. [{count}] dead letters encountered",
      path(&dropping_actor)
    )
  };
  let unhandled_expected = |count: u32| {
    format!(
      "Message [{type_name}] from {} to {} was unhandled. [{count}] dead letters encountered",
      path(&test_actor),
      path(&unhandled_actor)
    )
  };

  let system = spec.system_mut();
  EventFilter::info().start(&dead_expected(1)).intercept(system, || {
    dead_actor.tell(1_i32, test_actor.clone());
  });
  let system = spec.system_mut();
  EventFilter::info().start(&dropped_expected(2)).intercept(system, || {
    dropping_actor.tell(2_i32, test_actor.clone());
  });
  let system = spec.system_mut();
  EventFilter::info().start(&unhandled_expected(3)).intercept(system, || {
    unhandled_actor.tell(3_i32, test_actor.clone());
  });
  let system = spec.system_mut();
  EventFilter::info()
    .start(&(dead_expected(4) + ", no more dead letters will be logged in next"))
    .intercept(system, || {
      dead_actor.tell(4_i32, test_actor.clone());
    });

  dead_actor.tell(5_i32, test_actor.clone());
  dropping_actor.tell(6_i32, test_actor.clone());
  spec.system_mut().run_until_idle();

  // let the suspend duration elapse
  clock.advance(Duration::from_millis(2050));

  let system = spec.system_mut();
  EventFilter::info()
    .start(&(dead_expected(7) + ", of which 2 were not logged"))
    .intercept(system, || {
      dead_actor.tell(7_i32, test_actor.clone());
    });

  // the counter was reset by the resume
  let system = spec.system_mut();
  EventFilter::info().start(&dead_expected(1)).intercept(system, || {
    dead_actor.tell(8_i32, test_actor.clone());
  });

  spec.terminate();
}
