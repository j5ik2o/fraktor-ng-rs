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
// Licensed under the Apache License, version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

use std::sync::Mutex;
use std::vec::Vec;

use crate::adaptor::ChannelBoundary;
use crate::kernel::actor::Actor;
use crate::kernel::actor::DeadLetter;
use crate::kernel::actor::DeadLettersShared;
use crate::kernel::actor::Props;
use crate::kernel::actor::SuppressedDeadLetter;
use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::dispatch::SystemMessage;
use crate::kernel::event::EventClass;
use crate::kernel::event::EventStreamBus;
use crate::kernel::event::EventStreamBusWeak;
use crate::kernel::executor::Executor;
use crate::kernel::message::Message;
use crate::kernel::sync::Arc;

/// The kind of a system message, without the handles inside it, so the
/// comparisons below do not need `SystemMessage: PartialEq` (which the enum
/// does not derive).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
  Create,
  Suspend,
  Resume,
  Recreate,
  Failed,
  Terminate,
  ChildTerminated,
}

fn kind_of(message: &SystemMessage) -> Kind {
  match message {
    | SystemMessage::Create => Kind::Create,
    | SystemMessage::Suspend => Kind::Suspend,
    | SystemMessage::Resume { .. } => Kind::Resume,
    | SystemMessage::Recreate { .. } => Kind::Recreate,
    | SystemMessage::Failed(_) => Kind::Failed,
    | SystemMessage::Terminate => Kind::Terminate,
    | SystemMessage::ChildTerminated { .. } => Kind::ChildTerminated,
  }
}

/// One report that reached dead letters, read back as what it carries.
#[derive(Clone, Debug, PartialEq)]
enum Routed {
  Dead(Kind, ActorRef, ActorRef),
  Suppressed(Kind, ActorRef, ActorRef),
}

fn read_report(message: &Message) -> Option<Routed> {
  if let Some(dead) = message.downcast_ref::<DeadLetter>() {
    return dead
      .message()
      .downcast_ref::<SystemMessage>()
      .map(|carried| Routed::Dead(kind_of(carried), dead.sender().clone(), dead.recipient().clone()));
  }
  if let Some(dead) = message.downcast_ref::<SuppressedDeadLetter>() {
    return dead
      .message()
      .downcast_ref::<SystemMessage>()
      .map(|carried| Routed::Suppressed(kind_of(carried), dead.sender().clone(), dead.recipient().clone()));
  }
  None
}

struct Sink;

impl Actor for Sink {
  fn receive(&mut self, _context: &mut crate::kernel::actor::ActorContext<'_>, _message: Message) {}
}

fn new_executor() -> Executor {
  let address = crate::kernel::address::Address::new("pekko", "DeadLettersSharedTest");
  let root = crate::kernel::actor_path::RootActorPath::new(address);
  Executor::new(ActorPath::from_root(root), Arc::new(ChannelBoundary::new()))
}

/// Builds the shared state wired to a fresh bus, and subscribes a function
/// reference to `any()` that records each report as the system message it
/// carries. The bus is returned so the weak handle the shared state keeps can
/// upgrade for the life of the test.
fn shared_with_reports(executor: &mut Executor) -> (Arc<DeadLettersShared>, EventStreamBus, Arc<Mutex<Vec<Routed>>>) {
  let shared = Arc::new(DeadLettersShared::new());
  let bus = EventStreamBus::new();
  shared.set_bus(bus.downgrade());
  let delivered: Arc<Mutex<Vec<Routed>>> = Arc::new(Mutex::new(Vec::new()));
  let inbox = delivered.clone();
  let collector = executor.function_ref("systemMessageSharedCollector", move |_: &ActorRef, message: Message| {
    if let Some(report) = read_report(&message) {
      inbox.lock().unwrap().push(report);
    }
  });
  bus.subscribe(Some(collector), EventClass::any()).expect("the collector");
  (shared, bus, delivered)
}

// `Resume` for a closed mailbox is `DeadLetter(Resume, target, target)`.
#[test]
fn a_resume_is_a_dead_letter_naming_the_owner_on_both_ends() {
  let mut executor = new_executor();
  let (shared, _bus, reports) = shared_with_reports(&mut executor);
  let target = executor.try_spawn_user(Props::new(|| Sink), None).unwrap();
  let dead_letters = executor.dead_letters();

  shared.route_system_undeliverable(SystemMessage::Resume { caused_by_failure: false }, target.clone(), &dead_letters);

  assert_eq!(
    *reports.lock().unwrap(),
    std::vec![Routed::Dead(Kind::Resume, target.clone(), target)],
    "a Resume is a DeadLetter whose sender and recipient are the closed mailbox's owner"
  );
}

// `Create` for a closed mailbox is `DeadLetter(Create, target, target)`.
#[test]
fn a_create_is_a_dead_letter_naming_the_owner_on_both_ends() {
  let mut executor = new_executor();
  let (shared, _bus, reports) = shared_with_reports(&mut executor);
  let target = executor.try_spawn_user(Props::new(|| Sink), None).unwrap();
  let dead_letters = executor.dead_letters();

  shared.route_system_undeliverable(SystemMessage::Create, target.clone(), &dead_letters);

  assert_eq!(
    *reports.lock().unwrap(),
    std::vec![Routed::Dead(Kind::Create, target.clone(), target)],
    "a Create is a DeadLetter whose sender and recipient are the closed mailbox's owner"
  );
}

// `Terminate` and `ChildTerminated` are suppressed, naming the dead letters
// handle as the reference that processed them.
#[test]
fn terminate_and_child_terminated_are_suppressed() {
  let mut executor = new_executor();
  let (shared, _bus, reports) = shared_with_reports(&mut executor);
  let target = executor.try_spawn_user(Props::new(|| Sink), None).unwrap();
  let child = executor.try_spawn_user(Props::new(|| Sink), None).unwrap();
  let dead_letters = executor.dead_letters();

  shared.route_system_undeliverable(SystemMessage::Terminate, target.clone(), &dead_letters);
  shared.route_system_undeliverable(
    SystemMessage::ChildTerminated { child: child.clone() },
    target.clone(),
    &dead_letters,
  );

  assert_eq!(
    *reports.lock().unwrap(),
    std::vec![
      Routed::Suppressed(Kind::Terminate, target.clone(), dead_letters.clone()),
      Routed::Suppressed(Kind::ChildTerminated, target, dead_letters),
    ],
    "a Terminate and a ChildTerminated are suppressed, naming the dead letters handle"
  );
}

// Keeps `EventStreamBusWeak` in use for the module above when it is compiled
// again (it is the weak handle type the shared state holds); remove this and
// the import once a test reads the handle directly.
#[allow(dead_code)]
fn _keep_imports(_: Option<EventStreamBusWeak>) {}
