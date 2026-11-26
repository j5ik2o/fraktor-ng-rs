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

//! A part of the setup an actor system is created with.

use core::any::Any;

use crate::kernel::setup::ActorSystemSetup;

/// A setup part that can be put inside an [`ActorSystemSetup`].
///
/// Mirrors `org.apache.pekko.actor.setup.Setup`. The concrete type of a setup
/// is its key: an [`ActorSystemSetup`] holds at most one value of each type.
/// No setup is mandatory, so a system created without one uses its defaults.
pub trait Setup: Any + Send + Sync {
  /// Returns a setup holding this setup and `other`, which replaces this one
  /// when both have the same concrete type.
  ///
  /// Mirrors `Setup.and`, which allows `a and b and c` together with
  /// [`ActorSystemSetup::and`].
  fn and<S: Setup>(self, other: S) -> ActorSystemSetup
  where
    Self: Sized,
  {
    ActorSystemSetup::empty().with_setup(self).with_setup(other)
  }
}
