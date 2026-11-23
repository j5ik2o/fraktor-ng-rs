// Ported from Pekko v1.7.1:
//   actor-tests/src/test/scala/org/apache/pekko/actor/UidClashTest.scala
//   https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/actor/UidClashTest.scala
//
// The original has one case:
//   "The Terminated message for an old child stopped in preRestart" should
//     "not arrive after restart"
//
// The mechanism is kept as the original writes it:
// - The supervisor-side probe watches the parent and asks it to restart the child.
// - The child's preRestart stores the old grandchild, unwatches it, and stops it.
// - postRestart watches a colliding reference that carries the old grandchild's
//   full path and live UID, then sends PingMyself to itself.
// - PingMyself answers RestartedSafely to itself; RestartedSafely reaches the
//   parent and then the probe.
// - A Terminated that actually reaches the child fails the test immediately.
//
// The two-step self send is the original's trick to make the case positive
// without `expectNoMessage`: anything enqueued during postRestart is delivered
// before RestartedSafely, so the test completes only when the child receives no
// bogus Terminated. The observation is event driven; an absence of messages over
// a duration is never a pass.

use std::sync::Arc;
use std::sync::Mutex;

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props, Terminated};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::supervision::{Directive, FailureKind, SupervisorStrategy};
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

const SPEC_SYSTEM_NAME: &str = "UidClashTest";

// Original: `case object PleaseRestart`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PleaseRestart;

// Original: `case object PingMyself`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PingMyself;

// Original: `case object RestartedSafely`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RestartedSafely;

// Original: the "ignore everything" nature of `EvilCollidingActorRef`. The port
// expresses it as a boundary that swallows every message, which is what a
// stopped/no-op reference does with ordinary traffic. It is combined with the
// old child's own identifier so the handle collides with the old child while
// staying a distinct reference.
struct NoOpBoundary;

impl fraktor_actor::kernel::executor::ShardBoundary for NoOpBoundary {
  fn post(&self, _envelope: fraktor_actor::kernel::message::Envelope) {}

  fn poll(&self) -> Option<fraktor_actor::kernel::message::Envelope> {
    None
  }

  fn poll_within(&self, _max: core::time::Duration) -> Option<fraktor_actor::kernel::message::Envelope> {
    None
  }

  fn has_pending(&self) -> bool {
    false
  }

  fn close(&self) {}
}

// The original reuses the old child's own identity for the colliding reference
// (`system.provider`, `oldActor.path`, and the live UID). A colliding reference
// is a second `ActorRef` over the old child's own identity, so the test asks the
// production surface for one instead of writing the identity by hand: the fixed
// `ActorRef::new(0, 0, ...)` values are not used. The port has no such operation
// today: the fields of `ActorRef` are private and `ActorRef::new` is
// `pub(crate)` (`crates/fraktor-actor/src/kernel/actor_ref/actor_ref_value.rs:31`),
// and equality uses only the identifier and the generation (`:79`), so a
// reference the test builds by any other route cannot collide with the old
// child. `ActorRef::with_boundary` is the typed operation the test calls: it
// reuses the recipient's identifier, generation, and path and swaps only the
// boundary, and is called on the old child's own handle so the identity is never
// written by hand. `boundary_key_eq` proves the result is a separate reference
// over that same identity. Until the production surface offers them, these
// calls are the intended compile red, and the second phase adds them instead of
// weakening the case.
fn create_colliding_ref(old: &ActorRef) -> ActorRef {
  old.with_boundary(Arc::new(NoOpBoundary))
}

// Records the delivery of a Terminated to the restarted child. The port cannot
// recover the thrown value that Pekko's `TerminatedForNonWatchedActor` carries,
// so the failure is carried by this shared report, and the test reads it after
// the run so a caught panic cannot turn a bogus notice into a pass.
#[derive(Default)]
struct TerminationReport {
  received: Option<ActorRef>,
}

struct Shared {
  old_actor: Mutex<Option<ActorRef>>,
  termination: Mutex<TerminationReport>,
}

