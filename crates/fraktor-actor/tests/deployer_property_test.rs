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

// Property tests for the Deployer of issue #195 (design section 3,
// P1 to P8). These are not ports of original cases and do not count
// toward the ledger. Fixed-seed generators, in the style of the
// WildcardIndex model test of #187. The generator depth is capped at
// four elements: the index recurses on clone and drop, and very deep
// paths overflow the stack (an existing property of #187, recorded in
// the design).

use core::time::Duration;

use fraktor_actor::kernel::actor::{
  Deploy, DeployError, DeployedRouter, Deployer, DeploymentSection, NumberOfInstances, PoolKind, ResizerSection,
  RouterSelection, Scope,
};
use fraktor_actor::kernel::wildcard_index::WildcardIndex;

struct SplitMix64(u64);

impl SplitMix64 {
  fn next_u64(&mut self) -> u64 {
    self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.0;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }

  fn below(&mut self, bound: u64) -> usize {
    (self.next_u64() % bound) as usize
  }

  fn pick<'a>(&mut self, items: &'a [&'a str]) -> &'a str {
    items[self.below(items.len() as u64)]
  }
}

/// The split of the design section 2.3, written out independently here
/// so P5 compares two writings of the same rule: Java's
/// `path.split("/").drop(1)` drops trailing empty strings and then the
/// first element.
fn split_like_java(path: &str) -> Vec<String> {
  let mut parts: Vec<&str> = path.split('/').collect();
  while parts.len() > 1 && parts.last() == Some(&"") {
    parts.pop();
  }
  if path.is_empty() {
    // Java: "".split("/") is [""], and drop(1) leaves nothing.
    return Vec::new();
  }
  parts.into_iter().skip(1).map(str::to_string).collect()
}

fn deploy_value(path: &str) -> Deploy {
  Deploy {
    path: path.to_string(),
    section: DeploymentSection::default(),
    router: DeployedRouter::NoRouter,
    scope: Scope::NoScopeGiven,
    dispatcher: String::new(),
    mailbox: String::new(),
  }
}

const KEY_ELEMENTS: [&str; 6] = ["a", "b", "some", "*", "**", "a*"];
const QUERY_ELEMENTS: [&str; 6] = ["a", "b", "some", "x", "a*", "ab"];

fn random_elements(rng: &mut SplitMix64, pool: &[&str]) -> Vec<String> {
  let depth = 1 + rng.below(4);
  (0..depth).map(|_| rng.pick(pool).to_string()).collect()
}

// P1: the lookup of the Deployer agrees with a WildcardIndex fed the
// same keys split the same way, and a returned path is one of the
// registered keys.
#[test]
fn deployer_lookup_agrees_with_a_wildcard_index_fed_the_same_keys() {
  let mut rng = SplitMix64(0x195D);
  for _ in 0..200 {
    let key_count = 1 + rng.below(6);
    let mut deployer = Deployer::from_sections([]).expect("an empty deployer builds");
    let mut model: WildcardIndex<String> = WildcardIndex::new();
    let mut keys: Vec<String> = Vec::new();
    for _ in 0..key_count {
      let elements = random_elements(&mut rng, &KEY_ELEMENTS);
      let key = format!("/{}", elements.join("/"));
      if deployer.deploy(deploy_value(&key)).is_err() {
        continue;
      }
      let refs: Vec<&str> = elements.iter().map(String::as_str).collect();
      model = model.insert(&refs, key.clone()).expect("the same insert succeeds");
      keys.push(key);
    }
    for _ in 0..10 {
      let query = random_elements(&mut rng, &QUERY_ELEMENTS);
      let query_refs: Vec<&str> = query.iter().map(String::as_str).collect();
      let expected = model.find(&query_refs);
      let found = deployer.lookup(&query_refs).map(|deploy| &deploy.path);
      assert_eq!(found, expected, "query {query:?} over keys {keys:?}");
      if let Some(path) = found {
        assert!(keys.iter().any(|key| key == path), "the returned path is a registered key");
      }
    }
  }
}

// P2: a Deployer holding only an exact section answers that path and
// nothing else.
#[test]
fn deployer_with_only_an_exact_section_answers_only_that_path() {
  let mut rng = SplitMix64(0x195E);
  for _ in 0..100 {
    let elements = random_elements(&mut rng, &["a", "b", "some"]);
    let key = format!("/{}", elements.join("/"));
    let deployer = Deployer::from_sections([(key.clone(), DeploymentSection::default())]).expect("valid key");
    assert_eq!(deployer.lookup_path(&key).map(|deploy| &deploy.path), Some(&key));
    let other = random_elements(&mut rng, &QUERY_ELEMENTS);
    let other_path = format!("/{}", other.join("/"));
    if other_path != key {
      assert_eq!(deployer.lookup_path(&other_path), None, "only {key} is registered");
    }
  }
}

// P3: a Deployer holding only a trailing double wildcard section
// matches every extension of the prefix and not the prefix itself. The
// prefix holds no prefix pattern elements.
#[test]
fn deployer_with_only_a_double_wildcard_section_matches_every_extension() {
  let mut rng = SplitMix64(0x195F);
  for _ in 0..100 {
    let prefix = random_elements(&mut rng, &["a", "b", "some"]);
    let key = format!("/{}/**", prefix.join("/"));
    let deployer = Deployer::from_sections([(key.clone(), DeploymentSection::default())]).expect("valid key");
    let prefix_path = format!("/{}", prefix.join("/"));
    assert_eq!(deployer.lookup_path(&prefix_path), None, "the prefix itself does not match");
    let extension = random_elements(&mut rng, &QUERY_ELEMENTS);
    let extended = format!("{}/{}", prefix_path, extension.join("/"));
    assert_eq!(
      deployer.lookup_path(&extended).map(|deploy| &deploy.path),
      Some(&key),
      "every extension matches"
    );
  }
}

// P4: registration of arbitrary path strings never panics; the outcome
// is a registration or one of the two documented errors.
#[test]
fn deployer_registration_of_arbitrary_paths_never_panics() {
  let pieces = ["a", "*", "**", "***", "a**", "", "ubåt", "$bad", "a b"];
  let mut rng = SplitMix64(0x1960);
  for _ in 0..300 {
    let depth = rng.below(4);
    let mut path = String::new();
    for _ in 0..depth {
      path.push('/');
      path.push_str(rng.pick(&pieces));
    }
    let mut deployer = Deployer::from_sections([]).expect("an empty deployer builds");
    match deployer.deploy(deploy_value(&path)) {
      | Ok(()) => {},
      | Err(DeployError::InvalidName(_)) | Err(DeployError::Index(_)) => {},
      | Err(other) => panic!("an unexpected error for {path:?}: {other:?}"),
    }
  }
}

// P5: looking up by a path string agrees with looking up by the
// elements the split of section 2.3 produces.
#[test]
fn deployer_lookup_by_path_agrees_with_lookup_by_split_elements() {
  let deployer = Deployer::from_sections([
    ("/service".to_string(), DeploymentSection::default()),
    ("/b".to_string(), DeploymentSection::default()),
    ("/some/*".to_string(), DeploymentSection::default()),
  ])
  .expect("the sections are valid");
  for path in ["/service", "/service/", "/b", "a/b", "/some/x", "/some/", "/", "", "svc", "/undefined"] {
    let elements = split_like_java(path);
    let refs: Vec<&str> = elements.iter().map(String::as_str).collect();
    assert_eq!(deployer.lookup_path(path), deployer.lookup(&refs), "path {path:?}");
  }
}

// P6: for any number-of-instances value, from-code and group sections
// never fail on the count.
#[test]
fn deployer_from_code_and_group_sections_never_fail_on_the_count() {
  let counts = [
    NumberOfInstances::Value(-1),
    NumberOfInstances::Value(0),
    NumberOfInstances::Value(3),
    NumberOfInstances::NotANumber("boom".to_string()),
    NumberOfInstances::NotANumber("3000000000".to_string()),
  ];
  for count in counts {
    for router in [RouterSelection::FromCode, RouterSelection::RoundRobinGroup, RouterSelection::BroadcastGroup] {
      let section = DeploymentSection { router, nr_of_instances: count.clone(), ..DeploymentSection::default() };
      let built = Deployer::from_sections([("/svc".to_string(), section)]);
      assert!(built.is_ok(), "the count is not read for this kind: {count:?}");
    }
  }
}

fn resizer_section(lower_bound: u32, upper_bound: u32) -> ResizerSection {
  ResizerSection { lower_bound, upper_bound, ..ResizerSection::default() }
}

// P7: for the kinds that read the resizer, a section with a resizer
// yields Some for values that pass validation and InvalidResizer for
// values that do not, whatever the other fields hold.
#[test]
fn deployer_resizer_reading_pools_validate_the_resizer() {
  let readers = [
    RouterSelection::RoundRobinPool,
    RouterSelection::RandomPool,
    RouterSelection::SmallestMailboxPool,
    RouterSelection::BroadcastPool,
    RouterSelection::ScatterGatherPool,
    RouterSelection::TailChoppingPool,
    RouterSelection::ConsistentHashingPool,
  ];
  for router in readers {
    let good = DeploymentSection {
      router,
      resizer: Some(resizer_section(2, 5)),
      ..DeploymentSection::default()
    };
    let deployer = Deployer::from_sections([("/svc".to_string(), good)]).expect("a valid resizer builds");
    match &deployer.lookup_path("/svc").expect("deployed").router {
      | DeployedRouter::Pool { resizer, .. } => {
        let resizer = resizer.as_ref().expect("the resizer is kept");
        assert_eq!(resizer.lower_bound, 2);
        assert_eq!(resizer.upper_bound, 5);
      },
      | other => panic!("expected a pool, got {other:?}"),
    }
    let bad = DeploymentSection {
      router,
      resizer: Some(resizer_section(5, 2)),
      ..DeploymentSection::default()
    };
    let result = Deployer::from_sections([("/svc".to_string(), bad)]);
    assert!(
      matches!(result.unwrap_err(), DeployError::InvalidResizer { .. }),
      "a broken resizer is refused for {router:?}"
    );
  }
}

// P8: balancing, group and from-code sections ignore the resizer
// entirely, even a broken one, and balancing never uses the pool
// dispatcher.
#[test]
fn deployer_kinds_that_do_not_read_the_resizer_ignore_even_a_broken_one() {
  let ignored = [
    RouterSelection::BalancingPool,
    RouterSelection::RoundRobinGroup,
    RouterSelection::FromCode,
  ];
  for router in ignored {
    let section = DeploymentSection {
      router,
      resizer: Some(resizer_section(5, 2)),
      pool_dispatcher_given: true,
      ..DeploymentSection::default()
    };
    let deployer =
      Deployer::from_sections([("/svc".to_string(), section)]).expect("the resizer is not read for this kind");
    let found = deployer.lookup_path("/svc").expect("deployed");
    match &found.router {
      | DeployedRouter::Pool { kind, resizer, use_pool_dispatcher, .. } => {
        assert_eq!(*kind, PoolKind::Balancing);
        assert_eq!(*resizer, None);
        assert!(!use_pool_dispatcher, "balancing never uses the pool dispatcher");
      },
      | DeployedRouter::Group { .. } => {},
      | DeployedRouter::NoRouter => {},
    }
  }
}

// The scatter-gather within and the tail-chopping interval come from
// the section, as the original's Config constructors read them.
#[test]
fn deployer_pool_kinds_carry_the_section_values_the_original_reads() {
  let section = DeploymentSection {
    router: RouterSelection::TailChoppingPool,
    within: Duration::from_secs(3),
    tail_chopping_interval: Duration::from_millis(25),
    ..DeploymentSection::default()
  };
  let deployer = Deployer::from_sections([("/svc".to_string(), section)]).expect("valid");
  let found = deployer.lookup_path("/svc").expect("deployed");
  assert_eq!(
    found.router,
    DeployedRouter::Pool {
      kind: PoolKind::TailChopping { within: Duration::from_secs(3), interval: Duration::from_millis(25) },
      nr_of_instances: 1,
      resizer: None,
      use_pool_dispatcher: false,
    }
  );
}
