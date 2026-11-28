// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The stage 4b unit tests on which children a parent's restarts recreate,
//! and on the suspends those `Recreate`s answer (design of #234, sections
//! 11.13.3, 11.13.6 and 11.13.7, and 11.13.18 item 9).
//!
//! A submodule of `stage4b_tests`, whose actors, helpers and suspend read it
//! uses.

use super::*;

// ----- a second failure while failed (design section 11.13.3)

/// A parent that starts the children `c` and `d` in `pre_start`, keeps them
/// across its restarts, and applies to them a strategy whose decider and
/// child-terminated hook both panic.
struct FailsTwice {
  log: Log,
  kids: Arc<Mutex<Vec<ActorRef>>>,
}

impl Actor for FailsTwice {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    for name in ["c", "d"] {
      let kid = context.actor_of(worker(name, &self.log), name).expect("the parent takes its child");
      self.kids.lock().unwrap().push(kid);
    }
  }

  fn pre_restart(&mut self, _context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    self.log.lock().unwrap().push(String::from("p:pre_restart"));
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.log.lock().unwrap().push(String::from("p:post_restart"));
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    SupervisorStrategy::one_for_one(3)
      .with_decider(|_, _| panic!("simulated decider panic"))
      .with_child_terminated(|_context, _child, _siblings| panic!("simulated hook failure"))
  }
}

#[test]
fn a_second_failure_while_failed_suspends_the_first_perpetrator_too() {
  // Design 11.13.3 "失敗中にもう一度失敗したとき" and 11.13.18 item 9.2. Pekko
  // v2.0.0's `handleInvokeFailure` (`dungeon/FaultHandling.scala:216-225`)
  // leaves out of `suspendChildren` only the child it records as the
  // perpetrator in this very failure; a failure of an actor that has already
  // failed records nothing and suspends every child. `p` fails first on its
  // decider's panic over `Failed(c)`, which records `c` and leaves it out, and
  // then, still failed, on its child-terminated hook's panic over `d`, which
  // suspends `c` as well. So `c` holds two suspends, its own failure's and
  // `p`'s second one, and they pair with the two `Recreate`s `p`'s two restarts
  // send it (`:293-294`): `c` restarts twice, after `p`'s first and second
  // restart, and takes no normal message before its second restart. PR #300 at
  // `65c9ee1` leaves `c` out of the second suspend too, so the second
  // `Recreate` reaches `c` unsuspended and the debug assertion of
  // `fault_recreate` fails (the pre-merge reading of PR #300, item 2).
  let log = new_log();
  let kids = Arc::new(Mutex::new(Vec::new()));
  let mut system = with_catcher("Stage4bSecondFailureWhileFailed");
  let guardian = system.actor_of(parent("guardian", &log, SupervisorStrategy::default_strategy()));
  let failing = system.downgrade().spawn_child(
    &guardian,
    Props::new({
      let log = Arc::clone(&log);
      let kids = Arc::clone(&kids);
      move || FailsTwice { log: Arc::clone(&log), kids: Arc::clone(&kids) }
    }),
    Some("p"),
    SupervisorStrategy::default_strategy(),
  );
  system.run_until_idle();
  let (c, d) = {
    let kids = kids.lock().unwrap();
    assert_eq!(kids.len(), 2, "the parent did not start its two children");
    (kids[0].clone(), kids[1].clone())
  };
  log.lock().unwrap().clear();

  // `c` takes `boom` and fails, and its `Failed` waits at `p`; `m` stays in
  // `c`'s mailbox behind the failure. `d`'s end then reaches `p` behind
  // `Failed(c)`.
  c.tell("boom", guardian.clone());
  c.tell("m", guardian.clone());
  assert!(system.run_one(), "`c` did not take its `boom`");
  system.stop(&d);
  system.run_until_idle();
  c.tell("ping", guardian.clone());
  system.run_until_idle();

  let seen = entries(&log);
  for (text, times) in [("p:pre_restart", 2), ("p:post_restart", 2), ("c:pre_restart", 2), ("c:post_restart", 2)] {
    assert_eq!(counted(&log, text), times, "`{text}` is not recorded {times} times: {seen:?}");
  }
  let parent_restarts = positions(&seen, "p:post_restart");
  let child_restarts = positions(&seen, "c:pre_restart");
  for (parent_restart, child_restart) in parent_restarts.iter().zip(&child_restarts) {
    assert!(parent_restart < child_restart, "`c` restarted before the restart of `p` that sends it: {seen:?}");
  }
  let last_child_restart = positions(&seen, "c:post_restart")[1];
  assert!(last_child_restart < position(&seen, "c:m"), "`c` took a normal message before its second restart: {seen:?}");
  assert!(position(&seen, "c:m") < position(&seen, "c:ping"), "`c` did not take `m`, then `ping`: {seen:?}");
  assert_eq!(
    counts_of(&mut system, &[&guardian, &failing, &c]),
    alloc::vec![Some(0), Some(0), Some(0)],
    "a suspend was left unanswered: {seen:?}"
  );
}

