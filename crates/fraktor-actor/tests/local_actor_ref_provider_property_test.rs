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

// The wiring property for name validation of issue #194
// (docs/design/issue-194-specs.md section 6, property 2): for random
// names, every creation entry point succeeds exactly when the name is
// valid, and a refused creation creates no actor. The oracle is
// `ActorPath::validate_path_element`, which this issue does not
// change; what is under test is that the entry points consult it.

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_path::ActorPath;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

struct SplitMix64(u64);

impl SplitMix64 {
  fn next(&mut self) -> u64 {
    self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.0;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }
}

fn random_name(rng: &mut SplitMix64) -> String {
  const PIECES: &[&str] = &[
    "a", "B", "3", "-", "_", ".", "*", "$", "+", ":", "@", "&", "=", ",", "!", "~", "'", ";", "]", "%", "%3A", "%2f",
    "%zz", "?", "ü", "ß", " ", "#", "/", "\"", "<", ">", "(", ")",
  ];
  let count = (rng.next() % 5) as usize;
  let mut name = String::new();
  for _ in 0..count {
    name.push_str(PIECES[(rng.next() as usize) % PIECES.len()]);
  }
  name
}

struct NamedMaker {
  reply_to: ActorRef,
}

impl Actor for NamedMaker {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(name) = message.downcast_ref::<String>() {
      let result = context.actor_of(Props::empty(), name);
      let reply_to = self.reply_to.clone();
      reply_to.tell(result.is_ok(), context.self_ref());
    }
  }
}

#[test]
fn every_entry_point_accepts_exactly_the_valid_names() {
  let mut spec = FraktorSpec::new("LocalActorRefProviderProperty");
  let mut probe = TestProbe::new(spec.system_mut());
  let probe_ref = probe.ref_handle();
  let maker = spec.system_mut().actor_of(Props::new(move || NamedMaker { reply_to: probe_ref.clone() }));
  let sender = spec.system().dead_letters();

  let mut rng = SplitMix64(0x194);
  let mut checked = 0usize;
  for index in 0..300 {
    let raw = random_name(&mut rng);
    // Names with '/' or '#' cannot be path elements at all: the
    // path constructor itself refuses them, so they are outside the
    // property's input space.
    if raw.contains('/') || raw.contains('#') {
      continue;
    }
    // Make each name unique per iteration so a valid name is not
    // refused for being taken by an earlier iteration.
    let name = if ActorPath::validate_path_element(&raw).is_ok() && !raw.is_empty() {
      format!("{raw}-{index}")
    } else {
      raw.clone()
    };
    let valid = !name.is_empty() && ActorPath::validate_path_element(&name).is_ok();
    checked += 1;

    // Entry point 1: the system, by name.
    let system_result = spec.system_mut().actor_of_named(Props::empty(), name.as_str());
    assert_eq!(
      system_result.is_ok(),
      valid,
      "system entry point disagrees for name {name:?} (seed 0x194, index {index})"
    );
    if valid {
      let created = system_result.expect("checked above");
      assert_eq!(created.path().name(), name);
    }

    // Entry point 2: a parent, by name, inside a delivery.
    maker.tell(name.clone(), sender.clone());
    let parent_ok: bool = probe.expect_msg_type();
    assert_eq!(parent_ok, valid, "parent entry point disagrees for name {name:?} (seed 0x194, index {index})");
  }
  assert!(checked > 200, "the generator produced too few usable names: {checked}");
  spec.terminate();
}
