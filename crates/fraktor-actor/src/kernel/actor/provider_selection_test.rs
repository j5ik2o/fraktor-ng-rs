// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2019-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! Unit tests for [`ProviderSelection`](super::ProviderSelection).
//!
//! Mirrors the cases of Pekko's `ProviderSelectionSpec` that need no
//! actor system (design of #192, section 2.1).

use alloc::string::String;

use super::{ProviderSelection, CLUSTER_PROVIDER_FQCN, LOCAL_PROVIDER_FQCN, REMOTE_PROVIDER_FQCN};

#[test]
fn from_setting_copies_the_original_apply_for_the_twelve_review_inputs() {
  let cases: [(&str, ProviderSelection); 12] = [
    ("local", ProviderSelection::Local),
    ("remote", ProviderSelection::Remote),
    ("cluster", ProviderSelection::Cluster),
    (LOCAL_PROVIDER_FQCN, ProviderSelection::Custom(String::from(LOCAL_PROVIDER_FQCN))),
    (REMOTE_PROVIDER_FQCN, ProviderSelection::Remote),
    (CLUSTER_PROVIDER_FQCN, ProviderSelection::Cluster),
    ("other.ActorRefProvider", ProviderSelection::Custom(String::from("other.ActorRefProvider"))),
    ("LOCAL", ProviderSelection::Custom(String::from("LOCAL"))),
    (" local", ProviderSelection::Custom(String::from(" local"))),
    ("", ProviderSelection::Custom(String::new())),
    ("custom", ProviderSelection::Custom(String::from("custom"))),
    ("local ", ProviderSelection::Custom(String::from("local "))),
  ];
  for (input, expected) in cases {
    assert_eq!(ProviderSelection::from_setting(input), expected, "input {input:?}");
  }
}

#[test]
fn fqcn_and_has_cluster_follow_the_selection() {
  assert_eq!(ProviderSelection::Local.fqcn(), LOCAL_PROVIDER_FQCN);
  assert_eq!(ProviderSelection::Remote.fqcn(), REMOTE_PROVIDER_FQCN);
  assert_eq!(ProviderSelection::Cluster.fqcn(), CLUSTER_PROVIDER_FQCN);
  assert!(!ProviderSelection::Local.has_cluster());
  assert!(!ProviderSelection::Remote.has_cluster());
  assert!(ProviderSelection::Cluster.has_cluster());
  assert!(!ProviderSelection::Custom(String::from("cluster")).has_cluster());
}
