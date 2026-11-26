// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2014-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/actor/PropsCreationSpec.scala
//
// The original's five cases are kept as one test each, named after the case,
// with the original name as a comment.
//
// The original API and its Rust counterpart:
//
// - `Props(new OneParamActor(null))` and `Props(classOf[OneParamActor], null)`
//   build the actor from a class and constructor arguments the runtime finds
//   by reflection. The port hands `Props::new` a closure that captures the
//   argument instead: the property under test is that an actor taking an
//   argument can be created with that argument, not the reflection itself.
//   In Rust the cases 1 to 4 therefore take the same shape; the ledger
//   records that sameness. Cases 2 and 3 have identical bodies in the
//   original as well (both are `Props(classOf[OneParamActor], null)`).
// - The original's `null` argument is `Option::None` captured by the closure.
//   A mismatch of argument type or count is a compile error in the port.
// - `Props.create(classOf[Actor], OneParamActorCreator)` (the Java API) is a
//   struct with a `create()` method whose return value the closure returns.
//   No Java interface is built.
// - The original only checks that `system.actorOf(p)` returns without
//   throwing. The port also observes the argument the factory captured: the
//   factory records it in a shared log when it runs, and the test reads the
//   log after `actor_of` and `run_until_idle`.
// - The original runs with `pekko.actor.serialize-creators = on`, under
//   which `actorOf` verifies on the spot that the Props arguments and the
//   creator can be serialized, and the runtime constructor lookup is part of
//   cases 2 to 5. Both are specific to the JVM, so the port does not carry
//   them. Whether that omission is accepted is awaiting the owner's decision
//   (marshal's item 441, question 2); nothing here records it as approved.

use std::sync::{Arc, Mutex};

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;

/// The original's marker class `A`: a value with no content.
#[derive(Clone, Debug, PartialEq)]
struct A;

/// The original's marker class `B`: a value with no content.
#[derive(Clone, Debug, PartialEq)]
struct B;

/// What a factory observed when it ran: whether the captured argument of a
/// `OneParamActor` was present, as a `(one, two)` pair of the same for a
/// `TwoParamActor`.
#[derive(Clone, Debug, PartialEq)]
enum Observed {
  One(bool),
  Two(bool, bool),
}

/// The original's `OneParamActor`: takes one argument and handles no message
/// (`Actor.emptyBehavior`, which reports each message as unhandled).
struct OneParamActor {
  #[allow(dead_code)]
  param: Option<A>,
}

impl OneParamActor {
  fn new(param: Option<A>) -> Self {
    Self { param }
  }
}

impl Actor for OneParamActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    context.unhandled(message);
  }
}

/// The original's `TwoParamActor`: takes two arguments and handles no
/// message.
struct TwoParamActor {
  #[allow(dead_code)]
  first: Option<A>,
  #[allow(dead_code)]
  second: Option<B>,
}

impl TwoParamActor {
  fn new(first: Option<A>, second: Option<B>) -> Self {
    Self { first, second }
  }
}

impl Actor for TwoParamActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    context.unhandled(message);
  }
}

/// The original's `OneParamActorCreator`, an object implementing the Java
/// `Creator[Actor]` interface whose `create()` returns
/// `new OneParamActor(null)`.
struct OneParamActorCreator;

impl OneParamActorCreator {
  fn create(&self) -> OneParamActor {
    OneParamActor::new(None)
  }
}

fn new_system() -> ActorSystem {
  ActorSystem::new("props-creation", Arc::new(ChannelBoundary::new()))
}

fn observed(log: &Arc<Mutex<Vec<Observed>>>) -> Vec<Observed> {
  log.lock().expect("observation log").clone()
}

// "work with creator"
#[test]
fn props_work_with_creator() {
  let log = Arc::new(Mutex::new(Vec::new()));
  let recorded = Arc::clone(&log);
  let props = Props::new(move || {
    recorded.lock().expect("observation log").push(Observed::One(false));
    OneParamActor::new(None)
  });

  let mut system = new_system();
  let _actor = system.actor_of(props);
  system.run_until_idle();

  assert_eq!(observed(&log), vec![Observed::One(false)]);
  system.terminate();
}

// "work with classOf"
// The original's body is identical to the next case; only the title differs.
#[test]
fn props_work_with_class_of() {
  let log = Arc::new(Mutex::new(Vec::new()));
  let recorded = Arc::clone(&log);
  let props = Props::new(move || {
    recorded.lock().expect("observation log").push(Observed::One(false));
    OneParamActor::new(None)
  });

  let mut system = new_system();
  let _actor = system.actor_of(props);
  system.run_until_idle();

  assert_eq!(observed(&log), vec![Observed::One(false)]);
  system.terminate();
}

// "work with classOf + 1 `null` param"
// The original's body is identical to the previous case; only the title
// differs.
#[test]
fn props_work_with_class_of_and_one_null_param() {
  let log = Arc::new(Mutex::new(Vec::new()));
  let recorded = Arc::clone(&log);
  let props = Props::new(move || {
    recorded.lock().expect("observation log").push(Observed::One(false));
    OneParamActor::new(None)
  });

  let mut system = new_system();
  let _actor = system.actor_of(props);
  system.run_until_idle();

  assert_eq!(observed(&log), vec![Observed::One(false)]);
  system.terminate();
}

// "work with classOf + 2 `null` params"
#[test]
fn props_work_with_class_of_and_two_null_params() {
  let log = Arc::new(Mutex::new(Vec::new()));
  let recorded = Arc::clone(&log);
  let props = Props::new(move || {
    recorded.lock().expect("observation log").push(Observed::Two(false, false));
    TwoParamActor::new(None, None)
  });

  let mut system = new_system();
  let _actor = system.actor_of(props);
  system.run_until_idle();

  assert_eq!(observed(&log), vec![Observed::Two(false, false)]);
  system.terminate();
}

// "work with create(class, creator)" (Java API)
#[test]
fn props_work_with_create_class_and_creator() {
  let log = Arc::new(Mutex::new(Vec::new()));
  let recorded = Arc::clone(&log);
  let creator = OneParamActorCreator;
  let props = Props::new(move || {
    recorded.lock().expect("observation log").push(Observed::One(false));
    creator.create()
  });

  let mut system = new_system();
  let _actor = system.actor_of(props);
  system.run_until_idle();

  assert_eq!(observed(&log), vec![Observed::One(false)]);
  system.terminate();
}
