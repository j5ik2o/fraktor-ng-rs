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

// The exhaustive table for the configuration verification of issue
// #194 (docs/design/issue-194-specs.md section 6): every combination
// of pool presence and kind, and of head and routee dispatcher kind,
// against the table that rules 1 to 3 of section 2.3 produce. This is
// a finite table, not a property test with random inputs.

use std::sync::Arc;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::{ActorCreationError, PoolKind, Props, RouterConfig};
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::dispatch::{DispatcherConfig, DispatcherKind, Dispatchers};
use fraktor_actor::kernel::setup::ActorSystemSetup;

fn system() -> ActorSystem {
  let dispatchers = Dispatchers::new(vec![
    DispatcherConfig { id: "balancing-dispatcher".to_string(), kind: DispatcherKind::Balancing },
    DispatcherConfig { id: "pinned-dispatcher".to_string(), kind: DispatcherKind::Pinned },
  ]);
  let setup = ActorSystemSetup::empty().with_setup(dispatchers);
  ActorSystem::with_setup("ActorConfigurationVerificationTable", Arc::new(ChannelBoundary::new()), setup)
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum DispatcherChoice {
  Unspecified,
  Balancing,
  Pinned,
  Missing,
}

impl DispatcherChoice {
  fn name(self) -> Option<&'static str> {
    match self {
      | DispatcherChoice::Unspecified => None,
      | DispatcherChoice::Balancing => Some("balancing-dispatcher"),
      | DispatcherChoice::Pinned => Some("pinned-dispatcher"),
      | DispatcherChoice::Missing => Some("does not exist"),
    }
  }
}

#[test]
fn verification_matches_the_table_for_every_combination() {
  let pools: Vec<Option<PoolKind>> = vec![
    None,
    Some(PoolKind::RoundRobin),
    Some(PoolKind::Broadcast),
    Some(PoolKind::Random),
    Some(PoolKind::SmallestMailbox),
    Some(PoolKind::ScatterGatherFirstCompleted { within: core::time::Duration::from_secs(1) }),
  ];
  let choices =
    [DispatcherChoice::Unspecified, DispatcherChoice::Balancing, DispatcherChoice::Pinned, DispatcherChoice::Missing];
  let mut system = system();
  let mut created = 0usize;
  for pool in &pools {
    for head in choices {
      for routee in choices {
        let mut props = Props::empty();
        if let Some(name) = routee.name() {
          props = props.with_dispatcher(name);
        }
        if let Some(pool) = pool {
          props = props.with_router(RouterConfig {
            pool: pool.clone(),
            nr_of_instances: 3,
            router_dispatcher: head.name().map(str::to_string),
          });
        }
        // Rule table, from section 2.3 of the design:
        // - no router: the routee-side dispatcher (the Props
        //   dispatcher) must exist; its kind does not matter.
        // - router: the routee dispatcher must exist first; then the
        //   head dispatcher must exist; then a balancing head fails.
        let expect_ok = match pool {
          | None => routee != DispatcherChoice::Missing,
          | Some(_) => {
            routee != DispatcherChoice::Missing
              && head != DispatcherChoice::Missing
              && head != DispatcherChoice::Balancing
          },
        };
        let before = created;
        match system.try_actor_of(props, None) {
          | Ok(_) => {
            created += 1;
            assert!(expect_ok, "pool {pool:?}, head {head:?}, routee {routee:?}: expected a refusal");
          },
          | Err(ActorCreationError::Configuration(_)) => {
            assert!(!expect_ok, "pool {pool:?}, head {head:?}, routee {routee:?}: expected success");
          },
          | Err(other) => panic!("pool {pool:?}, head {head:?}, routee {routee:?}: unexpected error {other}"),
        }
        if !expect_ok {
          assert_eq!(created, before, "a refused creation must not create an actor");
        }
      }
    }
  }
  system.terminate();
}
// The head of a verified router reports a message sent to it as
// unhandled, as the design's condition (a) for the minimal router
// decides; routing itself is issue #269.
#[test]
fn router_head_reports_messages_as_unhandled() {
  let mut spec = fraktor_testkit::fraktorspec::FraktorSpec::new("ActorConfigurationVerificationTable");
  let props = Props::empty().with_router(fraktor_actor::kernel::actor::RouterConfig {
    pool: PoolKind::Broadcast,
    nr_of_instances: 5,
    router_dispatcher: None,
  });
  let head = spec.system_mut().try_actor_of(props, None).expect("the router head is created");
  let dead_letters = spec.system().dead_letters();
  let filter = fraktor_testkit::event_filter::EventFilter::warning()
    .with_message(&format!("unhandled message from {}", dead_letters.path()));
  let from = dead_letters.clone();
  fraktor_testkit::event_filter::EventFilter::intercept(&filter, spec.system_mut(), move || {
    head.tell("hello", from);
  });
  spec.terminate();
}
