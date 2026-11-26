// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2013-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file changes only by 9dfca01566 #3348, 870d4499cc #2235; the cases and their inputs are
// unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1:
//   testkit/src/test/scala/org/apache/pekko/testkit/ImplicitSenderSpec.scala
//
// Original test names are kept as comments next to each ported case.

use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_testkit::{fraktorspec::FraktorSpec, implicit_sender::ImplicitSender};

const CUSTOM_SPEC_SYSTEM_NAME: &str = "PekkoCustomSpec";

struct ImplicitSenderSpecCase {
  fraktor_spec: FraktorSpec,
}

impl ImplicitSenderSpecCase {
  fn new() -> Self {
    Self { fraktor_spec: FraktorSpec::new(CUSTOM_SPEC_SYSTEM_NAME) }
  }

  fn test_actor(&self) -> ActorRef {
    self.fraktor_spec.test_actor()
  }

  fn self_actor_ref(&self) -> ActorRef {
    ImplicitSender::self_actor_ref(&self.fraktor_spec)
  }
}

impl Drop for ImplicitSenderSpecCase {
  fn drop(&mut self) {
    self.fraktor_spec.terminate();
  }
}

// "An ImplicitSender" should { "have testActor as its self" in { ... } }
#[test]
fn have_test_actor_as_its_self() {
  let spec = ImplicitSenderSpecCase::new();

  let self_actor_ref: ActorRef = spec.self_actor_ref();
  let test_actor: ActorRef = spec.test_actor();

  assert_eq!(self_actor_ref, test_actor);
}