fn record_termination(shared: &Shared, terminated: &Terminated) {
  let mut report = shared.termination.lock().expect("the termination report is not poisoned");
  if report.received.is_none() {
    report.received = Some(terminated.target().clone());
  }
}

// Original: `class RestartedActor`, whose `receive` treats a Terminated as the
// failure, pings itself, and answers a ping with RestartedSafely.
//
// The original's `preStart` creates and watches one grandchild, so the port
// keeps that single child in a field instead of asking the context for its
// children, and `preRestart` reuses the kept handle rather than scanning a
// children getter the port does not offer.
struct RestartedActor {
  driver: ActorRef,
  shared: Arc<Shared>,
  child: Option<ActorRef>,
}

impl Actor for RestartedActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<PleaseRestart>().is_some() {
      panic!("restart");
    }
    if let Some(terminated) = message.downcast_ref::<Terminated>() {
      record_termination(&self.shared, terminated);
      let driver = self.driver.clone();
      context.stop(&driver);
      return;
    }
    if message.downcast_ref::<PingMyself>().is_some() {
      let self_ref = context.self_ref();
      context.tell(&self_ref, RestartedSafely);
      return;
    }
    if message.downcast_ref::<RestartedSafely>().is_some() {
      let driver = self.driver.clone();
      context.tell(&driver, RestartedSafely);
    }
  }

  // Original: `preStart` creates and watches the grandchild.
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let child = match context.actor_of(Props::new(|| EmptyActor), "child") {
      | Ok(child) => child,
      | Err(error) => panic!("the restarted actor could not start its child: {error}"),
    };
    self.child = Some(context.watch(&child));
  }

  // Original: `preRestart` saves the old child, unwatches it, and stops it.
  //
  // The original iterates `context.children.foreach`; the case has exactly one
  // grandchild, held in `self.child`, so the kept handle is reused instead of a
  // children getter. The original calls `context.unwatch(child)`. The port's
  // `ActorContext` has no `unwatch` yet; the minimum operation the test needs is
  // `unwatch(target)`, which drops this actor's watch on `target` alone. Routing
  // it through `WeakActorSystem::unwatch` does not work: that handle is
  // `!Send + !Sync` (`crates/fraktor-actor/src/kernel/actor_system/weak_actor_system.rs:27`)
  // and would re-borrow the same executor the hook is running inside. So the
  // hook keeps the single-argument `context.unwatch(&child)` call and the
  // missing method stays the intended compile red for the first phase; the
  // second phase adds `unwatch(target)`.
  fn pre_restart(&mut self, context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    let child = self.child.clone().expect("pre_start created the grandchild");
    *self.shared.old_actor.lock().expect("the old actor slot is not poisoned") = Some(child.clone());
    context.unwatch(&child);
    context.stop(&child);
    self.child = None;
  }

  // Original: `postRestart` watches the colliding reference and pings itself.
  //
  // The original relies on the old child having stopped during the restart. The
  // port checks that same precondition through the context before it watches,
  // so a watch of a still-live child is a test failure rather than a silent
  // pass.
  fn post_restart(&mut self, context: &mut ActorContext<'_>) {
    let old = self.shared.old_actor.lock().expect("the old actor slot is not poisoned").clone();
    let old = old.expect("pre_restart saved the old child");
    assert!(
      context.is_terminated(&old),
      "the old child must be stopped before post_restart watches the colliding reference"
    );
    let colliding = create_colliding_ref(&old);
    assert_eq!(colliding.path(), old.path(), "the colliding reference must carry the old child's full path");
    assert!(colliding == old, "the colliding reference must carry the old child's live UID");
    assert!(!colliding.boundary_key_eq(&old), "the colliding reference must be a distinct reference");
    let _ = context.watch(&colliding);
    let self_ref = context.self_ref();
    context.tell(&self_ref, PingMyself);
  }
}

