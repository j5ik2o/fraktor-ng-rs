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

//! An actor that sends back the messages it receives.

use fraktor_actor::kernel::actor::Actor;
use fraktor_actor::kernel::actor::ActorContext;
use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::message::Message;

/// Sends each received message back to its sender, unchanged.
///
/// Mirrors `org.apache.pekko.testkit.TestActors.EchoActor`.
pub struct EchoActor;

impl EchoActor {
  /// Returns the recipe that starts an `EchoActor`.
  pub fn props() -> Props {
    Props::new(|| EchoActor)
  }
}

impl Actor for EchoActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let sender = context.sender();
    context.tell(&sender, message);
  }
}
