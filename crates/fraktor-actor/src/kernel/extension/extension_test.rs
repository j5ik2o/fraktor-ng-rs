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

//! Unit tests for the extension registry and for named top-level actors.
//!
//! The registry's re-entrancy and panic behaviour, and the name rules of
//! `ActorSystem::actor_of_with_name`, are not cases of a ported Pekko spec;
//! they are recorded here, outside the ledger, as the design of issue #193
//! requires.

use alloc::string::ToString;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::adaptor::ChannelBoundary;
use crate::kernel::actor::Props;
use crate::kernel::actor_system::ActorSystem;
use crate::kernel::extension::{Extension, ExtensionError, ExtensionId};

fn new_system() -> ActorSystem {
  ActorSystem::new("extension-unit", Arc::new(ChannelBoundary::new()))
}

/// An id whose creation requests its own id again.
#[derive(Clone)]
struct ReentrantId {
  creations: Arc<AtomicUsize>,
}

#[derive(Debug)]
struct ReentrantExtension;

impl Extension for ReentrantExtension {}

impl ExtensionId for ReentrantId {
  type Ext = ReentrantExtension;

  fn create_extension(&self, system: &mut ActorSystem) -> Result<ReentrantExtension, ExtensionError> {
    self.creations.fetch_add(1, Ordering::SeqCst);
    // The inner request finds this id marked as being created and fails;
    // the creation returns that failure unchanged.
    system.register_extension(self)?;
    Ok(ReentrantExtension)
  }
}

#[test]
fn reentrant_creation_fails_with_in_progress_and_is_not_retried() {
  let mut system = new_system();
  let id = ReentrantId { creations: Arc::new(AtomicUsize::new(0)) };

  let error = system.register_extension(&id).expect_err("re-entrant creation fails");
  assert_eq!(error.cause_name(), "ExtensionInProgress");
  assert_eq!(id.creations.load(Ordering::SeqCst), 1);

  // The failure is kept: a later request returns it without creating again.
  let again = system.register_extension(&id).expect_err("the kept failure is returned");
  assert_eq!(again, error);
  assert_eq!(id.creations.load(Ordering::SeqCst), 1);
}

/// An id whose creation always panics.
struct PanickingId;

#[derive(Debug)]
struct PanickingExtension;

impl Extension for PanickingExtension {}

impl ExtensionId for PanickingId {
  type Ext = PanickingExtension;

  fn create_extension(&self, _system: &mut ActorSystem) -> Result<PanickingExtension, ExtensionError> {
    panic!("creation panics");
  }
}

#[test]
fn panicked_creation_propagates_and_later_requests_get_in_progress() {
  let mut system = new_system();
  let id = PanickingId;

  let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| system.register_extension(&id)));
  assert!(outcome.is_err(), "the panic propagates out of register_extension");

  // The marker stays: later requests return the in-progress error, whose
  // content is not the panic's, and creation is not retried.
  let error = system.register_extension(&id).expect_err("the marker stays");
  assert_eq!(error.cause_name(), "ExtensionInProgress");
  assert!(system.extension(&id).is_err());
  assert!(!system.has_extension(&id));
}

#[test]
fn duplicate_top_level_name_is_refused() {
  let mut system = new_system();
  let first = system.actor_of_with_name(Props::empty(), "uniqueName").expect("the first actor starts");
  assert_eq!(first.path().elements(), alloc::vec!["user".to_string(), "uniqueName".to_string()]);

  let error = system.actor_of_with_name(Props::empty(), "uniqueName").expect_err("the name is taken");
  assert_eq!(error.to_string(), "Actor name [uniqueName] is not unique!");

  system.terminate();
}

#[test]
fn top_level_name_is_reusable_after_the_actor_stops() {
  let mut system = new_system();
  let first = system.actor_of_with_name(Props::empty(), "recycled").expect("the first actor starts");
  system.stop(&first);
  system.run_until_idle();
  assert!(system.is_actor_terminated(&first));

  let second = system.actor_of_with_name(Props::empty(), "recycled").expect("the name is free again");
  assert_ne!(first, second);
  assert_eq!(second.path().name(), "recycled");

  system.terminate();
}

#[test]
fn invalid_top_level_names_are_refused_with_the_validation_message() {
  let mut system = new_system();
  for (name, expected) in [
    ("", "Actor path element must not be empty"),
    ("$hallo", "Invalid actor path element [$hallo]"),
    ("üß", "Invalid actor path element [üß]"),
    ("a/b", "Invalid actor path element [a/b]"),
  ] {
    let error = system.actor_of_with_name(Props::empty(), name).expect_err("the name is refused");
    assert!(error.to_string().starts_with(expected), "name {name:?}: got {error}");
  }

  system.terminate();
}

#[test]
fn named_actor_on_a_terminated_system_returns_terminating() {
  let mut system = new_system();
  system.terminate();

  let error = system.actor_of_with_name(Props::empty(), "late").expect_err("the system is terminated");
  assert_eq!(error.to_string(), "cannot create children while terminating or terminated");
}
