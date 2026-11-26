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

//! The implicit timeout a test gets from its settings.

use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::timeout::Timeout;

use crate::test_kit_extension::TestKitSettings;

/// Provides the timeout that a test uses when none is given explicitly.
///
/// Mirrors `org.apache.pekko.testkit.DefaultTimeout`, whose
/// `implicit val timeout` is `testKitSettings.DefaultTimeout`. The Rust port
/// cannot mix a trait into a test type implicitly, so the value is produced by
/// a function that takes the actor system of the test.
pub struct DefaultTimeout;

impl DefaultTimeout {
  /// Returns the `DefaultTimeout` of the settings of the given system.
  pub fn timeout(system: &ActorSystem) -> Timeout {
    TestKitSettings::from_system(system).default_timeout()
  }
}
