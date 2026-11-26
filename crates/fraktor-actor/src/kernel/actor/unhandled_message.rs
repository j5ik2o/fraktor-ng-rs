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

//! A message that an actor could not handle.

use crate::kernel::actor_ref::ActorRef;
use crate::kernel::message::Message;

/// The event that reports a message an actor could not handle.
///
/// Mirrors `org.apache.pekko.actor.UnhandledMessage`: it carries the message,
/// the actor that sent it, and the actor that did not handle it. Pekko's event
/// stream publishes it on the `UnhandledMessage` channel, where a logger can
/// turn it into a debug event.
#[derive(Clone)]
pub struct UnhandledMessage {
  message: Message,
  sender: ActorRef,
  recipient: ActorRef,
}

impl UnhandledMessage {
  /// Creates the report of `message`, sent by `sender` to `recipient`.
  pub fn new<M: core::any::Any + Send + Sync>(message: M, sender: ActorRef, recipient: ActorRef) -> Self {
    Self { message: Message::new(message), sender, recipient }
  }

  /// Returns the message that was not handled.
  pub fn message(&self) -> &Message {
    &self.message
  }

  /// Returns the actor that sent the message.
  pub fn sender(&self) -> &ActorRef {
    &self.sender
  }

  /// Returns the actor that did not handle the message.
  pub fn recipient(&self) -> &ActorRef {
    &self.recipient
  }
}
