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

//! The message that applies an operation to each current subscriber.

use crate::kernel::actor_ref::ActorRef;
use alloc::boxed::Box;

/// Runs a closure once for each actor in the broadcaster's subscriber set.
///
/// Mirrors `org.apache.pekko.routing.WithListeners`. The closure decides what
/// to do with each subscriber, so the test names the operation while the
/// broadcaster owns the set. The closure must be `Send + Sync + 'static`
/// because it travels as a message payload and outlives the call that sends it.
pub struct WithSubscribers {
  operation: Box<dyn Fn(&ActorRef) + Send + Sync + 'static>,
}

impl WithSubscribers {
  /// Creates a message that applies `operation` to each subscriber.
  pub fn new<F>(operation: F) -> Self
  where
    F: Fn(&ActorRef) + Send + Sync + 'static,
  {
    Self { operation: Box::new(operation) }
  }

  pub(crate) fn apply(&self, subscriber: &ActorRef) {
    (self.operation)(subscriber);
  }
}
