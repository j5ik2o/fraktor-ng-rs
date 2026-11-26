// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2016-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file changes only by 870d4499cc #2235; the cases and their inputs are unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/scala/org/apache/pekko/actor/setup/ActorSystemSetupSpec.scala
//
// The original's five cases under "The ActorSystemSettings should" are kept as
// one test each, named after the case, with the original name as a comment.
//
// The original API and its Rust counterpart:
//
// - `case class DummySetup(name: String) extends Setup` is a struct with
//   `PartialEq`, `Debug` and `Clone`, the parts of a case class the assertions
//   use, and an `impl Setup`.
// - `ActorSystemSetup()` is `ActorSystemSetup::empty()`, and
//   `ActorSystemSetup(a, b)` is `ActorSystemSetup::from_setups([..])`.
// - `withSetup(s)` is `with_setup(s)`, and `a and b` is `a.and(b)`.
// - `get[T]: Option[T]` is `get::<T>()`, which returns `Option<&T>`. The
//   original widens it to `Option[Setup]` before `===`; the port compares the
//   concrete value, which is what that widening keeps.
// - `ActorSystem("name", setup)` is `ActorSystem::with_setup("name", boundary,
//   setup)`, and `system.settings.setup` is `system.setup()`: the port has no
//   settings layer between the system and its setup.
// - `TestKit.shutdownActorSystem(system)` in `finally` is `terminate`, and the
//   system's `Drop` terminates it on a failed assertion as `finally` would.
//
// The test calls the production `Setup`, `ActorSystemSetup` and the system's
// setup. Until they exist the file does not compile: that compile error is the
// intended red for this node.

use std::sync::Arc;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::setup::{ActorSystemSetup, Setup};

#[derive(Clone, Debug, PartialEq)]
struct DummySetup {
  name: String,
}

#[derive(Clone, Debug, PartialEq)]
struct DummySetup2 {
  name: String,
}

#[derive(Clone, Debug, PartialEq)]
struct DummySetup3 {
  name: String,
}

impl Setup for DummySetup {}
impl Setup for DummySetup2 {}
impl Setup for DummySetup3 {}

fn dummy(name: &str) -> DummySetup {
  DummySetup { name: name.to_string() }
}

fn dummy2(name: &str) -> DummySetup2 {
  DummySetup2 { name: name.to_string() }
}

// "store and retrieve a setup"
#[test]
fn store_and_retrieve_a_setup() {
  let setup = dummy("Al Dente");
  let setups = ActorSystemSetup::empty().with_setup(setup.clone());

  assert_eq!(setups.get::<DummySetup>(), Some(&setup));
  assert_eq!(setups.get::<DummySetup2>(), None);
}

// "replace setup if already defined"
#[test]
fn replace_setup_if_already_defined() {
  let setup1 = dummy("Al Dente");
  let setup2 = dummy("Earl E. Bird");
  let setups = ActorSystemSetup::empty().with_setup(setup1).with_setup(setup2.clone());

  assert_eq!(setups.get::<DummySetup>(), Some(&setup2));
}

// "provide a fluent creation alternative"
#[test]
fn provide_a_fluent_creation_alternative() {
  let a = dummy("Al Dente");
  let b = dummy("Earl E. Bird"); // same type again
  let c = dummy2("Amanda Reckonwith");
  let setups = a.and(b.clone()).and(c.clone());

  assert_eq!(setups.get::<DummySetup>(), Some(&b));
  assert_eq!(setups.get::<DummySetup2>(), Some(&c));
}

// "be created with a set of setups"
#[test]
fn be_created_with_a_set_of_setups() {
  let setup1 = dummy("Manny Kin");
  let setup2 = dummy2("Pepe Roni");
  let setups = ActorSystemSetup::from_setups([Arc::new(setup1) as Arc<dyn Setup>, Arc::new(setup2)]);

  assert!(setups.get::<DummySetup>().is_some());
  assert!(setups.get::<DummySetup2>().is_some());
  assert!(setups.get::<DummySetup3>().is_none());
}

// "be available from the ExtendedActorSystem"
#[test]
fn be_available_from_the_extended_actor_system() {
  let setup = dummy("Tad Moore");
  let mut system = ActorSystem::with_setup(
    "name",
    Arc::new(ChannelBoundary::new()),
    ActorSystemSetup::from_setups([Arc::new(setup.clone()) as Arc<dyn Setup>]),
  );

  assert_eq!(system.setup().get::<DummySetup>(), Some(&setup));

  system.terminate();
  assert!(system.is_terminated());
}
