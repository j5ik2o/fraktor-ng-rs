// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2017-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! What a supervisor decides for a failed child.

/// The action a supervisor takes for a child that threw while handling a
/// message.
///
/// Mirrors the subset of `org.apache.pekko.actor.SupervisorStrategy.Directive`
/// that the test probe needs: it restarts the child or stops it. Resuming and
/// escalating belong to later goals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Directive {
  /// Re-create the child from its recipe and run its `post_restart`.
  Restart,
  /// Stop the child and tell its watchers.
  Stop,
}
