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

// Reproduction for issue #74:
//   restarting the default loggers at a lower level leaves the subscriptions of
//   the higher levels in place.
//
// `EventStream::start_default_loggers` subscribes a logger to the channels of
// the accepted levels and never unsubscribes the channels the level drops.
// Starting the same logger at `Debug` and then restarting it at `Warning`
// therefore leaves `Info` and `Debug` subscribed, so the logger keeps receiving
// events the `Warning` setting should not accept. Pekko's
// `LoggingBus.startDefaultLoggers` is not called twice in practice, but the
// port exposes it as a public method and the contract the issue asks for is the
// one the ported spec's `setLogLevel` already keeps: after the restart the
// subscription state must be the one a logger started at that level would have.
//
// The tests use the same fixtures as `tests/event_stream_spec.rs`: a `MyLog`
// style logger that forwards every log event to a destination set through
// `SetTarget`, and an `EventStream` built directly on the system. The
// forwarding destination is the test kit's test actor, so a delivered event is
// observed through `expect_msg_type` and a missing one through the ordered
// expectations that follow it.

use core::time::Duration;

use fraktor_actor::kernel::actor::Logging::{
  Debug, Error, Info, InitializeLogger, LogEvent, LogLevel, LoggerInitialized, Warning,
};
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::event::{EventClass, EventStream};
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;

// Original: `EventStreamSpec.SetTarget(ref: ActorRef)`.
//
// The field is called `target` because `ref` is a Rust keyword.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SetTarget {
  target: ActorRef,
}

// Original: `EventStreamSpec.MyLog`. The logger answers `LoggerInitialized` when
// it is initialized, remembers the destination from `SetTarget`, and forwards
// every log event to that destination.
struct MyLog {
  destination: Option<ActorRef>,
}

impl MyLog {
  fn new() -> Self {
    Self { destination: None }
  }
}

impl Actor for MyLog {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(initialize) = message.downcast_ref::<InitializeLogger>() {
      let bus = initialize.bus();
      bus.subscribe(Some(context.self_ref()), EventClass::of::<SetTarget>()).expect("the logger is not null");
      let sender = context.sender();
      context.tell(&sender, LoggerInitialized);
      return;
    }
    if let Some(set_target) = message.downcast_ref::<SetTarget>() {
      self.destination = Some(set_target.target.clone());
      let target = set_target.target.clone();
      context.tell(&target, Message::new("OK"));
      return;
    }
    if message.is::<Debug>() || message.is::<Info>() || message.is::<Warning>() || message.is::<Error>() {
      if let Some(destination) = self.destination.clone() {
        context.tell(&destination, message);
      }
    }
  }
}

fn default_timeout() -> Duration {
  Duration::from_secs(3)
}

fn one_second() -> Duration {
  Duration::from_secs(1)
}

// Starts one logger at the given level, points it at the test actor, and
// returns the logger.
fn logger_forwarding_to_test_actor(spec: &mut FraktorSpec, bus: &EventStream, level: LogLevel) -> ActorRef {
  let logger = spec.system_mut().actor_of(Props::new(MyLog::new));
  bus.start_default_loggers(core::slice::from_ref(&logger), level);
  bus.publish(SetTarget { target: spec.test_actor() });
  spec.expect_msg("OK");
  logger
}

// The dropped levels must not reach the logger after the restart, while the
// accepted levels still do. The accepted `Warning` is published after the two
// dropped events, so the first message the test actor receives must be that
// `Warning`; a leftover `Info` or `Debug` subscription would have delivered one
// of them first, and the test would fail on the wrong type rather than only on
// an idle timeout.
#[test]
fn start_default_loggers_must_drop_the_channels_a_lower_restart_does_not_accept() {
  let mut spec = FraktorSpec::new("EventStreamLowerRestartDrops");
  let bus = EventStream::new(spec.system().downgrade(), false);
  let logger = logger_forwarding_to_test_actor(&mut spec, &bus, LogLevel::Debug);

  bus.start_default_loggers(&[logger], LogLevel::Warning);
  bus.publish(Info::new(String::new(), 0, "info"));
  bus.publish(Debug::new(String::new(), 0, "debug"));
  bus.publish(Warning::new(String::new(), 0, "warning"));

  spec.within(default_timeout(), |spec| {
    let received: Warning = spec.expect_msg_type();
    assert_eq!(received.message(), "warning");
    spec.test_kit_mut().expect_no_message(one_second());
  });
  spec.terminate();
}

// `Error` is more severe than `Warning`, so the restart must not drop it: the
// unsubscribe is limited to the channels the new level does not accept.
#[test]
fn start_default_loggers_must_keep_the_channels_a_lower_restart_accepts() {
  let mut spec = FraktorSpec::new("EventStreamLowerRestartKeeps");
  let bus = EventStream::new(spec.system().downgrade(), false);
  let logger = logger_forwarding_to_test_actor(&mut spec, &bus, LogLevel::Debug);

  bus.start_default_loggers(&[logger], LogLevel::Warning);
  bus.publish(Warning::new(String::new(), 0, "warning"));
  bus.publish(Error::new(String::new(), 0, "error"));

  spec.within(default_timeout(), |spec| {
    let warning: Warning = spec.expect_msg_type();
    assert_eq!(warning.message(), "warning");
    let error: Error = spec.expect_msg_type();
    assert_eq!(error.message(), "error");
  });
  spec.terminate();
}

// The subscription is keyed by logger and channel, so a restart at the same
// level must not subscribe the logger twice. The logger forwards each event it
// receives, so a duplicated subscription would make the single `Info` below
// reach the test actor twice; the test checks that exactly one arrives.
#[test]
fn start_default_loggers_must_not_duplicate_a_same_level_restart() {
  let mut spec = FraktorSpec::new("EventStreamSameLevelRestart");
  let bus = EventStream::new(spec.system().downgrade(), false);
  let logger = logger_forwarding_to_test_actor(&mut spec, &bus, LogLevel::Info);

  bus.start_default_loggers(&[logger], LogLevel::Info);
  bus.publish(Info::new(String::new(), 0, "info"));
  spec.within(default_timeout(), |spec| {
    let first: Info = spec.expect_msg_type();
    assert_eq!(first.message(), "info");
    spec.test_kit_mut().expect_no_message(one_second());
  });
  spec.terminate();
}
