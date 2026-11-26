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

//! Tests of the test handle itself: the timer driver ends when the
//! machine it watches stops for good, not only when the handle is
//! dropped.

use core::time::Duration;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::{ActorContext, Fsm, FsmState, FsmView};
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;

use super::TestFsmRef;

fn timer_fsm() -> Fsm<i32, ()> {
  let mut fsm = Fsm::new();
  fsm.start_with(1, ());
  fsm.when(1, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, _event: &Message, _data: &()| {
    Some(FsmState::stay())
  });
  fsm
}

#[test]
fn the_timer_driver_ends_when_the_machine_stops_for_good() {
  let mut system = ActorSystem::new("TestFsmRefDriverStop", Arc::new(ChannelBoundary::new()));
  let fsm = TestFsmRef::new(&mut system, timer_fsm);
  let target = fsm.actor_ref();
  system.stop(&target);
  let deadline = std::time::Instant::now() + Duration::from_secs(3);
  while !fsm.stop_driver.load(Ordering::Relaxed) {
    assert!(std::time::Instant::now() < deadline, "the timer driver kept running after the machine stopped for good");
    system.downgrade().run_until_idle();
    std::thread::sleep(Duration::from_millis(5));
  }
  system.terminate();
}
