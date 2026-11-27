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

//! The startup and shutdown hooks of a spec.

use crate::test_kit::TestKit;

/// The points at which a spec may run code around the lifetime of its actor
/// system.
///
/// Mirrors the overridable methods of `org.apache.pekko.testkit.PekkoSpec`:
/// `atStartup` runs inside `beforeAll`, and `beforeTermination` and
/// `afterTermination` run inside `afterAll` around `shutdown`. Pekko inherits
/// and overrides them; the port takes the overrides as one object because a
/// Rust type cannot inherit from `FraktorSpec` and override its methods.
pub trait FraktorSpecHooks {
  /// Runs after the actor system and the test kit are ready.
  fn at_startup(&mut self, _test_kit: &mut TestKit) {}

  /// Runs while the actor system is still running, before it is shut down.
  fn before_termination(&mut self, _test_kit: &mut TestKit) {}

  /// Runs after the actor system has been shut down.
  fn after_termination(&mut self, _test_kit: &mut TestKit) {}
}
