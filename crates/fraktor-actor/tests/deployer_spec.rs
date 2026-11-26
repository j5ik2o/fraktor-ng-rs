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

// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/actor/DeployerSpec.scala
//
// Mirrors `org.apache.pekko.actor.DeployerSpec`. The original reads HOCON
// sections; the port feeds the typed `DeploymentSection` values that stand
// for the same sections one to one (design of #195, sections 1.2 and 5).
// Cases 5 and 16 are the faithfulness replacements F1 and F3 of that
// design. A few assertions are stronger than the original's `assertRouting`
// (which compares only the path, the router class, the scope and the
// resizer); each stronger assertion is marked where it appears.

use core::time::Duration;

use fraktor_actor::kernel::actor::deploy::{
  DefaultResizer, Deploy, DeployError, DeployedRouter, Deployer, DeploymentSection, NumberOfInstances, RouterSelection,
  Scope,
};
use fraktor_actor::kernel::actor::PoolKind;

fn section(router: RouterSelection) -> DeploymentSection {
  DeploymentSection { router, ..DeploymentSection::default() }
}

/// The sections of the original's `deployerConf`, as typed values.
fn deployer() -> Deployer {
  let scatter_gather = DeploymentSection {
    router: RouterSelection::ScatterGatherPool,
    within: Duration::from_secs(2),
    ..DeploymentSection::default()
  };
  let resizer = DeploymentSection {
    router: RouterSelection::RoundRobinPool,
    resizer: Some(fraktor_actor::kernel::actor::deploy::ResizerSection {
      lower_bound: 1,
      upper_bound: 10,
      ..fraktor_actor::kernel::actor::deploy::ResizerSection::default()
    }),
    ..DeploymentSection::default()
  };
  let direct2 = DeploymentSection {
    router: RouterSelection::FromCode,
    nr_of_instances: NumberOfInstances::Value(2),
    ..DeploymentSection::default()
  };
  Deployer::from_sections([
    ("/service1".to_string(), DeploymentSection::default()),
    ("/service-direct".to_string(), section(RouterSelection::FromCode)),
    ("/service-direct2".to_string(), direct2),
    (
      "/service3".to_string(),
      DeploymentSection { dispatcher: "my-dispatcher".to_string(), ..DeploymentSection::default() },
    ),
    ("/service4".to_string(), DeploymentSection { mailbox: "my-mailbox".to_string(), ..DeploymentSection::default() }),
    ("/service-round-robin".to_string(), section(RouterSelection::RoundRobinPool)),
    ("/service-random".to_string(), section(RouterSelection::RandomPool)),
    ("/service-scatter-gather".to_string(), scatter_gather),
    ("/service-consistent-hashing".to_string(), section(RouterSelection::ConsistentHashingPool)),
    ("/service-resizer".to_string(), resizer),
    ("/some/random-service".to_string(), section(RouterSelection::RoundRobinPool)),
    ("/some/*".to_string(), section(RouterSelection::RandomPool)),
    ("/*/some".to_string(), section(RouterSelection::ScatterGatherPool)),
    ("/double/**".to_string(), section(RouterSelection::RandomPool)),
    ("/double/more/**".to_string(), section(RouterSelection::RoundRobinPool)),
  ])
  .expect("the spec sections are valid")
}

fn pool(kind: PoolKind, nr_of_instances: i32) -> DeployedRouter {
  DeployedRouter::Pool { kind, nr_of_instances, resizer: None, use_pool_dispatcher: false }
}

// "be able to parse 'pekko.actor.deployment._' with all default values"
#[test]
fn deployer_empty_section_yields_the_default_deploy() {
  let deployer = deployer();
  let expected = Deploy {
    path: "/service1".to_string(),
    section: DeploymentSection::default(),
    router: DeployedRouter::NoRouter,
    scope: Scope::NoScopeGiven,
    dispatcher: String::new(),
    mailbox: String::new(),
  };
  assert_eq!(deployer.lookup_path("/service1"), Some(&expected));
}

// "use None deployment for undefined service"
#[test]
fn deployer_lookup_of_an_undefined_service_is_none() {
  let deployer = deployer();
  assert_eq!(deployer.lookup_path("/undefined"), None);
}

// "be able to parse 'pekko.actor.deployment._' with dispatcher config"
#[test]
fn deployer_section_with_a_dispatcher_names_that_dispatcher() {
  let deployer = deployer();
  let found = deployer.lookup_path("/service3").expect("the section is deployed");
  assert_eq!(found.dispatcher, "my-dispatcher");
  assert_eq!(found.mailbox, "");
  assert_eq!(found.router, DeployedRouter::NoRouter);
  assert_eq!(found.scope, Scope::NoScopeGiven);
}

