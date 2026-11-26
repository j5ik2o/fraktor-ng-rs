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

// Tests of the switch outside the original SwitchSpec (design of #186,
// docs/design/issue-186-util-specs.md §2.3 and §3): the no-argument forms
// (the original's cases never call them), the start-on construction, the
// revert on panic (the original catches every `Throwable`; the port's drop
// guard reverts on unwind as well as on `Err`), and S5, the property that a
// change from another thread does not complete while `locked` is held. The
// S5 test cannot fail on a correct implementation, but whether it catches a
// broken one depends on timing, as the design records. These are not ports
// of Pekko cases and are not counted in the ledger.

use core::convert::Infallible;
use std::{
  panic::{self, AssertUnwindSafe},
  sync::mpsc,
  time::Duration,
};

use fraktor_actor::kernel::switch::Switch;

#[test]
fn switch_no_argument_forms_change_state_once() {
  let s = Switch::new(false);
  assert!(s.switch_on());
  assert!(!s.switch_on());
  assert!(s.is_on());
  assert!(s.switch_off());
  assert!(!s.switch_off());
  assert!(s.is_off());
}

#[test]
fn switch_starts_on_when_asked() {
  let s = Switch::new(true);
  assert!(s.is_on());
  assert!(!s.is_off());
}

#[test]
fn switch_reverts_when_action_panics() {
  let s = Switch::new(false);
  let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
    let _ = s.switch_on_with(|| -> Result<(), Infallible> { panic!("action failed") });
  }));
  assert!(outcome.is_err(), "the panic itself propagates");
  assert!(s.is_off(), "the drop guard reverted the state during unwind");
  // The lock was released as well: a later change goes through.
  assert!(s.switch_on());
  assert!(s.is_on());
}

#[test]
fn switch_reverts_when_action_inside_locked_panics() {
  let s = Switch::new(false);
  let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
    s.locked(|held| {
      let _ = held.switch_on_with(|| -> Result<(), Infallible> { panic!("action failed") });
    });
  }));
  assert!(outcome.is_err());
  assert!(s.is_off());
  assert!(s.switch_on());
}

#[test]
fn switch_change_from_other_thread_does_not_complete_while_locked_is_held() {
  let s = Switch::new(false);
  let s = &s;
  let (held_tx, held_rx) = mpsc::channel::<()>();
  let (release_tx, release_rx) = mpsc::channel::<()>();
  let (done_tx, done_rx) = mpsc::channel::<bool>();

  std::thread::scope(|scope| {
    let holder = scope.spawn(move || {
      s.locked(move |_held| {
        held_tx.send(()).expect("report that the lock is held");
        release_rx.recv().expect("wait for the release signal");
      });
    });
    held_rx.recv().expect("the holder took the lock");

    let switch_ref = &s;
    let changer = scope.spawn(move || {
      let changed = switch_ref.switch_off();
      done_tx.send(changed).expect("report completion");
    });

    // While the lock is held, the other thread's change must not complete.
    // A correct implementation never completes here; catching a broken one
    // depends on the changer being scheduled within the window. The result
    // is recorded first and asserted only after both threads are joined,
    // so a broken implementation fails this test instead of leaving the
    // holder waiting for a release that never comes.
    let early = done_rx.recv_timeout(Duration::from_millis(300)).ok();
    let completed_while_held = early.is_some();

    release_tx.send(()).expect("release the holder");
    holder.join().expect("the holder finishes");
    // The switch was off the whole time, so the change reports false once
    // it finally runs (or already reported, if it wrongly completed).
    let changed = match early {
      | Some(value) => value,
      | None => done_rx.recv().expect("the changer completes"),
    };
    changer.join().expect("the changer finishes");

    assert!(!completed_while_held, "the change completed while `locked` was held");
    assert!(!changed, "the change reports false: the switch was off");
  });
}