// The grandchild that `pre_restart` stops; it has no behaviour of its own.
struct EmptyActor;

impl Actor for EmptyActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

// Original: `class RestartingActor(probe: ActorRef)`. It owns the restarted
// child, restarts it for the failure that asks for a restart, and forwards a
// RestartedSafely from the child to the probe. A child that received a bogus
// Terminated stops its driver, so the decider stops instead of restarting.
//
// `pre_start` creates the child once and keeps its handle, as the original keeps
// `theRestartedOne` in a field; the receive path reuses that handle so a second
// child of the same name is never started.
struct RestartingActor {
  probe: ActorRef,
  shared: Arc<Shared>,
  restarted_one: Option<ActorRef>,
}

impl Actor for RestartingActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<PleaseRestart>().is_some() {
      let child = self.restarted_one.clone().expect("pre_start created the restarted child");
      context.tell(&child, PleaseRestart);
      return;
    }
    if message.downcast_ref::<RestartedSafely>().is_some() {
      let probe = self.probe.clone();
      context.tell(&probe, RestartedSafely);
    }
  }

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    let shared = Arc::clone(&self.shared);
    SupervisorStrategy::one_for_one(1).with_decider(move |_child, _kind: FailureKind| {
      let recorded = {
        let report = shared.termination.lock().expect("the termination report is not poisoned");
        report.received.is_some()
      };
      if recorded {
        Directive::Stop
      } else {
        Directive::Restart
      }
    })
  }

  // The original creates the child in its constructor; the port's hooks run
  // after the strategy is read, so the child is started here exactly once.
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let driver = context.self_ref();
    let props = Props::new({
      let shared = Arc::clone(&self.shared);
      move || RestartedActor { driver: driver.clone(), shared: Arc::clone(&shared), child: None }
    });
    let child = match context.actor_of(props, "theRestartedOne") {
      | Ok(child) => child,
      | Err(error) => panic!("the parent could not start the restarted child: {error}"),
    };
    self.restarted_one = Some(child);
  }
}

// Original: the single case of `UidClashTest`.
#[test]
fn the_terminated_message_for_an_old_child_stopped_in_pre_restart_does_not_arrive_after_restart() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut watcher = TestProbe::with_name(spec.system_mut(), "watcher");
  let probe_ref = watcher.ref_handle();
  let shared = Arc::new(Shared { old_actor: Mutex::new(None), termination: Mutex::new(TerminationReport::default()) });
  let parent_props = Props::new({
    let probe = probe_ref.clone();
    let shared = Arc::clone(&shared);
    move || RestartingActor { probe: probe.clone(), shared: Arc::clone(&shared), restarted_one: None }
  });
  let top = spec.system_mut().actor_of(parent_props);
  watcher.watch(&top);

  // Original: `topActor ! PleaseRestart; watcher.expectMsg(RestartedSafely)`
  let sender = watcher.ref_handle();
  top.tell(PleaseRestart, sender);
  watcher.expect_msg_type::<RestartedSafely>();

  // The original relies on the delivery order for the failure: the case is
  // positive only because no bogus Terminated is enqueued during the restart.
  // Draining the queue here makes a notice that was enqueued but not yet run
  // observable, so the failure is decided by what the child actually received,
  // not by the absence of a message over a duration.
  spec.system_mut().run_until_idle();

  // Original: the failure surfaces only as the thrown
  // `TerminatedForNonWatchedActor`; the port records the same failure in the
  // shared report, so a caught panic is still a failure here.
  {
    let report = shared.termination.lock().expect("the termination report is not poisoned");
    assert!(
      report.received.is_none(),
      "the restarted actor received a Terminated for a reference it did not watch: {:?}",
      report.received
    );
  }
  let old = shared.old_actor.lock().expect("the old actor slot is not poisoned").clone();
  let old = old.expect("pre_restart saved the old child");
  assert!(spec.system().is_actor_terminated(&old), "the old child stopped by pre_restart must be terminated");
  spec.terminate();
}
