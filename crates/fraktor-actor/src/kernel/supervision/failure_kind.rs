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

//! The kind of failure a child raised.

/// The kind of failure a child raised, as the default decider distinguishes it.
///
/// Mirrors the parts of `org.apache.pekko.actor.FaultHandling.defaultDecider`
/// that the strategies of this goal need: a failure during startup maps to
/// `Stop`, a `Kill` maps to `Stop`, and any other failure maps to `Restart`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
  /// The child failed while it was being created or in `preStart`, which Pekko
  /// wraps in `ActorInitializationException`.
  Initialization,
  /// The child received `Kill`, which Pekko raises as `ActorKilledException`.
  Killed,
  /// The child threw while handling a message.
  MessageHandling,
}
