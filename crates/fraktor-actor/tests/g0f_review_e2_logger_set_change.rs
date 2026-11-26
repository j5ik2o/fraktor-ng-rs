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

// Reproduction for review thread E2:
//   replacing the logger set does not re-apply the subscription state.
//
// The bot example (CodeRabbit event_stream.rs:209) says: call
// `set_log_level(Debug)` before any logger starts, then start the default
// loggers at `Debug`. The first call saves `log_level = Debug` against an empty
// logger set, so the loggers started afterwards are subscribed to nothing; a
// later `set_log_level` finds `candidate > old` false for every level and adds
// no subscription either. Pekko's `startDefaultLoggers` subscribes each newly
// started logger to the channels of the configured level, so a logger started
// at `Debug` receives every level.
//
// This test checks only that claim: a logger started after the level was set
// receives a `Debug` event.

use core::time::Duration;

use fraktor_actor::kernel::actor::Logging::{Debug, InitializeLogger, LogLevel};
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::event::EventStream;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;

// A logger that forwards every `Debug` event it receives to a destination set
// at start.
struct ForwardingLogger {
  destination: ActorRef,
}

impl ForwardingLogger {
  fn new(destination: ActorRef) -> Self {
    Self { destination }
  }
}

impl Actor for ForwardingLogger {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.is::<InitializeLogger>() {
      context.tell(&context.self_ref(), fraktor_actor::kernel::actor::Logging::LoggerInitialized);
      return;
    }
    if message.is::<Debug>() {
      context.tell(&self.destination, message);
    }
  }
}

fn wait() -> Duration {
  Duration::from_secs(1)
}

#[test]
fn g0f_review_e2_logger_set_change_reapplies_the_subscription() {
  let mut spec = FraktorSpec::new("G0fReviewE2");
  let bus = EventStream::new(spec.system().downgrade(), false);

  // Save the level before any logger exists.
  bus.set_log_level(LogLevel::Debug);

  let destination = spec.test_actor();
  let logger = spec.system_mut().actor_of(Props::new(move || ForwardingLogger::new(destination.clone())));

  // Start a logger at the level that was already saved.
  bus.start_default_loggers(&[logger], LogLevel::Debug);

  // A later level change must not need to re-add the subscription the start
  // should already have applied; publish a `Debug` and expect the logger to
  // receive it.
  bus.publish(Debug::new(String::new(), 0, "trace"));

  spec.within(wait(), |spec| {
    spec.expect_msg_type::<Debug>();
  });
  spec.terminate();
}
