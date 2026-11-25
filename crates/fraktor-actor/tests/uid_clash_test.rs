// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file is the same as in v1.7.1.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
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

// The original reuses the old child's own identity for the colliding reference
// (`system.provider`, `oldActor.path`, and the live UID). A colliding reference
// is a second `ActorRef` over the old child's own identity, so the test asks the
// production surface for one instead of writing the identity by hand: the fixed
// `ActorRef::new(0, 0, ...)` values are not used. `ActorContext::stopped_noop_ref`
// is that limited operation: it reuses the stopped actor's identifier,
// generation, and path over a delivery entity that ignores everything, and
// refuses an actor that has not stopped. The ActorRef unit test checks that the
// new handle has a separate delivery boundary.
fn create_colliding_ref(context: &ActorContext<'_>, old: &ActorRef) -> ActorRef {
  context.stopped_noop_ref(old)
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
// The original throws `TerminatedForNonWatchedActor` from `receive`; its
// supervisor strategy maps that exception to `context.stop(self); Stop`. The port
// expresses the same failure as a panic so the supervision machinery sees it:
// the receipt is first recorded in the shared report (the value a caught panic
// would otherwise hide), the original's `context.stop(self)` becomes stopping
// the driver, and the panic drives the parent's strategy to `Stop` just as the
// original's `Stop` does. Merely returning after the stop would never reach the
// decider, so the panic is what makes the failure branch real.
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
      panic!("received Terminated for a reference the restarted actor did not watch");
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
  // children getter. `context.unwatch(child)` drops this actor's watch on the
  // child alone; routing it through `WeakActorSystem::unwatch` would re-borrow
  // the executor the hook is running inside, so it goes through this context.
  fn pre_restart(&mut self, context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    let child = self.child.clone().expect("pre_start created the grandchild");
    *self.shared.old_actor.lock().expect("the old actor slot is not poisoned") = Some(child.clone());
    context.unwatch(&child);
    context.stop(&child);
    self.child = None;
  }

  // Original: `postRestart` watches the colliding reference and pings itself.
  //
  // `stopped_noop_ref` refuses an actor that is still live, so a watch of a
  // still-live child is a test failure rather than a silent pass.
  fn post_restart(&mut self, context: &mut ActorContext<'_>) {
    let old = self.shared.old_actor.lock().expect("the old actor slot is not poisoned").clone();
    let old = old.expect("pre_restart saved the old child");
    let colliding = create_colliding_ref(context, &old);
    assert_eq!(colliding.path(), old.path(), "the colliding reference must carry the old child's full path");
    assert!(colliding == old, "the colliding reference must carry the old child's live UID");
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
