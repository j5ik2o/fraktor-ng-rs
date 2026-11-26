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

// Issue #286: a message that a closed mailbox turns away goes to dead letters
// on the sender's thread, before `tell` returns.
//
// In Pekko 2.0.0 a stopped actor's mailbox is the dead letter mailbox, whose
// `enqueue` sends `DeadLetter(message, sender, receiver)` to `deadLetters`
// right there (`dispatch/Mailboxes.scala`, lines 54 to 63). The dead letters
// handle publishes it to the event stream, and a subscriber that runs where it
// is told (a function reference, as the test actor on the calling-thread
// dispatcher) sees it on the sender's thread, before `tell` returns.
//
// Before #286 the port handed the message to the executor, which routed it at
// `run_until_idle`. None of the cases below drives the executor between the
// send and the check.

use std::string::String;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::thread::ThreadId;
use std::vec::Vec;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::Actor;
use fraktor_actor::kernel::actor::ActorContext;
use fraktor_actor::kernel::actor::DeadLetter;
use fraktor_actor::kernel::actor::DeadLetterSuppression;
use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::actor::SuppressedDeadLetter;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::message::Message;

struct Sink;

impl Actor for Sink {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Suppressed;

impl DeadLetterSuppression for Suppressed {}

// What a subscriber saw: the kind of event, the payload, the sender, the
// recipient, and the thread the subscriber ran on.
#[derive(Debug)]
struct Seen {
  kind: &'static str,
  payload: String,
  sender: ActorRef,
  recipient: ActorRef,
  thread: ThreadId,
}

type Log = Arc<Mutex<Vec<Seen>>>;

fn payload_of(message: &Message) -> String {
  if let Some(text) = message.downcast_ref::<&'static str>() {
    return String::from(*text);
  }
  if message.downcast_ref::<Suppressed>().is_some() {
    return String::from("Suppressed");
  }
  String::from("<other>")
}

// Subscribes a function reference to `DeadLetter` and `SuppressedDeadLetter`.
// Posting to a function reference runs its handler before the sender returns.
fn subscribe_collector(system: &mut ActorSystem) -> Log {
  let log: Log = Arc::new(Mutex::new(Vec::new()));
  let sink = Arc::clone(&log);
  let collector = system.function_ref("dead-letter-collector", move |_: &ActorRef, message: Message| {
    let seen = if let Some(letter) = message.downcast_ref::<DeadLetter>() {
      Seen {
        kind: "DeadLetter",
        payload: payload_of(letter.message()),
        sender: letter.sender().clone(),
        recipient: letter.recipient().clone(),
        thread: thread::current().id(),
      }
    } else if let Some(letter) = message.downcast_ref::<SuppressedDeadLetter>() {
      Seen {
        kind: "SuppressedDeadLetter",
        payload: payload_of(letter.message()),
        sender: letter.sender().clone(),
        recipient: letter.recipient().clone(),
        thread: thread::current().id(),
      }
    } else {
      return;
    };
    sink.lock().unwrap().push(seen);
  });
  let stream = system.event_stream();
  stream.subscribe(Some(collector.clone()), EventClass::of::<DeadLetter>()).expect("a collector");
  stream.subscribe(Some(collector), EventClass::of::<SuppressedDeadLetter>()).expect("a collector");
  log
}

// Starts an actor and stops it, so its mailbox is closed.
fn stopped_actor(system: &mut ActorSystem) -> ActorRef {
  let target = system.actor_of(Props::new(|| Sink));
  system.stop(&target);
  system.run_until_idle();
  assert!(system.is_actor_terminated(&target), "the actor has stopped");
  target
}

#[test]
fn a_message_to_a_stopped_actor_reaches_the_dead_letter_subscriber_before_tell_returns() {
  let mut system = ActorSystem::new("DeadLettersOnSenderSameThread", Arc::new(ChannelBoundary::new()));
  let target = stopped_actor(&mut system);
  let sender = system.function_ref("sender", |_: &ActorRef, _: Message| {});
  let log = subscribe_collector(&mut system);

  target.tell("lost", sender.clone());

  // No `run_until_idle` or `run_one` between the send and this check.
  let seen = log.lock().unwrap();
  assert_eq!(seen.len(), 1, "the dead letter is published before tell returns: {seen:?}");
  assert_eq!(seen[0].kind, "DeadLetter");
  assert_eq!(seen[0].payload, "lost");
  assert_eq!(seen[0].sender, sender, "the sender is kept");
  assert_eq!(seen[0].recipient, target, "the recipient is the stopped actor");
  assert_eq!(seen[0].thread, thread::current().id(), "the subscriber runs on the sender's thread");
}

#[test]
fn a_suppressed_message_to_a_stopped_actor_is_published_before_tell_returns() {
  let mut system = ActorSystem::new("DeadLettersOnSenderSuppressed", Arc::new(ChannelBoundary::new()));
  system.register_dead_letter_suppression::<Suppressed>();
  let target = stopped_actor(&mut system);
  let sender = system.function_ref("sender", |_: &ActorRef, _: Message| {});
  let dead_letters = system.dead_letters();
  let log = subscribe_collector(&mut system);

  target.tell(Suppressed, sender.clone());

  let seen = log.lock().unwrap();
  assert_eq!(seen.len(), 1, "the suppressed dead letter is published before tell returns: {seen:?}");
  assert_eq!(seen[0].kind, "SuppressedDeadLetter");
  assert_eq!(seen[0].payload, "Suppressed");
  assert_eq!(seen[0].sender, sender, "the sender is kept");
  assert_eq!(seen[0].recipient, dead_letters, "the recipient is the dead letters handle");
  assert_eq!(seen[0].thread, thread::current().id(), "the subscriber runs on the sender's thread");
}

#[test]
fn a_message_sent_from_another_thread_reaches_the_subscriber_on_that_thread() {
  let mut system = ActorSystem::new("DeadLettersOnSenderOtherThread", Arc::new(ChannelBoundary::new()));
  let target = stopped_actor(&mut system);
  let sender = system.function_ref("sender", |_: &ActorRef, _: Message| {});
  let log = subscribe_collector(&mut system);

  let (sending_thread, seen_when_tell_returned) = {
    let target = target.clone();
    let sender = sender.clone();
    let log = Arc::clone(&log);
    thread::spawn(move || {
      target.tell("from-another-thread", sender);
      let seen: Vec<(String, ThreadId)> =
        log.lock().unwrap().iter().map(|seen| (seen.payload.clone(), seen.thread)).collect();
      (thread::current().id(), seen)
    })
    .join()
    .expect("the sending thread")
  };

  // The sending thread looked before anything drove the executor.
  assert_eq!(
    seen_when_tell_returned,
    std::vec![(String::from("from-another-thread"), sending_thread)],
    "the subscriber ran on the sending thread before tell returned"
  );
  assert_ne!(sending_thread, thread::current().id());
  let seen = log.lock().unwrap();
  assert_eq!(seen.len(), 1, "published once: {seen:?}");
  assert_eq!(seen[0].sender, sender, "the sender is kept");
  assert_eq!(seen[0].recipient, target, "the recipient is the stopped actor");
}
