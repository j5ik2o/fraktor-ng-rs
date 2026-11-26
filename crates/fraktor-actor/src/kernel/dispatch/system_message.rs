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
use crate::kernel::supervision::FailureKind;

/// A message that drives the life of an actor rather than its behaviour.
///
/// Mirrors `org.apache.pekko.dispatch.sysmsg.SystemMessage` and its cases. A
/// mailbox keeps these in a queue of their own, and a run handles all that are
/// waiting before each normal message (design of #234, invariant I11). Closing
/// a mailbox sends the ones left to dead letters, and so does sending one to a
/// closed mailbox (invariant I12): dead letters answer a `Watch` with a
/// `DeathWatchNotification`.
#[derive(Clone, Debug)]
pub enum SystemMessage {
  /// Start the actor.
  Create,
  /// Stop taking normal messages until as many `Resume`s have come.
  Suspend,
  /// Undo one `Suspend`.
  Resume,
  /// Restart the actor after a failure.
  Recreate {
    /// The failure that made the parent restart the actor.
    cause: FailureKind,
  },
  /// Stop the actor.
  Terminate,
  /// The parent learns that `child` exists.
  Supervise {
    /// The new child.
    child: ActorRef,
    /// Whether the child was created asynchronously.
    is_async: bool,
  },
  /// `watcher` starts watching `watchee`.
  Watch {
    /// The actor being watched.
    watchee: ActorRef,
    /// The actor that watches.
    watcher: ActorRef,
  },
  /// `watcher` stops watching `watchee`.
  Unwatch {
    /// The actor being watched.
    watchee: ActorRef,
    /// The actor that watches.
    watcher: ActorRef,
  },
  /// A child failed and waits for the parent's decision.
  Failed {
    /// The child that failed.
    child: ActorRef,
    /// What the child failed with.
    cause: FailureKind,
  },
  /// A watched actor has stopped.
  DeathWatchNotification {
    /// The actor that stopped.
    actor: ActorRef,
    /// Whether the actor is known to have existed.
    existence_confirmed: bool,
    /// Whether the actor's address went away rather than the actor itself.
    address_terminated: bool,
  },
  /// A child has stopped.
  ChildTerminated {
    /// The child that stopped.
    child: ActorRef,
  },
}
