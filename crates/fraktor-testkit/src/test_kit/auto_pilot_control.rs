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

//! What an auto pilot asks the test actor to do next.

/// The outcome of one auto pilot run.
///
/// Mirrors the two values of `org.apache.pekko.testkit.TestKit.TestActor` that
/// a pilot may return: `KeepRunning`, which keeps the pilot installed, and
/// `NoAutoPilot`, which removes it so later messages are only queued.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoPilotControl {
  /// Keeps the current pilot for the next message.
  KeepRunning,
  /// Removes the current pilot.
  NoAutoPilot,
}
