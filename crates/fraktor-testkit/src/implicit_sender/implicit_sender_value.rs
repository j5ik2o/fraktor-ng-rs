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

//! The sender a test uses when it sends a message without naming one.

use fraktor_actor::kernel::actor_ref::ActorRef;

use crate::fraktorspec::FraktorSpec;

/// Provides the sender that a test uses implicitly.
///
/// Mirrors `org.apache.pekko.testkit.ImplicitSender`, whose
/// `implicit def self: ActorRef` is `testActor`. The Rust port cannot mix a
/// trait into a test type implicitly, so the handle is produced by a function
/// that takes the spec of the test.
pub struct ImplicitSender;

impl ImplicitSender {
  /// Returns the test actor of the given spec, which is the implicit sender.
  pub fn self_actor_ref(spec: &FraktorSpec) -> ActorRef {
    spec.test_actor()
  }
}
