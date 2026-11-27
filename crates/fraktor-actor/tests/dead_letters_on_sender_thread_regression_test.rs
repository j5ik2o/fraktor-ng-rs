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

// Regression tests for issue #286: two shapes the sender-thread routing
// must keep.
//
// - A system that has terminated discards a refused envelope. A subscriber
//   of another system that was subscribed to the terminated system's event
//   stream receives nothing further.
// - An immediate DeathWatch notice for a watcher that has already stopped
//   is routed as the notice itself: a `Terminated` is a suppressed type in
//   the port's dead letter rules, so it surfaces as a
//   `SuppressedDeadLetter` carrying the notice, and the executor's pending
//   record is consumed. The internal carrier never surfaces as a payload.

use std::sync::Arc;
use std::sync::Mutex;
use std::vec::Vec;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::Actor;
use fraktor_actor::kernel::actor::ActorContext;
use fraktor_actor::kernel::actor::DeadLetter;
use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::actor::SuppressedDeadLetter;
use fraktor_actor::kernel::actor::Terminated;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::message::Message;

struct Sink;

impl Actor for Sink {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

// What a dead letter subscriber saw: the payload text of a plain message,
// or the target of a `Terminated` the dead letter carried.
#[derive(Debug, PartialEq, Eq)]
enum Seen {
  Text(&'static str),
  Notice(ActorRef),
  Other,
}

type Log = Arc<Mutex<Vec<Seen>>>;

fn subscribe_collector(system: &mut ActorSystem, stream: fraktor_actor::kernel::event::EventStream) -> Log {
  let log: Log = Arc::new(Mutex::new(Vec::new()));
  let sink = Arc::clone(&log);
  let collector = system.function_ref("dead-letter-collector", move |_: &ActorRef, message: Message| {
    let carried = message
      .downcast_ref::<DeadLetter>()
      .map(|letter| letter.message())
      .or_else(|| message.downcast_ref::<SuppressedDeadLetter>().map(|letter| letter.message()));
    if let Some(carried) = carried {
      let seen = if let Some(notice) = carried.downcast_ref::<Terminated>() {
        Seen::Notice(notice.target().clone())
      } else if let Some(text) = carried.downcast_ref::<&'static str>() {
        Seen::Text(text)
      } else {
        Seen::Other
      };
      sink.lock().unwrap().push(seen);
    }
  });
  stream.subscribe(Some(collector.clone()), EventClass::of::<DeadLetter>()).expect("a collector");
  stream.subscribe(Some(collector), EventClass::of::<SuppressedDeadLetter>()).expect("a collector");
  log
}

fn stopped_actor(system: &mut ActorSystem) -> ActorRef {
  let target = system.actor_of(Props::new(|| Sink));
  system.stop(&target);
  system.run_until_idle();
  assert!(system.is_actor_terminated(&target), "the actor has stopped");
  target
}

#[test]
fn a_tell_after_the_system_terminated_publishes_nothing() {
  let mut system_a = ActorSystem::new("RefusedRouteAfterTerminateA", Arc::new(ChannelBoundary::new()));
  let mut system_b = ActorSystem::new("RefusedRouteAfterTerminateB", Arc::new(ChannelBoundary::new()));
  let target = stopped_actor(&mut system_a);
  let sender = system_b.function_ref("sender", |_: &ActorRef, _: Message| {});
  let stream_a = system_a.event_stream();
  let log = subscribe_collector(&mut system_b, stream_a);

  target.tell("before-terminate", sender.clone());
  assert_eq!(
    log.lock().unwrap().as_slice(),
    [Seen::Text("before-terminate")],
    "the routing works while the system runs"
  );

  system_a.terminate();

  target.tell("after-terminate", sender);
  assert_eq!(
    log.lock().unwrap().as_slice(),
    [Seen::Text("before-terminate")],
    "a terminated system discards the refused envelope, as its closed executor queue did"
  );
}

#[test]
fn an_immediate_notice_for_a_stopped_watcher_is_routed_as_the_notice_itself() {
  let mut system = ActorSystem::new("RefusedRouteStoppedWatcher", Arc::new(ChannelBoundary::new()));
  let target = stopped_actor(&mut system);
  let watcher = stopped_actor(&mut system);
  let stream = system.event_stream();
  let log = subscribe_collector(&mut system, stream);

  // Both ends have already stopped, so `watch` answers at once with a
  // notice the stopped watcher's mailbox turns away.
  system.watch(&watcher, &target);
  system.run_until_idle();

  assert_eq!(
    log.lock().unwrap().as_slice(),
    [Seen::Notice(target.clone())],
    "the notice is routed as the Terminated itself, not the internal carrier"
  );

  // The pending record was consumed: asking again routes one more notice,
  // and nothing stale is routed a second time.
  system.watch(&watcher, &target);
  system.run_until_idle();
  assert_eq!(log.lock().unwrap().as_slice(), [Seen::Notice(target.clone()), Seen::Notice(target)]);
}
