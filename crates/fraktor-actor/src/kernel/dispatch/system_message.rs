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

use alloc::string::String;

use crate::kernel::{actor_ref::ActorRef, supervision::FailureKind};

/// A failure of one actor, which the `Failed` system message carries to
/// its parent.
///
/// Mirrors `org.apache.pekko.dispatch.sysmsg.Failed`: it keeps what the
/// parent's decision reads of the child. The number of restarts the child
/// has already used and the strategy it was started under live in the
/// parent's table, and the sender of the failed message lives in the
/// child's cell, so neither rides here.
#[derive(Clone, Debug)]
pub(crate) struct Failure {
  /// The child that failed.
  pub(crate) child: ActorRef,
  /// The kind of the failure.
  pub(crate) kind: FailureKind,
  /// The name of the cause the failure is reported under.
  pub(crate) cause_name: &'static str,
  /// The message of the failure.
  pub(crate) message: String,
}

/// What a restart was for, which the `Recreate` system message carries to
/// the child it restarts.
///
/// Mirrors the cause `org.apache.pekko.dispatch.sysmsg.Recreate` carries.
#[derive(Clone, Debug)]
pub(crate) struct RestartCause {
  /// The name of the cause the failure was reported under.
  pub(crate) cause_name: &'static str,
  /// The message of the failure.
  pub(crate) message: String,
}

impl RestartCause {
  /// Returns the cause of a restart that answers `failure`.
  pub(crate) fn of(failure: &Failure) -> Self {
    Self { cause_name: failure.cause_name, message: failure.message.clone() }
  }
}

/// A message that drives the life of an actor rather than its behaviour.
///
/// Mirrors `org.apache.pekko.dispatch.sysmsg.SystemMessage`. A mailbox
/// keeps these in a queue of their own, and a run handles all that are
/// waiting before each normal message. A system message left in a
/// mailbox that closes, or sent to a closed one, goes to dead letters under
/// the dead letter rules: a suppressed kind as a `SuppressedDeadLetter`, and
/// every other kind as a `DeadLetter`.
#[derive(Clone, Debug)]
pub(crate) enum SystemMessage {
  /// Start the actor (Pekko's `Create`).
  Create,
  /// Suspend the actor, and pass the suspend on to its children (Pekko's
  /// `Suspend`).
  Suspend,
  /// Undo one suspend of the mailbox (Pekko's `Resume`).
  Resume {
    /// Whether this resume answers a failure: the child whose failure it
    /// answers has its failure record cleared, the others do not.
    caused_by_failure: bool,
  },
  /// Restart the actor for the cause the message carries (Pekko's
  /// `Recreate`).
  Recreate {
    /// What the restart was for.
    cause: RestartCause,
  },
  /// A child has failed (Pekko's `Failed`).
  Failed(Failure),
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
      | Self::Suspend => "fraktor_actor::kernel::dispatch::system_message::Suspend",
      | Self::Resume { .. } => "fraktor_actor::kernel::dispatch::system_message::Resume",
      | Self::Recreate { .. } => "fraktor_actor::kernel::dispatch::system_message::Recreate",
      | Self::Failed(_) => "fraktor_actor::kernel::dispatch::system_message::Failed",
      | Self::Terminate => "fraktor_actor::kernel::dispatch::system_message::Terminate",
      | Self::ChildTerminated { .. } => "fraktor_actor::kernel::dispatch::system_message::ChildTerminated",
    }
  }
}
