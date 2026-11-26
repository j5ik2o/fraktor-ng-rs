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

//! Unit tests for the condvar wake signal.

use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use super::CondvarSignal;
use crate::kernel::dispatch::WakeSignal;

#[test]
fn a_notify_before_the_wait_is_not_lost() {
  let signal = CondvarSignal::new();
  signal.notify();

  let started = Instant::now();
  assert!(signal.wait(Duration::from_secs(5)));
  assert!(started.elapsed() < Duration::from_secs(1));
  assert!(!signal.wait(Duration::from_millis(1)), "a wait takes the flag down");
}

#[test]
fn a_notify_from_another_thread_ends_the_wait() {
  let signal = Arc::new(CondvarSignal::new());
  let notifier = Arc::clone(&signal);
  let started = Instant::now();
  let handle = thread::spawn(move || {
    thread::sleep(Duration::from_millis(50));
    notifier.notify();
  });

  assert!(signal.wait(Duration::from_secs(5)));
  assert!(started.elapsed() < Duration::from_secs(2), "the wait slept past the notify");
  handle.join().unwrap();
}

#[test]
fn a_wait_without_a_notify_times_out() {
  let signal = CondvarSignal::new();
  let started = Instant::now();

  assert!(!signal.wait(Duration::from_millis(20)));
  assert!(started.elapsed() >= Duration::from_millis(20));
}
