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

//! The message that removes an actor from a broadcaster's subscriber set.

use crate::kernel::actor_ref::ActorRef;

/// Asks the broadcaster to remove its actor from the set it delivers to.
///
/// Mirrors `org.apache.pekko.routing.Deafen`. The removal takes the entry for
/// the actor out of the set; it does not stop the actor, which stays alive and
/// simply receives no further deliveries.
pub struct Unsubscribe {
  actor: ActorRef,
}

impl Unsubscribe {
  /// Creates a message that removes `actor` from the set of the broadcaster it
  /// reaches.
  pub fn new(actor: ActorRef) -> Self {
    Self { actor }
  }

  /// Returns the actor to remove.
  pub fn actor(&self) -> &ActorRef {
    &self.actor
  }
}