// "be able to parse 'pekko.actor.deployment._' with mailbox config"
#[test]
fn deployer_section_with_a_mailbox_names_that_mailbox() {
  let deployer = deployer();
  let found = deployer.lookup_path("/service4").expect("the section is deployed");
  assert_eq!(found.mailbox, "my-mailbox");
  assert_eq!(found.dispatcher, "");
  assert_eq!(found.router, DeployedRouter::NoRouter);
  assert_eq!(found.scope, Scope::NoScopeGiven);
}

// "detect invalid number-of-instances" (faithfulness replacement F1: the
// HOCON type check becomes an uninterpreted typed value that the
// interpretation step refuses, for the kinds that read the count)
#[test]
fn deployer_rejects_a_non_numeric_number_of_instances() {
  let invalid = DeploymentSection {
    router: RouterSelection::RoundRobinPool,
    nr_of_instances: NumberOfInstances::NotANumber("boom".to_string()),
    ..DeploymentSection::default()
  };
  let result = Deployer::from_sections([("/service-invalid-number-of-instances".to_string(), invalid)]);
  assert_eq!(result.unwrap_err(), DeployError::WrongType { key: "nr-of-instances", value: "boom".to_string() });
}

// "detect invalid deployment path"
#[test]
fn deployer_rejects_an_invalid_deployment_path() {
  let section = DeploymentSection {
    router: RouterSelection::RoundRobinPool,
    nr_of_instances: NumberOfInstances::Value(2),
    ..DeploymentSection::default()
  };
  let result = Deployer::from_sections([("/gul/ubåt".to_string(), section)]);
  match result.unwrap_err() {
    | DeployError::InvalidName(error) => {
      let message = error.to_string();
      assert!(message.contains("[ubåt]"), "the message names the element: {message}");
      assert!(message.contains("[/gul/ubåt]"), "the message names the full path: {message}");
    },
    | other => panic!("expected an invalid name error, got {other:?}"),
  }
}

// "be able to parse 'pekko.actor.deployment._' with from-code router"
#[test]
fn deployer_from_code_router_is_no_router() {
  let deployer = deployer();
  let found = deployer.lookup_path("/service-direct").expect("the section is deployed");
  assert_eq!(found.path, "/service-direct");
  assert_eq!(found.router, DeployedRouter::NoRouter);
  assert_eq!(found.scope, Scope::NoScopeGiven);
}

// "ignore nr-of-instances with from-code router"
#[test]
fn deployer_from_code_router_ignores_the_number_of_instances() {
  let deployer = deployer();
  let found = deployer.lookup_path("/service-direct2").expect("the section is deployed");
  assert_eq!(found.path, "/service-direct2");
  assert_eq!(found.router, DeployedRouter::NoRouter);
}

// "be able to parse 'pekko.actor.deployment._' with round-robin router"
// (the count of one is a stronger assertion than the original's)
#[test]
fn deployer_round_robin_section_yields_a_round_robin_pool_of_one() {
  let deployer = deployer();
  let found = deployer.lookup_path("/service-round-robin").expect("the section is deployed");
  assert_eq!(found.path, "/service-round-robin");
  assert_eq!(found.router, pool(PoolKind::RoundRobin, 1));
  assert_eq!(found.scope, Scope::NoScopeGiven);
}

// "be able to parse 'pekko.actor.deployment._' with random router"
#[test]
fn deployer_random_section_yields_a_random_pool_of_one() {
  let deployer = deployer();
  let found = deployer.lookup_path("/service-random").expect("the section is deployed");
  assert_eq!(found.path, "/service-random");
  assert_eq!(found.router, pool(PoolKind::Random, 1));
  assert_eq!(found.scope, Scope::NoScopeGiven);
}

// "be able to parse 'pekko.actor.deployment._' with scatter-gather router"
// (the count and the within of two seconds are stronger assertions than
// the original's)
#[test]
fn deployer_scatter_gather_section_yields_a_pool_with_a_two_second_within() {
  let deployer = deployer();
  let found = deployer.lookup_path("/service-scatter-gather").expect("the section is deployed");
  assert_eq!(found.path, "/service-scatter-gather");
  assert_eq!(found.router, pool(PoolKind::ScatterGatherFirstCompleted { within: Duration::from_secs(2) }, 1));
  assert_eq!(found.scope, Scope::NoScopeGiven);
}

