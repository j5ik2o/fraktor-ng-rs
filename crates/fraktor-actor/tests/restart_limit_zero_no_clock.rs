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

// Regression test for the finite-window one-for-one strategy over a custom
// shard boundary that supplies no monotonic clock. A restart limit of zero
// stops the failed child at once, as `AllForOneStrategy.processFailure` does,
// and must not read the clock: the custom boundary contract lets a boundary
// without a clock start actors whose windowed strategy still stops on a zero
// limit. The boundary below panics if its clock is ever read, so a granted
// restart that consults the clock would fail the test instead of stopping.

use core::time::Duration;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use fraktor_actor::adaptor::{ChannelBoundary, UnwindCatcher};
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::executor::ShardBoundary;
use fraktor_actor::kernel::message::{Envelope, Message};
use fraktor_actor::kernel::supervision::{Directive, FailureKind, RestartLimit, RestartWindow, SupervisorStrategy};

const SPEC_SYSTEM_NAME: &str = "RestartLimitZeroNoClock";
const EMPLOYEE_NAME: &str = "employee";

fn default_timeout() -> Duration {
  Duration::from_secs(3)
}

// A boundary that carries envelopes like the standard channel boundary but
// refuses to report time. Reading its clock is a contract violation for a zero
// restart limit, so it panics and counts the read.
struct ClocklessBoundary {
  transport: ChannelBoundary,
  clock_reads: Arc<AtomicUsize>,
}

impl ShardBoundary for ClocklessBoundary {
  fn monotonic_time(&self) -> Option<Duration> {
    self.clock_reads.fetch_add(1, Ordering::SeqCst);
    panic!("a zero restart limit must not read the monotonic clock");
  }

  fn post(&self, envelope: Envelope) {
    self.transport.post(envelope);
  }

  fn poll(&self) -> Option<Envelope> {
    self.transport.poll()
  }

  fn poll_within(&self, max: Duration) -> Option<Envelope> {
    self.transport.poll_within(max)
  }

  fn has_pending(&self) -> bool {
    self.transport.has_pending()
  }

  fn close(&self) {
    self.transport.close();
  }
}

fn restart_on_throwable(_child: &ActorRef, _failure: FailureKind) -> Directive {
  Directive::Restart
}

// Answers a `Props` with a single named employee it starts under its strategy,
// as the ported `Supervisor` helper does.
struct Supervisor {
  strategy: SupervisorStrategy,
}

impl Actor for Supervisor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(props) = message.downcast_ref::<Props>() else {
      return;
    };
    let employee = context.actor_of(props.clone(), EMPLOYEE_NAME).expect("the supervisor starts its employee");
    context.sender().tell(employee, context.self_ref());
  }

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    self.strategy.clone()
  }
}

impl Actor for CrashingEmployee {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {
    panic!("Crashing...");
  }
}

struct CrashingEmployee;

fn employee_under(boss: &ActorRef, make: impl Fn() -> CrashingEmployee + Send + Sync + 'static) -> ActorRef {
  let props = Props::new(make);
  boss.ask(props).await_result(default_timeout()).downcast_ref::<ActorRef>().cloned().expect("the employee handle")
}

#[test]
fn a_zero_limit_finite_window_stops_without_reading_the_clock() {
  let clock_reads = Arc::new(AtomicUsize::new(0));
  let boundary =
    Arc::new(ClocklessBoundary { transport: ChannelBoundary::new(), clock_reads: Arc::clone(&clock_reads) });
  let mut system = ActorSystem::with_failure_catcher(SPEC_SYSTEM_NAME, boundary, Arc::new(UnwindCatcher));
  fraktor_actor::adaptor::attach_wait_driver(&system);

  let strategy =
    SupervisorStrategy::one_for_one_with_window(RestartLimit::new(0), RestartWindow::Within(Duration::from_secs(1)))
      .with_decider(restart_on_throwable);
  let boss = system.actor_of(Props::new(move || Supervisor { strategy: strategy.clone() }));
  let employee = employee_under(&boss, || CrashingEmployee);

  // Crash the employee; the supervisor decides `Restart`, the zero limit denies
  // it, and the employee stops. The clock must never be read along the way.
  employee.tell("crash", system.dead_letters());
  system.run_until_idle();

  assert_eq!(clock_reads.load(Ordering::SeqCst), 0, "a zero restart limit must not read the monotonic clock");
  assert!(system.is_actor_terminated(&employee), "the employee stops when the restart limit is zero");
  system.terminate();
}
