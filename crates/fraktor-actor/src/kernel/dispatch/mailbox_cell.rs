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

//! What a mailbox runs: the actor's side of a run.

use crate::kernel::dispatch::Mailbox;
use crate::kernel::dispatch::Next;
use crate::kernel::dispatch::SystemMessage;
use crate::kernel::message::Envelope;

/// The actor that a mailbox holds and a run hands messages to.
///
/// Mirrors the two entry points of `org.apache.pekko.actor.ActorCell` that
/// `Mailbox.run` calls: `invoke` and `systemInvoke`. A run takes the cell out
/// of its mailbox, calls these outside every lock, and puts the cell back
/// (design of #234, section 3.2, invariant I16). The cell changes the mailbox
/// through the `&Mailbox` it is given: it suspends and resumes it, and closes
/// it when the actor stops.
///
/// A run lends the cell what it needs to handle a message, as `env`: the
/// executor of the actor system, while one lock still guards it (design of
/// #234, section 11.11.2).
pub(crate) trait MailboxCell: Send + Sized {
  /// What a run lends the cell while it handles messages.
  type Env: ?Sized;

  /// Handles one normal message and says whether the run may take the next.
  fn invoke(&mut self, env: &mut Self::Env, mailbox: &Mailbox<Self>, envelope: Envelope) -> Next;

  /// Handles one system message.
  fn system_invoke(&mut self, env: &mut Self::Env, mailbox: &Mailbox<Self>, message: SystemMessage);
}
