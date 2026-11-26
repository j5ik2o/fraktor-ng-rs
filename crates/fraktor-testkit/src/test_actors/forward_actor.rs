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

//! An actor that forwards the messages it receives to a target.

use fraktor_actor::kernel::actor::Actor;
use fraktor_actor::kernel::actor::ActorContext;
use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;

/// Forwards each received message to a target, keeping the original sender.
///
/// Mirrors `org.apache.pekko.testkit.TestActors.ForwardActor`.
pub struct ForwardActor {
  target: ActorRef,
}

impl ForwardActor {
  /// Returns the recipe that starts a `ForwardActor` towards `target`.
  pub fn props(target: ActorRef) -> Props {
    Props::new(move || ForwardActor { target: target.clone() })
  }
}

impl Actor for ForwardActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    self.target.forward(message, context);
  }
}
