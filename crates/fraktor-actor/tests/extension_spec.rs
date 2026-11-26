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

// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/actor/ExtensionSpec.scala
//
// Five of the original's eight cases are kept as one test each, named after
// the case, with the original name as a comment. The remaining three are not
// in this file:
//
// - "allow for auto-loading of library-extensions from reference.conf" is
//   awaiting the owner's decision on its weakened form (marshal's item 441,
//   question 3) and is added once that decision arrives.
// - "log an error if an extension listed in pekko.extensions cannot be
//   loaded" and "fail the actor system if a library-extension cannot be
//   loaded" are proposed as not built: the original loads extensions by
//   class name from a config string, and a name that cannot be loaded is a
//   state the port's typed input cannot express. That proposal is awaiting
//   the owner's decision (marshal's item 441, question 1); nothing here
//   records it as approved. The alternative example is the `compile_fail`
//   doctest on `ExtensionStartup`.
//
// The original API and its Rust counterpart:
//
// - `pekko.extensions = ["org.apache.pekko.actor.TestExtension"]` (a config
//   string of class names) is the typed `ExtensionStartup` value handed to
//   `ActorSystem::with_extensions`: `extension(id)` is the listed column and
//   `library_extension(id)` is the `pekko.library-extensions` column. The
//   library column is processed first, as Pekko starts it first. The port
//   does not read the columns back from a config; the creation counts are
//   the property under test, so the original's `listedExtensions.count(...)`
//   readbacks are not ported.
// - `TestExtension(system)` (the id's `apply`, which creates) is
//   `register_extension`, and `system.extension(id)` (lookup only) is
//   `extension`. The original's `TestExtension` holds the system itself;
//   the port holds the system's name, because `ActorSystem` is not `Clone`,
//   and the test compares the names.
// - The original's `FailingTestExtension` creates an actor named
//   "uniqueName" as a side effect and then throws `TestException`. The port
//   returns `Err(ExtensionError)` with the cause name `TestException`
//   instead, and the test checks that a second request returns the same
//   error and that creation ran only once. The original does not count
//   creations; the port does, because "a failed creation is not retried" is
//   the property of the case and the count is its direct observation.
// - `InstanceCountingExtension.createCount` is a global `AtomicInteger` in
//   the original, which assumes its tests do not run in parallel. The port
//   carries the counter inside the id value (`Arc<AtomicUsize>`), so the
//   parallel tests of one binary do not contaminate each other.
// - `shutdownActorSystem(system)` is `terminate`, and the system's `Drop`
//   terminates it on a failed assertion.
// - The port does not use the testkit (`FraktorSpec`, `EventFilter`) that
//   the ledger's `needs` lists for this node; the error-log case those
//   entries belong to is the not-built proposal above.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::extension::{Extension, ExtensionError, ExtensionId, ExtensionStartup};

/// The original's `TestExtension`: an extension that keeps the system it was
/// created for. The port keeps the system's name.
struct TestExtension {
  system_name: String,
}

impl Extension for TestExtension {}

/// The id of [`TestExtension`]. A unit value: extensions are keyed by the
/// id's type.
#[derive(Clone)]
struct TestExtensionId;

impl ExtensionId for TestExtensionId {
  type Ext = TestExtension;

  fn create_extension(&self, system: &mut ActorSystem) -> Result<TestExtension, ExtensionError> {
    Ok(TestExtension { system_name: system.name().to_string() })
  }
}

/// The original's `FailingTestExtension`: creating it starts an actor named
/// "uniqueName" as a side effect and then fails with `TestException`.
struct FailingTestExtension;

impl Extension for FailingTestExtension {}

/// The id of [`FailingTestExtension`]. The creation count travels in the id
/// value, as the counting id below does.
#[derive(Clone)]
struct FailingTestExtensionId {
  creations: Arc<AtomicUsize>,
}

impl FailingTestExtensionId {
  fn new() -> Self {
    Self { creations: Arc::new(AtomicUsize::new(0)) }
  }

  fn creations(&self) -> usize {
    self.creations.load(Ordering::SeqCst)
  }
}

impl ExtensionId for FailingTestExtensionId {
  type Ext = FailingTestExtension;

