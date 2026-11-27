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
/// the port sends so far: the others come
/// back in the stages that send them. A mailbox keeps these in a queue of
/// their own, and a run handles all that are waiting before each normal
/// message. A system message left in a
/// mailbox that closes, or sent to a closed one, goes to dead letters under
/// the dead letter rules: a suppressed kind as a `SuppressedDeadLetter`, and
/// every other kind as a `DeadLetter`.
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

impl SystemMessage {
  /// Returns the name of this message's kind, in the port's own namespace.
  ///
  /// Mirrors the `getClass.getName` a dead letter listener reads for a
  /// system message in Pekko: the port wraps the enum in a `Message`, whose
  /// type name is the enum's for every kind, so the listener reads this
  /// name instead and the kind survives into the log line.
  pub(crate) fn class_name(&self) -> &'static str {
    match self {
      | Self::Create => "fraktor_actor::kernel::dispatch::system_message::Create",
      | Self::Resume => "fraktor_actor::kernel::dispatch::system_message::Resume",
      | Self::Terminate => "fraktor_actor::kernel::dispatch::system_message::Terminate",
      | Self::ChildTerminated { .. } => "fraktor_actor::kernel::dispatch::system_message::ChildTerminated",
    }
  }
}
