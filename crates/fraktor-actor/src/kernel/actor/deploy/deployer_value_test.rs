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
//! Unit tests for the boundaries of the deployer that the ported
//! cases do not reach (design of #195, section 3).

use super::split_deployment_path;
use crate::kernel::actor::deploy::{
  Deploy, DeployError, DeployedRouter, Deployer, DeploymentSection, NumberOfInstances, ResizerSection, RouterSelection,
  Scope,
};
use crate::kernel::wildcard_index::WildcardIndexError;

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

#[test]
fn split_follows_the_java_rules() {
  assert_eq!(split_deployment_path("/service/"), vec!["service".to_string()]);
  assert_eq!(split_deployment_path("/"), Vec::<String>::new());
  assert_eq!(split_deployment_path(""), Vec::<String>::new());
  assert_eq!(split_deployment_path("service"), Vec::<String>::new());
  assert_eq!(split_deployment_path("a/b"), vec!["b".to_string()]);
  assert_eq!(split_deployment_path("//a"), vec!["".to_string(), "a".to_string()]);
  assert_eq!(split_deployment_path("/a/b/c"), vec!["a".to_string(), "b".to_string(), "c".to_string()]);
}

#[test]
fn a_trailing_slash_key_is_found_by_its_trimmed_path() {
  let mut deployer = Deployer::from_sections([]).expect("empty");
  deployer.deploy(deploy_value("/service/")).expect("a trailing slash is not an error");
  let found = deployer.lookup_path("/service").expect("found by the trimmed path");
  assert_eq!(found.path, "/service/");
}

#[test]
fn keys_that_split_to_nothing_are_silently_not_registered() {
  let deployer = Deployer::from_sections([
    ("/".to_string(), DeploymentSection::default()),
    ("svc".to_string(), DeploymentSection::default()),
  ])
  .expect("these keys are not errors");
  assert_eq!(deployer.lookup_path("/"), None);
  assert_eq!(deployer.lookup_path("/svc"), None);
}

#[test]
fn an_empty_element_is_the_deployment_specific_error() {
  let mut deployer = Deployer::from_sections([]).expect("empty");
  let error = deployer.deploy(deploy_value("//a")).unwrap_err();
  match error {
    | DeployError::InvalidName(error) => {
      assert_eq!(error.to_string(), "Actor name in deployment [//a] must not be empty");
    },
    | other => panic!("expected an invalid name error, got {other:?}"),
  }
}

#[test]
fn a_double_wildcard_suffix_fails_at_insertion_after_validation() {
  for path in ["/a/***", "/a/a**"] {
    let mut deployer = Deployer::from_sections([]).expect("empty");
    let error = deployer.deploy(deploy_value(path)).unwrap_err();
    assert!(
      matches!(error, DeployError::Index(WildcardIndexError::DoubleWildcardAsSuffix { .. })),
      "path {path}: {error:?}"
    );
  }
}

#[test]
fn a_double_wildcard_in_the_middle_is_an_ordinary_element() {
  let mut deployer = Deployer::from_sections([]).expect("empty");
  deployer.deploy(deploy_value("/a/**/b")).expect("not an error");
  assert_eq!(deployer.lookup_path("/a/x/b"), None);
  assert!(deployer.lookup_path("/a/**/b").is_some());
}

#[test]
fn the_default_key_is_skipped_and_a_later_duplicate_wins() {
  let deployer = Deployer::from_sections([
    ("default".to_string(), DeploymentSection { dispatcher: "ignored".to_string(), ..DeploymentSection::default() }),
    ("/svc".to_string(), DeploymentSection { dispatcher: "first".to_string(), ..DeploymentSection::default() }),
    ("/svc".to_string(), DeploymentSection { dispatcher: "second".to_string(), ..DeploymentSection::default() }),
  ])
  .expect("valid sections");
  assert_eq!(deployer.lookup_path("/default"), None);
  assert_eq!(deployer.lookup_path("/svc").expect("deployed").dispatcher, "second");
}

#[test]
fn the_no_router_class_name_is_refused_like_the_original() {
  let section = DeploymentSection {
    router: RouterSelection::from_setting("org.apache.pekko.routing.NoRouter"),
    ..DeploymentSection::default()
  };
  assert_eq!(section.router, RouterSelection::Custom("org.apache.pekko.routing.NoRouter".to_string()));
  let error = Deployer::from_sections([("/svc".to_string(), section)]).unwrap_err();
  assert!(matches!(error, DeployError::UnsupportedRouter { .. }), "{error:?}");
}

#[test]
fn a_resizer_section_with_a_lower_bound_only_keeps_the_other_defaults() {
  let section = DeploymentSection {
    router: RouterSelection::RoundRobinPool,
    resizer: Some(ResizerSection { lower_bound: 2, ..ResizerSection::default() }),
    ..DeploymentSection::default()
  };
  let deployer = Deployer::from_sections([("/svc".to_string(), section)]).expect("valid");
  match &deployer.lookup_path("/svc").expect("deployed").router {
    | DeployedRouter::Pool { resizer, .. } => {
      let resizer = resizer.as_ref().expect("the resizer is on");
      assert_eq!(resizer.lower_bound, 2);
      assert_eq!(resizer.upper_bound, 10);
    },
    | other => panic!("expected a pool, got {other:?}"),
  }
}

#[test]
fn a_negative_count_is_held_as_written() {
  let section = DeploymentSection {
    router: RouterSelection::RoundRobinPool,
    nr_of_instances: NumberOfInstances::Value(-1),
    ..DeploymentSection::default()
  };
  let deployer = Deployer::from_sections([("/svc".to_string(), section)]).expect("a negative count builds");
  match &deployer.lookup_path("/svc").expect("deployed").router {
    | DeployedRouter::Pool { nr_of_instances, .. } => assert_eq!(*nr_of_instances, -1),
    | other => panic!("expected a pool, got {other:?}"),
  }
}
