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

// Stage 1 of #234 (issue #249): ActorSystem is made Send without changing
// behaviour.
//
// The compile-time checks below are the red of the stage: on the code before
// the stage they fail with E0277 (9 errors in the design's prototype,
// docs/design/issue-234-multithread-executor.md section 10, item 1), because
// the executor, the weak handle and the event stream setup sit behind
// Rc<RefCell<..>> and the actor traits carry no Send bound. After the stage
// they compile, and the existing tests must pass unchanged.
//
// The re-entry test is the runtime half of the stage (design review M3 and
// decision D5): calling back into the system from inside `receive` through a
// WeakActorSystem must return with a panic, as the RefCell double borrow did,
// and must not hang on the spin lock that replaces it.

use std::sync::Arc;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_system::{ActorSystem, WeakActorSystem};
use fraktor_actor::kernel::event::EventStream;
use fraktor_actor::kernel::message::Message;

fn assert_send<T: Send>() {}
fn assert_sync<T: Sync>() {}
fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn actor_system_handles_are_send_and_sync() {
  assert_send_sync::<ActorSystem>();
  assert_send_sync::<WeakActorSystem>();
  assert_send_sync::<EventStream>();
  // Kept from the design's prototype: these already passed before the stage.
  assert_send::<fraktor_actor::kernel::actor_ref::ActorRef>();
  assert_sync::<fraktor_actor::kernel::actor_ref::ActorRef>();
  assert_send_sync::<Props>();
}

/// An actor that calls back into its own system through the weak handle
/// while the executor is delivering a message to it.
struct Reentering {
  system: WeakActorSystem,
}

impl Actor for Reentering {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {
    // Any call that reaches the executor re-enters the lock the delivery
    // already holds, so this call must panic instead of returning or hanging.
    let _ = self.system.run_one();
  }
}

#[test]
#[should_panic(expected = "already borrowed")]
fn reentering_the_system_from_receive_panics_instead_of_hanging() {
  let mut system = ActorSystem::new("Stage1Reentry", Arc::new(ChannelBoundary::new()));
  let weak = system.downgrade();
  let actor = system.actor_of(Props::new(move || Reentering { system: weak.clone() }));
  let sender = system.dead_letters();
  actor.tell("go", sender);
  system.run_until_idle();
}
