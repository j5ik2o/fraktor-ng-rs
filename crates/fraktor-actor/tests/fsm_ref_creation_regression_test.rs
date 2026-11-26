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

//! Behaviour contracts of the child-creation entry that a test handle
//! reaches: the user guardian takes children like a top-level creation
//! does, and a dispatcher name whose alias cannot be resolved is
//! refused by verification instead of panicking at creation.

use std::sync::Arc;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::dispatch_settings::DispatchSettings;
use fraktor_actor::kernel::actor::{ActorCreationError, Props};
use fraktor_actor::kernel::actor_path::ActorPath;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::dispatch::{DispatcherConfig, DispatcherKind, Dispatchers};
use fraktor_actor::kernel::setup::ActorSystemSetup;
use fraktor_actor::kernel::supervision::SupervisorStrategy;

const SPEC_SYSTEM_NAME: &str = "FsmRefCreationRegression";

fn system_with_broken_alias() -> ActorSystem {
  let dispatchers =
    Dispatchers::new(vec![DispatcherConfig { id: "aliased-dispatcher".to_string(), kind: DispatcherKind::Default }]);
  let mut settings = DispatchSettings::new();
  settings.alias_dispatcher("aliased-dispatcher", "missing-target");
  let setup = ActorSystemSetup::empty().with_setup(dispatchers).with_setup(settings);
  ActorSystem::with_setup(SPEC_SYSTEM_NAME, Arc::new(ChannelBoundary::new()), setup)
}

#[test]
fn child_creation_refuses_a_dispatcher_whose_alias_target_is_missing() {
  let mut system = system_with_broken_alias();
  let parent = system.actor_of(Props::empty());
  let props = Props::empty().with_dispatcher("aliased-dispatcher");
  let result =
    system.downgrade().try_spawn_child(&parent, props, Some("child"), SupervisorStrategy::default_strategy());
  match result {
    | Err(ActorCreationError::Configuration(_)) => {},
    | Err(other) => panic!("expected a configuration error, got {other}"),
    | Ok(child) => panic!("expected a configuration error, got actor {}", child.path()),
  }
  system.terminate();
}

#[test]
fn a_child_of_the_user_guardian_is_created_like_a_top_level_actor() {
  let mut system = ActorSystem::new(SPEC_SYSTEM_NAME, Arc::new(ChannelBoundary::new()));
  let guardian_path =
    ActorPath::from_string(&format!("pekko://{SPEC_SYSTEM_NAME}")).expect("the root path parses").with_child("user");
  let guardian = system.resolve_actor_ref(&guardian_path);
  let child = system
    .downgrade()
    .try_spawn_child(&guardian, Props::empty(), Some("guardian-child"), SupervisorStrategy::default_strategy())
    .expect("the guardian takes children");
  assert_eq!(child.path(), guardian_path.with_child("guardian-child"));
  let again = system.downgrade().try_spawn_child(
    &guardian,
    Props::empty(),
    Some("guardian-child"),
    SupervisorStrategy::default_strategy(),
  );
  assert!(matches!(again, Err(ActorCreationError::InvalidName(_))), "the name was taken twice: {again:?}");
  let named = system.actor_of_named(Props::empty(), "guardian-child");
  assert!(matches!(named, Err(ActorCreationError::InvalidName(_))), "the top-level entry missed the name: {named:?}");
  system.terminate();
}
