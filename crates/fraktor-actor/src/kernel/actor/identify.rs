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

//! The message that asks an actor to identify itself.

use crate::kernel::message::Message;

/// The message that asks an actor for its identity.
///
/// Mirrors `org.apache.pekko.actor.Identify(messageId)`. It does not
/// influence the receive timeout. `Identify(None)` in the original is
/// written `Identify::new(Option::<()>::None)`.
#[derive(Clone, Debug)]
pub struct Identify {
  message_id: Message,
}

impl Identify {
  /// Creates the message that carries `message_id`.
  pub fn new<T: core::any::Any + Send + Sync>(message_id: T) -> Self {
    Self { message_id: Message::new(message_id) }
  }

  /// The identifier the sender attached.
  pub fn message_id(&self) -> &Message {
    &self.message_id
  }
}
