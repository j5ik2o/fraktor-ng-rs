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

//! The messages between actors that the kernel itself sends.

use crate::kernel::actor_ref::ActorRef;

/// A message that drives the life of an actor rather than its behaviour.
///
/// Mirrors `org.apache.pekko.dispatch.sysmsg.SystemMessage` and the cases that
/// stage 4a of #234 sends (design of #234, section 11.12.9): the others come
/// back in the stages that send them. A mailbox keeps these in a queue of
/// their own, and a run handles all that are waiting before each normal
/// message (design of #234, invariant I11). A system message left in a
/// mailbox that closes, or sent to a closed one, is dropped (section 11.12.3).
#[derive(Clone, Debug)]
pub(crate) enum SystemMessage {
  /// Start the actor (Pekko's `Create`).
  Create,
  /// Undo one suspend of the mailbox (Pekko's `Resume`).
  Resume,
  /// Stop the actor (Pekko's `Terminate`).
  Terminate,
  /// A child has stopped (Pekko's `DeathWatchNotification` to the parent,
  /// which `handleChildTerminated` handles).
  ChildTerminated {
    /// The child that stopped.
    child: ActorRef,
  },
}