// "be able to parse 'pekko.actor.deployment._' with consistent-hashing router"
// (the original's Config constructor does not read virtual-nodes-factor,
// so the factor of the result is the constructor default of zero)
#[test]
fn deployer_consistent_hashing_section_yields_a_consistent_hashing_pool_of_one() {
  let deployer = deployer();
  let found = deployer.lookup_path("/service-consistent-hashing").expect("the section is deployed");
  assert_eq!(found.path, "/service-consistent-hashing");
  assert_eq!(found.router, pool(PoolKind::ConsistentHashing { virtual_nodes_factor: 0 }, 1));
  assert_eq!(found.scope, Scope::NoScopeGiven);
}

// "be able to parse 'pekko.actor.deployment._' with router resizer"
// (the original expects nrOfInstances = 0, which assertRouting never
// compares; the port keeps the section value of one, a stronger
// assertion, and compares the resizer, as the original does)
#[test]
fn deployer_section_with_a_resizer_section_yields_a_pool_with_the_default_resizer() {
  let deployer = deployer();
  let found = deployer.lookup_path("/service-resizer").expect("the section is deployed");
  assert_eq!(found.path, "/service-resizer");
  assert_eq!(
    found.router,
    DeployedRouter::Pool {
      kind: PoolKind::RoundRobin,
      nr_of_instances: 1,
      resizer: Some(DefaultResizer::defaults()),
      use_pool_dispatcher: false,
    }
  );
  assert_eq!(found.scope, Scope::NoScopeGiven);
}

// "be able to use wildcards"
// (the within of the second route is the default of five seconds; the
// original's expected expression says two seconds but never compares it,
// a stronger assertion here)
#[test]
fn deployer_single_wildcards_match_one_path_element() {
  let deployer = deployer();
  let first = deployer.lookup_path("/some/wildcardmatch").expect("the wildcard section matches");
  assert_eq!(first.path, "/some/*");
  assert_eq!(first.router, pool(PoolKind::Random, 1));
  let second = deployer.lookup_path("/somewildcardmatch/some").expect("the wildcard section matches");
  assert_eq!(second.path, "/*/some");
  assert_eq!(second.router, pool(PoolKind::ScatterGatherFirstCompleted { within: Duration::from_secs(5) }, 1));
}

// "be able to use double wildcards"
#[test]
fn deployer_double_wildcards_match_the_remaining_path_elements() {
  let deployer = deployer();
  let first = deployer.lookup_path("/double/wildcardmatch").expect("the double wildcard matches");
  assert_eq!(first.path, "/double/**");
  assert_eq!(first.router, pool(PoolKind::Random, 1));
  let second = deployer.lookup_path("/double/wildcardmatch/anothermatch").expect("the double wildcard matches");
  assert_eq!(second.path, "/double/**");
  let third = deployer.lookup_path("/double/more/anothermatch").expect("the longer prefix wins");
  assert_eq!(third.path, "/double/more/**");
  assert_eq!(third.router, pool(PoolKind::RoundRobin, 1));
  assert_eq!(deployer.lookup_path("/double"), None);
}

// "have correct router mappings" (faithfulness replacement F3: the
// mapping table becomes the typed correspondence of
// `RouterSelection::from_setting` and `RouterSelection::fqcn`, the shape
// of #192's ProviderSelection; the original compares thirteen names, the
// table itself has fifteen entries)
#[test]
fn deployer_router_type_mapping_matches_the_original_table() {
  let table = [
    ("from-code", "org.apache.pekko.routing.NoRouter"),
    ("round-robin-pool", "org.apache.pekko.routing.RoundRobinPool"),
    ("round-robin-group", "org.apache.pekko.routing.RoundRobinGroup"),
    ("random-pool", "org.apache.pekko.routing.RandomPool"),
    ("random-group", "org.apache.pekko.routing.RandomGroup"),
    ("balancing-pool", "org.apache.pekko.routing.BalancingPool"),
    ("smallest-mailbox-pool", "org.apache.pekko.routing.SmallestMailboxPool"),
    ("broadcast-pool", "org.apache.pekko.routing.BroadcastPool"),
    ("broadcast-group", "org.apache.pekko.routing.BroadcastGroup"),
    ("scatter-gather-pool", "org.apache.pekko.routing.ScatterGatherFirstCompletedPool"),
    ("scatter-gather-group", "org.apache.pekko.routing.ScatterGatherFirstCompletedGroup"),
    ("consistent-hashing-pool", "org.apache.pekko.routing.ConsistentHashingPool"),
    ("consistent-hashing-group", "org.apache.pekko.routing.ConsistentHashingGroup"),
  ];
  for (name, fqcn) in table {
    assert_eq!(RouterSelection::from_setting(name).fqcn(), fqcn, "the mapping of {name}");
  }
}
