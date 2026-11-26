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

//! The settings of the dead letter listener.

use core::num::NonZeroU32;
use core::time::Duration;

use crate::kernel::actor::DeadLetterLogLimit;

/// The values the dead letter listener runs with.
///
/// Mirrors the two settings Pekko's `DeadLetterListener` reads:
/// `pekko.log-dead-letters` and `pekko.log-dead-letters-suspend-duration`.
/// The port has no configuration layer for them, so the values are handed
/// to
/// [`ActorSystem::start_dead_letter_listener`](crate::kernel::actor_system::ActorSystem::start_dead_letter_listener)
/// directly. Pekko's default suspend duration is 5 minutes; the default
/// here states it.
#[derive(Clone, Copy)]
pub struct DeadLetterSettings {
  limit: DeadLetterLogLimit,
  suspend_duration: Option<Duration>,
}

impl DeadLetterSettings {
  /// Creates the settings with a log `limit` and an optional suspend
  /// duration.
  ///
  /// Without a suspend duration the listener stops itself when it reaches
  /// the limit, as Pekko does when the setting is not a finite duration.
  pub fn new(limit: DeadLetterLogLimit, suspend_duration: Option<Duration>) -> Self {
    Self { limit, suspend_duration }
  }

  /// Returns the settings Pekko's defaults describe: a limit of 10 and a
  /// suspend duration of 5 minutes.
  pub fn defaults() -> Self {
    Self {
      limit: DeadLetterLogLimit::Count(NonZeroU32::new(10).expect("10 is not zero")),
      suspend_duration: Some(Duration::from_secs(5 * 60)),
    }
  }

  /// Returns the log limit.
  pub fn limit(&self) -> DeadLetterLogLimit {
    self.limit
  }

  /// Returns the suspend duration, when one is set.
  pub fn suspend_duration(&self) -> Option<Duration> {
    self.suspend_duration
  }
}
