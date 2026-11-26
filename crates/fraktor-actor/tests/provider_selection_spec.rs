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

// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/actor/ProviderSelectionSpec.scala
//
// The original's four cases each check three things: the selection's
// `fqcn`, its `hasCluster`, and that the `ProviderClass` the `Settings`
// read from the configuration string agrees. The port has no configuration
// file, so the third check is replaced by the typed entry
// `ProviderSelection::from_setting`, which copies the original's
// `ProviderSelection.apply` (design issue-192-specs.md section 1.1 and the
// faithfulness note E1 in section 5).

use fraktor_actor::kernel::actor::{
  ProviderSelection, CLUSTER_PROVIDER_FQCN, LOCAL_PROVIDER_FQCN, REMOTE_PROVIDER_FQCN,
};

// Original: "create a Local ProviderSelection and set local provider fqcn in Settings"
#[test]
fn provider_selection_local_has_local_fqcn_and_no_cluster() {
  let selection = ProviderSelection::from_setting("local");
  assert_eq!(selection, ProviderSelection::Local);
  assert_eq!(selection.fqcn(), LOCAL_PROVIDER_FQCN);
  assert!(!selection.has_cluster());
}

// Original: "create a Remote ProviderSelection and set remote provider fqcn in Settings"
#[test]
fn provider_selection_remote_has_remote_fqcn_and_no_cluster() {
  let by_key = ProviderSelection::from_setting("remote");
  let by_fqcn = ProviderSelection::from_setting(REMOTE_PROVIDER_FQCN);
  assert_eq!(by_key, ProviderSelection::Remote);
  assert_eq!(by_fqcn, ProviderSelection::Remote);
  assert_eq!(by_key.fqcn(), REMOTE_PROVIDER_FQCN);
  assert!(!by_key.has_cluster());
}

// Original: "create a Cluster ProviderSelection and set cluster provider fqcn in Settings"
#[test]
fn provider_selection_cluster_has_cluster_fqcn_and_has_cluster() {
  let by_key = ProviderSelection::from_setting("cluster");
  let by_fqcn = ProviderSelection::from_setting(CLUSTER_PROVIDER_FQCN);
  assert_eq!(by_key, ProviderSelection::Cluster);
  assert_eq!(by_fqcn, ProviderSelection::Cluster);
  assert_eq!(by_key.fqcn(), CLUSTER_PROVIDER_FQCN);
  assert!(by_key.has_cluster());
}

// Original: "create a Custom ProviderSelection and set custom provider fqcn in Settings"
#[test]
fn provider_selection_custom_keeps_the_given_fqcn_and_has_no_cluster() {
  let selection = ProviderSelection::from_setting("other.ActorRefProvider");
  assert_eq!(selection, ProviderSelection::Custom(String::from("other.ActorRefProvider")));
  assert_eq!(selection.fqcn(), "other.ActorRefProvider");
  assert!(!selection.has_cluster());
  // The original's asymmetry: the Local provider's fqcn is not a key of
  // `apply`, so it becomes a Custom selection rather than Local.
  assert_eq!(
    ProviderSelection::from_setting(LOCAL_PROVIDER_FQCN),
    ProviderSelection::Custom(String::from(LOCAL_PROVIDER_FQCN))
  );
}