// ----- a child created while the restart waits (design section 11.13.6)

/// A parent that starts `x` in `pre_start` and stops it in `pre_restart`, so
/// its restart waits for `x`. Before it stops `x`, `pre_restart` sends `tick`
/// to `marker`, which gives the test a normal message to stop `run_one` at
/// while the restart waits.
struct WaitsInItsRestart {
  log: Log,
  waited_for: Arc<Mutex<Option<ActorRef>>>,
  marker: ActorRef,
}

impl Actor for WaitsInItsRestart {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let x = context.actor_of(Props::new(|| Leaf), "x").expect("the parent takes `x`");
    *self.waited_for.lock().unwrap() = Some(x);
  }

  fn pre_restart(&mut self, context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    self.log.lock().unwrap().push(String::from("parent:pre_restart"));
    self.marker.tell("tick", context.self_ref());
    if let Some(x) = self.waited_for.lock().unwrap().take() {
      context.stop(&x);
    }
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.log.lock().unwrap().push(String::from("parent:post_restart"));
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("simulated failure of the waiting parent");
    }
  }
}

#[test]
fn a_child_created_while_the_restart_waits_is_restarted_with_the_survivors() {
  // Design 11.13.6, the second half's item 1 ("待つ間に作られた子は含む"), and
  // 11.13.18 item 9.4: Pekko v2.0.0's `finishRecreate` takes the survivors
  // when the wait has ended (`dungeon/FaultHandling.scala:280`), so a child
  // started under the parent while its restart waits for `x` is among them and
  // is restarted by the second half (`:293-294`). It was started under the
  // suspended parent, so that `Recreate` answers its inherited `Suspend`, and
  // it handles a normal message afterwards.
  let log = new_log();
  let waited_for = Arc::new(Mutex::new(None));
  let mut system = with_catcher("Stage4bCreatedWhileWaiting");
  let guardian = system.actor_of(parent("guardian", &log, SupervisorStrategy::default_strategy()));
  let marker = system.actor_of(worker("marker", &log));
  let failing = system.downgrade().spawn_child(
    &guardian,
    Props::new({
      let log = Arc::clone(&log);
      let waited_for = Arc::clone(&waited_for);
      move || WaitsInItsRestart { log: Arc::clone(&log), waited_for: Arc::clone(&waited_for), marker: marker.clone() }
    }),
    Some("parent"),
    SupervisorStrategy::one_for_one(3),
  );
  system.run_until_idle();
  log.lock().unwrap().clear();

  // The parent takes `boom` and fails. The next `run_one` runs the guardian's
  // decision and the parent's first half, which sends `tick` and then stops
  // `x`, and stops at `marker`'s `tick`, before `x` has ended: the restart
  // still waits when `y` is started under the parent.
  failing.tell("boom", guardian.clone());
  assert!(system.run_one(), "the parent did not take its `boom`");
  assert!(system.run_one(), "`marker` did not take `tick`");
  assert_eq!(entries(&log), ["parent:pre_restart", "marker:tick"], "the restart is not waiting for `x`");
  let y = spawn_inheriting(&mut system, &failing, worker("y", &log), "y");
  system.run_until_idle();
  let seen = entries(&log);
  assert_eq!(counted(&log, "parent:post_restart"), 1, "the parent did not restart: {seen:?}");
  assert_eq!(counted(&log, "y:pre_restart"), 1, "the second half did not restart `y`: {seen:?}");
  assert_eq!(counted(&log, "y:post_restart"), 1, "the second half did not restart `y`: {seen:?}");
  assert!(
    position(&seen, "parent:post_restart") < position(&seen, "y:pre_restart"),
    "`y` restarted before the parent's second half: {seen:?}"
  );
  assert_eq!(
    counts_of(&mut system, &[&failing, &y]),
    alloc::vec![Some(0), Some(0)],
    "the restart left the parent or `y` suspended: {seen:?}"
  );

  y.tell("ping", guardian.clone());
  system.run_until_idle();
  let seen = entries(&log);
  assert!(
    position(&seen, "y:post_restart") < position(&seen, "y:ping"),
    "`y` did not take `ping` after its restart: {seen:?}"
  );
}
