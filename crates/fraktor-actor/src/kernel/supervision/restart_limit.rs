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

//! The maximum number of restart requests a child may receive.

/// A restart allowance: a finite maximum, including zero to forbid every
/// restart, or an omitted maximum that allows every restart.
///
/// Mirrors Pekko's `maxNrOfRetriesOption`: a negative `maxNrOfRetries` is
/// omitted and every request is allowed, otherwise the value caps the requests.
#[derive(Clone, Copy)]
pub struct RestartLimit(Option<u32>);

impl RestartLimit {
  /// Creates a finite allowance. Every unsigned value is valid; zero denies
  /// the first request and positive values allow that many requests.
  pub fn new(maximum: u32) -> Self {
    Self(Some(maximum))
  }

  /// Omits the maximum. A strategy with no time window allows every restart.
  pub fn unlimited() -> Self {
    Self(None)
  }

  /// Returns whether this allowance omits a maximum.
  pub(crate) fn is_unlimited(self) -> bool {
    self.0.is_none()
  }

  /// Returns whether the limit forbids every restart.
  pub(crate) fn forbids_restart(self) -> bool {
    self.0 == Some(0)
  }

  /// Returns whether the number of requests is within this allowance. An
  /// omitted maximum permits every count.
  pub(crate) fn permits(self, requests: u32) -> bool {
    match self.0 {
      | Some(maximum) => requests <= maximum,
      | None => true,
    }
  }

  /// Returns the allowance Pekko's window algorithm applies inside a time
  /// range. Mirrors `retriesInWindowOkay(if (x.isDefined) x.get else 1, window)`:
  /// an omitted maximum becomes a single allowed restart per window.
  pub(crate) fn for_window(self) -> Self {
    match self.0 {
      | Some(maximum) => Self::new(maximum),
      | None => Self::new(1),
    }
  }
}

impl core::fmt::Debug for RestartLimit {
  fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    match self.0 {
      | Some(maximum) => core::fmt::Debug::fmt(&maximum, formatter),
      | None => formatter.write_str("unlimited"),
    }
  }
}
