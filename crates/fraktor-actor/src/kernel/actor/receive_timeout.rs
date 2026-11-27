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

//! The message an actor receives when its receive timeout passes.

use core::time::Duration;

/// The message sent to an actor that received no influencing message for
/// the duration of its receive timeout.
///
/// Mirrors `org.apache.pekko.actor.ReceiveTimeout(timeout)`, which carries
/// the configured duration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReceiveTimeout {
  timeout: Duration,
}

impl ReceiveTimeout {
  /// Creates the message that carries `timeout`.
  pub fn new(timeout: Duration) -> Self {
    Self { timeout }
  }

  /// The configured duration this message carries.
  pub fn timeout(&self) -> Duration {
    self.timeout
  }
}
