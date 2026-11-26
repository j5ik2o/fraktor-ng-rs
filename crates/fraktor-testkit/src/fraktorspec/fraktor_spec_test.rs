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

//! Unit tests for [`FraktorSpec`](super::FraktorSpec).

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use fraktor_actor::kernel::actor::Actor;
use fraktor_actor::kernel::actor::ActorContext;
use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::message::Message;

use super::FraktorSpec;
use crate::fraktorspec::FraktorSpecHooks;
use crate::test_kit::ReceiveWhile;
use crate::test_kit::TestKit;

struct RecordingHooks {
  events: Arc<Mutex<Vec<&'static str>>>,
}

impl FraktorSpecHooks for RecordingHooks {
  fn at_startup(&mut self, _test_kit: &mut TestKit) {
    self.events.lock().unwrap().push("at_startup");
  }

  fn before_termination(&mut self, test_kit: &mut TestKit) {
    assert!(!test_kit.system().is_terminated(), "before_termination runs while the system is running");
    self.events.lock().unwrap().push("before_termination");
  }

  fn after_termination(&mut self, test_kit: &mut TestKit) {
    assert!(test_kit.system().is_terminated(), "after_termination runs after the system stopped");
    self.events.lock().unwrap().push("after_termination");
  }
}

#[test]
fn with_hooks_runs_at_startup_once() {
  let events = Arc::new(Mutex::new(Vec::new()));
  let hooks = RecordingHooks { events: Arc::clone(&events) };

  let _spec = FraktorSpec::with_hooks("PekkoSpecSpec", Box::new(hooks));

  assert_eq!(*events.lock().unwrap(), vec!["at_startup"]);
}

#[test]
fn terminate_runs_the_hooks_in_order() {
  let events = Arc::new(Mutex::new(Vec::new()));
  let hooks = RecordingHooks { events: Arc::clone(&events) };
  let mut spec = FraktorSpec::with_hooks("PekkoSpecSpec", Box::new(hooks));

  spec.terminate();

  assert_eq!(*events.lock().unwrap(), vec!["at_startup", "before_termination", "after_termination"]);
}

#[test]
fn a_second_terminate_does_nothing() {
  let events = Arc::new(Mutex::new(Vec::new()));
  let hooks = RecordingHooks { events: Arc::clone(&events) };
  let mut spec = FraktorSpec::with_hooks("PekkoSpecSpec", Box::new(hooks));

  spec.terminate();
  spec.terminate();

  assert_eq!(events.lock().unwrap().len(), 3);
}

#[test]
fn new_creates_a_running_system() {
  let spec = FraktorSpec::new("PekkoSpecSpec");

  assert!(!spec.system().is_terminated());
  assert_eq!(spec.system().name(), "PekkoSpecSpec");
}

struct Idle;

impl Actor for Idle {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

// The capability that Pekko's "terminate all actors" (PekkoSpecSpec) uses:
// after the spec shuts its system down, `awaitCond` sees the test actor and
// the actors of the system as terminated.
#[test]
fn await_cond_sees_every_actor_terminated_after_terminate() {
  let mut spec = FraktorSpec::new("PekkoSpecSpec");
  let actor = spec.system_mut().actor_of(Props::new(|| Idle));
  let refs = [spec.test_actor(), actor];
  let system = spec.system().downgrade();
  assert!(refs.iter().all(|reference| !system.is_actor_terminated(reference)));

  spec.terminate();

  spec.await_cond(|| refs.iter().all(|reference| system.is_actor_terminated(reference)), Duration::from_secs(2));
}

#[test]
fn receive_while_reads_the_test_actor_of_the_spec() {
  let mut spec = FraktorSpec::new("PekkoSpecSpec");
  let test_actor = spec.test_actor();
  test_actor.tell(7usize, test_actor.clone());

  let received = spec.receive_while(ReceiveWhile::new().max(Duration::from_millis(200)).messages(1), |message| {
    message.downcast_ref::<usize>().copied()
  });

  assert_eq!(received, vec![7]);
}
