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

//! A message with the sender that delivered it.

use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;

/// One message the test actor received, together with its sender.
///
/// Mirrors `org.apache.pekko.testkit.TestKit.TestActor.RealMessage`, which
/// pairs the message with the sender so that `lastSender` and `reply` can
/// address the actor that just spoke.
#[derive(Clone)]
pub struct ReceivedMessage {
  message: Message,
  sender: ActorRef,
}

impl ReceivedMessage {
  pub(crate) fn new(message: Message, sender: ActorRef) -> Self {
    Self { message, sender }
  }

  /// Returns the message.
  pub fn message(&self) -> &Message {
    &self.message
  }

  /// Returns the message as the given type, or `None` if it has another type.
  pub fn downcast_ref<T: core::any::Any>(&self) -> Option<&T> {
    self.message.downcast_ref::<T>()
  }

  /// Returns the sender of the message.
  pub fn sender(&self) -> &ActorRef {
    &self.sender
  }
}
