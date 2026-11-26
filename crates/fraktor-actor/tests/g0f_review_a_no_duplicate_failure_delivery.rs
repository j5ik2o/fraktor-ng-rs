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

// Reproduction companion for review thread A:
//   joining the EventSink and EventStream paths must not deliver the same
//   failure log twice.
//
// The reproduction test `g0f_review_a_system_failure_log` checks only that the
// failure `Error` reaches the event stream. It does not check how many times the
// error reaches a stream subscriber after the port joins the two paths. Pekko
// publishes the failure to the event stream once (`FaultHandling.publish` calls
// `context.system.eventStream.publish`), so a subscriber on the stream sees it
// exactly once.
//
// A naive join that publishes the failure to the sink and, independently, to the
// event stream still makes the reproduction test green, but a subscriber that
// sits on the stream receives the error twice. This test rejects that wrong
// implementation: a probe subscribed to `Error` on `system.event_stream()` sees
// the failure once, followed by a sentinel published after it, and nothing else.

use core::time::Duration;

use fraktor_actor::kernel::actor::Logging::Error;
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::implicit_sender::ImplicitSender;

// The actor whose failure the parent logs as an `Error`.
struct IgnoringActor;

impl Actor for IgnoringActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

// A distinct event published after the failure, so the test can tell the first
// delivered message from the next one without waiting on a timeout.
#[derive(Clone, Copy)]
struct AfterFailure;

fn fault_wait() -> Duration {
  Duration::from_secs(3)
}

#[test]
fn g0f_review_a_failure_reaches_the_stream_exactly_once() {
  let mut spec = FraktorSpec::new("G0fReviewANoDuplicate");
  let stream = spec.system().event_stream();

  // Subscribe the test actor to `Error` (the failure class) and to
  // `AfterFailure` (the sentinel), both on the system's own event stream.
  assert!(stream.subscribe(Some(spec.test_actor()), EventClass::of::<Error>()).expect("the test actor is not null"));
  assert!(stream
    .subscribe(Some(spec.test_actor()), EventClass::of::<AfterFailure>())
    .expect("the test actor is not null"));

  let victim = spec.system_mut().actor_of(Props::new(|| IgnoringActor));
  let sender = ImplicitSender::self_actor_ref(&spec);
  victim.tell("Kill", sender);
  spec.system_mut().run_until_idle();

  // A sentinel published after the failure; the stream must deliver it once.
  stream.publish(AfterFailure);

  // Take exactly two messages under one deadline: the failure `Error` and the
  // sentinel. A pathological delivery of the failure twice to the stream would
  // push the sentinel to third, so the sentinel would not be found here.
  let messages = spec.test_kit_mut().receive_n(2, fault_wait());
  assert_eq!(messages.len(), 2);

  let mut errors = 0;
  let mut sentinels = 0;
  for received in &messages {
    if received.downcast_ref::<Error>().is_some() {
      errors += 1;
    } else if received.downcast_ref::<AfterFailure>().is_some() {
      sentinels += 1;
    }
  }
  assert_eq!(errors, 1, "the failure error reached the stream subscriber more than once");
  assert_eq!(sentinels, 1, "the sentinel published after the failure was not delivered");

  spec.terminate();
}