  fn create_extension(&self, system: &mut ActorSystem) -> Result<FailingTestExtension, ExtensionError> {
    self.creations.fetch_add(1, Ordering::SeqCst);
    // The side effect comes first, as in the original: the named actor is
    // created before the failure.
    let _actor = system
      .actor_of_with_name(Props::empty(), "uniqueName")
      .expect("the name uniqueName is free on a fresh system");
    Err(ExtensionError::new("TestException", "ERR"))
  }
}

/// The original's `InstanceCountingExtension`: an extension whose creation
/// is counted.
struct InstanceCountingExtension;

impl Extension for InstanceCountingExtension {}

/// The id of [`InstanceCountingExtension`]. The counter travels in the id
/// value, not in a global, so parallel tests do not contaminate each other.
#[derive(Clone)]
struct InstanceCountingExtensionId {
  count: Arc<AtomicUsize>,
}

impl InstanceCountingExtensionId {
  fn new() -> Self {
    Self { count: Arc::new(AtomicUsize::new(0)) }
  }

  fn count(&self) -> usize {
    self.count.load(Ordering::SeqCst)
  }
}

impl ExtensionId for InstanceCountingExtensionId {
  type Ext = InstanceCountingExtension;

  fn create_extension(&self, _system: &mut ActorSystem) -> Result<InstanceCountingExtension, ExtensionError> {
    self.count.fetch_add(1, Ordering::SeqCst);
    Ok(InstanceCountingExtension)
  }
}

fn boundary() -> Arc<ChannelBoundary> {
  Arc::new(ChannelBoundary::new())
}

// "support extensions"
#[test]
fn supports_extensions() {
  let startup = ExtensionStartup::new().extension(TestExtensionId);
  let mut system = ActorSystem::with_extensions("extensions", boundary(), startup).expect("startup succeeds");

  // The configured extension is loaded at startup.
  assert!(system.has_extension(&TestExtensionId));
  let looked_up = system.extension(&TestExtensionId).expect("the extension is registered");
  assert_eq!(looked_up.system_name, system.name());
  // The creating path returns the same instance as the lookup path.
  let registered = system.register_extension(&TestExtensionId).expect("the extension is registered");
  assert!(Arc::ptr_eq(&looked_up, &registered));

  system.terminate();
}

// "handle extensions that fail to initialize"
#[test]
fn handles_extensions_that_fail_to_initialize() {
  let mut system = ActorSystem::new("extensions", boundary());
  let id = FailingTestExtensionId::new();

  // First attempt: an actor is created as a side effect, then the creation
  // fails.
  let first = system.register_extension(&id).expect_err("creation fails");
  assert_eq!(first.cause_name(), "TestException");
  assert_eq!(first.message(), "ERR");

  // Second attempt: the same (kept) error, and creation is not retried.
  let second = system.register_extension(&id).expect_err("the kept failure is returned");
  assert_eq!(second, first);
  assert_eq!(id.creations(), 1);
  assert!(!system.has_extension(&id));
  assert!(system.extension(&id).is_err());

  system.terminate();
}

// "fail the actor system if an extension listed in pekko.extensions fails to start"
#[test]
fn fails_the_actor_system_if_a_listed_extension_fails_to_start() {
  let startup = ExtensionStartup::new().extension(FailingTestExtensionId::new());
  let error = ActorSystem::with_extensions("failing", boundary(), startup).expect_err("the system fails to start");
  assert_eq!(error.cause_name(), "TestException");
}

// "not create duplicate instances when auto-loading of library-extensions"
#[test]
fn does_not_create_duplicate_instances_when_auto_loading_library_extensions() {
  let id = InstanceCountingExtensionId::new();
  // The same extension is offered three times in the library column, as the
  // original lists it three times (once with a `$` suffix) in its config.
  let startup = ExtensionStartup::new()
    .library_extension(id.clone())
    .library_extension(id.clone())
    .library_extension(id.clone());
  let mut system = ActorSystem::with_extensions("extensions", boundary(), startup).expect("startup succeeds");

  assert_eq!(id.count(), 1);
  assert!(system.has_extension(&id));

  system.terminate();
}

// "fail the actor system if a library-extension fails to start"
#[test]
fn fails_the_actor_system_if_a_library_extension_fails_to_start() {
  let startup = ExtensionStartup::new().library_extension(FailingTestExtensionId::new());
  let error = ActorSystem::with_extensions("failing", boundary(), startup).expect_err("the system fails to start");
  assert_eq!(error.cause_name(), "TestException");
}
