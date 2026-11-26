// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/util/SwitchSpec.scala
//   (the implementation is `class Switch` in util/LockUtil.scala)
//
// The original's six cases are kept as one test each. In the original,
// `s.switchOn(())` resolves to the one-argument action form (transcend), and
// no case calls the no-argument form, so state changes here go through
// `switch_on_with` / `switch_off_with` with an action that succeeds (design
// of #186, docs/design/issue-186-util-specs.md §1.3). The exception case
// becomes an action that returns `Err`. The locking case drops the original
// `Thread.sleep(500)`, which runs while no other thread exists, and replaces
// the latch with `join()`.

use core::convert::Infallible;

use fraktor_actor::kernel::switch::Switch;

#[derive(Debug, PartialEq, Eq)]
struct Boom;

fn on(s: &Switch) -> bool {
  s.switch_on_with(|| Ok::<(), Infallible>(())).expect("infallible action")
}

fn off(s: &Switch) -> bool {
  s.switch_off_with(|| Ok::<(), Infallible>(())).expect("infallible action")
}

#[test]
fn switch_turns_on_and_off() {
  let s = Switch::new(false);
  assert!(s.is_off());
  assert!(!s.is_on());

  assert!(on(&s));
  assert!(s.is_on());
  assert!(!s.is_off());
  assert!(!on(&s));
  assert!(s.is_on());
  assert!(!s.is_off());

  assert!(off(&s));
  assert!(s.is_off());
  assert!(!s.is_on());
  assert!(!off(&s));
  assert!(s.is_off());
  assert!(!s.is_on());
}

#[test]
fn switch_reverts_when_action_fails() {
  let s = Switch::new(false);
  let result = s.switch_on_with(|| Err::<(), Boom>(Boom));
  assert_eq!(result, Err(Boom));
  assert!(s.is_off());
}

#[test]
fn switch_runs_action_without_locking() {
  let s = Switch::new(false);
  assert_eq!(s.if_off_yield(|| "yes"), Some("yes"));
  assert_eq!(s.if_on_yield(|| "no"), None);
  assert!(s.if_off(|| {}));
  assert!(!s.if_on(|| {}));

  on(&s);
  assert_eq!(s.if_on_yield(|| "yes"), Some("yes"));
  assert_eq!(s.if_off_yield(|| "no"), None);
  assert!(s.if_on(|| {}));
  assert!(!s.if_off(|| {}));
}

#[test]
fn switch_runs_action_with_locking() {
  let s = Switch::new(false);
  assert_eq!(s.while_off_yield(|| "yes"), Some("yes"));
  assert_eq!(s.while_on_yield(|| "no"), None);
  assert!(s.while_off(|| {}));
  assert!(!s.while_on(|| {}));

  on(&s);
  assert_eq!(s.while_on_yield(|| "yes"), Some("yes"));
  assert_eq!(s.while_off_yield(|| "no"), None);
  assert!(s.while_on(|| {}));
  assert!(!s.while_off(|| {}));
}

#[test]
fn switch_folds_by_state() {
  let s = Switch::new(false);
  assert_eq!(s.fold(|| "on", || "off"), "off");
  on(&s);
  assert_eq!(s.fold(|| "on", || "off"), "on");
}

#[test]
fn switch_locked_allows_reentry_and_then_other_thread_changes() {
  let s = Switch::new(false);

  s.locked(|held| {
    assert_eq!(held.switch_on_with(|| Ok::<(), Infallible>(())), Ok(true));
    assert!(held.is_on());
  });
  assert!(s.is_on());

  let handle = std::thread::spawn(move || {
    assert!(off(&s));
  });
  // The handle owns the switch now; recover it through the join result is
  // not possible, so the state is checked by the spawned thread itself.
  handle.join().expect("the other thread finishes");
}
