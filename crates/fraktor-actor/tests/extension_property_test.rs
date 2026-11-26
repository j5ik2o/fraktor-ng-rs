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

// Property-based tests of the extension registry (the design of #193,
// docs/design/issue-193-specs.md §5, P1 and P2).
//
// These are not ports of Pekko cases and are not counted in the ledger. The
// inputs come from a fixed-seed SplitMix64, so a failure repeats; every
// failure message names the seed. The counters travel inside the id
// values, so trials and parallel tests do not contaminate each other.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::extension::{Extension, ExtensionError, ExtensionId, ExtensionStartup};

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

  fn below(&mut self, bound: usize) -> usize {
    (self.next_u64() % bound as u64) as usize
  }
}

#[derive(Debug)]
struct CountingExtension;

impl Extension for CountingExtension {}

/// A succeeding id. The const parameter keeps the id types distinct, so
/// one system can hold several of them under different keys.
#[derive(Clone)]
struct CountingId<const N: usize> {
  creations: Arc<AtomicUsize>,
}

impl<const N: usize> CountingId<N> {
  fn new() -> Self {
    Self { creations: Arc::new(AtomicUsize::new(0)) }
  }

  fn creations(&self) -> usize {
    self.creations.load(Ordering::SeqCst)
  }
}

impl<const N: usize> ExtensionId for CountingId<N> {
  type Ext = CountingExtension;

  fn create_extension(&self, _system: &mut ActorSystem) -> Result<CountingExtension, ExtensionError> {
    self.creations.fetch_add(1, Ordering::SeqCst);
    Ok(CountingExtension)
  }
}

#[derive(Debug)]
struct FailingExtension;

impl Extension for FailingExtension {}

/// A failing id: creation always returns the same error.
#[derive(Clone)]
struct FailingId {
  creations: Arc<AtomicUsize>,
}

impl FailingId {
  fn new() -> Self {
    Self { creations: Arc::new(AtomicUsize::new(0)) }
  }

  fn creations(&self) -> usize {
    self.creations.load(Ordering::SeqCst)
  }
}

impl ExtensionId for FailingId {
  type Ext = FailingExtension;

  fn create_extension(&self, _system: &mut ActorSystem) -> Result<FailingExtension, ExtensionError> {
    self.creations.fetch_add(1, Ordering::SeqCst);
    Err(ExtensionError::new("PropertyFailure", "always fails"))
  }
}

fn boundary() -> Arc<ChannelBoundary> {
  Arc::new(ChannelBoundary::new())
}

// P1: for any sequence of registrations and lookups mixing succeeding and
// failing ids, each id is created at most once, a succeeding id always
// returns the same `Arc`, and a failing id always returns an equal `Err`.
#[test]
fn mixed_requests_create_at_most_once_and_agree() {
  for trial in 0..200u64 {
    let seed = 0x193_0001 + trial;
    let mut rng = SplitMix64::new(seed);
    let first = CountingId::<1>::new();
    let second = CountingId::<2>::new();
    let failing = FailingId::new();
    let mut system = ActorSystem::new("property-p1", boundary());
    let mut first_instance: Option<Arc<CountingExtension>> = None;
    let mut second_instance: Option<Arc<CountingExtension>> = None;
    let mut failure: Option<ExtensionError> = None;

    let steps = rng.below(33);
    for _ in 0..steps {
      match rng.below(6) {
        | 0 => {
          let got = system.register_extension(&first).expect("the first id succeeds");
          match &first_instance {
            | Some(kept) => assert!(Arc::ptr_eq(kept, &got), "seed {seed}: the first id changed instance"),
            | None => first_instance = Some(got),
          }
        },
        | 1 => {
          let got = system.register_extension(&second).expect("the second id succeeds");
          match &second_instance {
            | Some(kept) => assert!(Arc::ptr_eq(kept, &got), "seed {seed}: the second id changed instance"),
            | None => second_instance = Some(got),
          }
        },
        | 2 => {
          let error = system.register_extension(&failing).expect_err("the failing id fails");
          match &failure {
            | Some(kept) => assert_eq!(&error, kept, "seed {seed}: the failing id changed its error"),
            | None => failure = Some(error),
          }
        },
        | 3 => match (&first_instance, system.extension(&first)) {
          | (Some(kept), Ok(got)) => assert!(Arc::ptr_eq(kept, &got), "seed {seed}: lookup disagrees"),
          | (None, Err(_)) => {},
          | _ => panic!("seed {seed}: lookup of the first id disagrees with registration"),
        },
        | 4 => match (&second_instance, system.extension(&second)) {
          | (Some(kept), Ok(got)) => assert!(Arc::ptr_eq(kept, &got), "seed {seed}: lookup disagrees"),
          | (None, Err(_)) => {},
          | _ => panic!("seed {seed}: lookup of the second id disagrees with registration"),
        },
        | _ => match (&failure, system.extension(&failing)) {
          | (Some(kept), Err(error)) => assert_eq!(&error, kept, "seed {seed}: lookup changed the failure"),
          | (None, Err(_)) => {},
          | _ => panic!("seed {seed}: lookup of the failing id disagrees with registration"),
        },
      }
    }

    assert_eq!(first.creations(), usize::from(first_instance.is_some()), "seed {seed}: first creations");
    assert_eq!(second.creations(), usize::from(second_instance.is_some()), "seed {seed}: second creations");
    assert_eq!(failing.creations(), usize::from(failure.is_some()), "seed {seed}: failing creations");
    assert_eq!(system.has_extension(&first), first_instance.is_some(), "seed {seed}");
    assert_eq!(system.has_extension(&second), second_instance.is_some(), "seed {seed}");
    assert!(!system.has_extension(&failing), "seed {seed}");

    system.terminate();
  }
}

// P2: for any startup list of succeeding ids with any duplication, each
// listed id is created exactly once, and `has_extension` is true exactly
// for the ids that appeared in the list.
#[test]
fn startup_lists_create_each_listed_id_once() {
  for trial in 0..200u64 {
    let seed = 0x193_0002 + trial;
    let mut rng = SplitMix64::new(seed);
    let first = CountingId::<1>::new();
    let second = CountingId::<2>::new();
    let absent = CountingId::<3>::new();

    let mut startup = ExtensionStartup::new();
    let mut listed_first = false;
    let mut listed_second = false;
    let steps = rng.below(33);
    for _ in 0..steps {
      match rng.below(4) {
        | 0 => {
          startup = startup.extension(first.clone());
          listed_first = true;
        },
        | 1 => {
          startup = startup.extension(second.clone());
          listed_second = true;
        },
        | 2 => {
          startup = startup.library_extension(first.clone());
          listed_first = true;
        },
        | _ => {
          startup = startup.library_extension(second.clone());
          listed_second = true;
        },
      }
    }

    let mut system = ActorSystem::with_extensions("property-p2", boundary(), startup).expect("startup succeeds");
    assert_eq!(first.creations(), usize::from(listed_first), "seed {seed}: first creations");
    assert_eq!(second.creations(), usize::from(listed_second), "seed {seed}: second creations");
    assert_eq!(absent.creations(), 0, "seed {seed}: the absent id was created");
    assert_eq!(system.has_extension(&first), listed_first, "seed {seed}");
    assert_eq!(system.has_extension(&second), listed_second, "seed {seed}");
    assert!(!system.has_extension(&absent), "seed {seed}");

    system.terminate();
  }
}
