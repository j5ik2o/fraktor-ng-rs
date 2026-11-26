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

//! The marker that a log event may carry.

/// A marker attached to a log event.
///
/// Mirrors `org.apache.pekko.event.LogMarker`, of which only `Security` exists
/// in this milestone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogMarker {
  /// The marker Pekko publishes as `LogMarker.Security`.
  Security,
}

impl LogMarker {
  /// Returns the name the marker reports.
  ///
  /// Mirrors `LogMarker.Security.name`, which is the `name` of the user-defined
  /// marker it stands for.
  pub const fn name(&self) -> &'static str {
    match *self {
      | Self::Security => "SECURITY",
    }
  }
}
