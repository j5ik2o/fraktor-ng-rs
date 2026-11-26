// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2023 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Property-based tests of ProviderSelection (the design of #192,
// docs/design/issue-192-specs.md section 3, S1-S4). Arbitrary strings
// Mirrors the semantics of Pekko's `org.apache.pekko.actor.ProviderSelection`.
// are drawn from a fixed-seed SplitMix64 (no new dependency, as in the
// other property tests of this crate), mixing the built-in keys, the
// three provider class names, case and whitespace variants, the empty
// string, long strings, and non-ASCII text. These are not ports of
// Pekko cases and are not counted in the ledger.

use fraktor_actor::kernel::actor::{
  ProviderSelection, CLUSTER_PROVIDER_FQCN, LOCAL_PROVIDER_FQCN, REMOTE_PROVIDER_FQCN,
};

struct SplitMix64 {
  state: u64,
}

impl SplitMix64 {
  fn new(seed: u64) -> Self {
    Self { state: seed }
  }

  fn next_u64(&mut self) -> u64 {
    self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }

  fn below(&mut self, bound: u64) -> u64 {
    self.next_u64() % bound
  }
}

const FRAGMENTS: [&str; 16] = [
  "local",
  "remote",
  "cluster",
  LOCAL_PROVIDER_FQCN,
  REMOTE_PROVIDER_FQCN,
  CLUSTER_PROVIDER_FQCN,
  "LOCAL",
  " local",
  "local ",
  "",
  "custom",
  "other.ActorRefProvider",
  "cl*",
  "*",
  "a..b",
  "プロバイダ",
];

fn draw_string(rng: &mut SplitMix64) -> String {
  match rng.below(4) {
    | 0 => String::from(FRAGMENTS[rng.below(FRAGMENTS.len() as u64) as usize]),
    | 1 => {
      let mut text = String::from(FRAGMENTS[rng.below(FRAGMENTS.len() as u64) as usize]);
      text.push_str(FRAGMENTS[rng.below(FRAGMENTS.len() as u64) as usize]);
      text
    },
    | 2 => "x".repeat(rng.below(10_000) as usize),
    | _ => {
      let mut text = String::new();
      for _ in 0..rng.below(12) {
        text.push(char::from_u32(0x61 + (rng.below(26) as u32)).expect("a latin letter"));
      }
      text
    },
  }
}

// The independent model of the original's `apply`, written from the
// design's correspondence table rather than from the code under test.
fn model(value: &str) -> ProviderSelection {
  match value {
    | "local" => ProviderSelection::Local,
    | "remote" | REMOTE_PROVIDER_FQCN => ProviderSelection::Remote,
    | "cluster" | CLUSTER_PROVIDER_FQCN => ProviderSelection::Cluster,
    | other => ProviderSelection::Custom(String::from(other)),
  }
}

#[test]
fn provider_selection_matches_the_model_for_arbitrary_strings() {
  let seed = 0x192_5EED_u64;
  let mut rng = SplitMix64::new(seed);
  for step in 0..2_000 {
    let input = draw_string(&mut rng);
    let selection = ProviderSelection::from_setting(&input);
    assert_eq!(selection, model(&input), "seed {seed} step {step} input {input:?}");
    // S1/S3: the fqcn of a built-in selection is its constant, and a
    // Custom keeps its string unchanged.
    match &selection {
      | ProviderSelection::Local => assert_eq!(selection.fqcn(), LOCAL_PROVIDER_FQCN),
      | ProviderSelection::Remote => assert_eq!(selection.fqcn(), REMOTE_PROVIDER_FQCN),
      | ProviderSelection::Cluster => assert_eq!(selection.fqcn(), CLUSTER_PROVIDER_FQCN),
      | ProviderSelection::Custom(name) => assert_eq!(name, &input),
    }
    // S2: has_cluster is true only for Cluster.
    assert_eq!(selection.has_cluster(), selection == ProviderSelection::Cluster, "seed {seed} step {step}");
    // S4: value equality, and a Custom holding a key is not the built-in.
    assert_eq!(selection, ProviderSelection::from_setting(&input));
    if input == "local" {
      assert_ne!(ProviderSelection::Custom(input.clone()), ProviderSelection::Local);
    }
  }
}
