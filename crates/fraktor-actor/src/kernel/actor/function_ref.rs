// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2018-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright (C) 2014-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! A handle to a function registered on an actor's cell.
//!
//! Mirrors `org.apache.pekko.actor.FunctionRef`: sending runs the
//! function while the ref is registered, and removing the ref or
//! stopping its parent ends it.

use core::any::Any;

use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::message::Message;

#[cfg(test)]
#[path = "function_ref_test.rs"]
mod tests;

/// A handle to a function that an actor registered on its own cell with
/// [`ActorContext::add_function_ref`](crate::kernel::actor::ActorContext::add_function_ref).
///
/// Mirrors `org.apache.pekko.actor.FunctionRef`: messages sent to it run
/// the function, it can be watched like an actor, and it ends when its
/// parent actor ends or when the parent removes it. The handle is a
/// newtype over [`ActorRef`] and holds nothing else — in particular no
/// reference to the executor — so it is `Send + Sync` and can travel as
/// a message, as the original's cases send it. The operations that need
/// the executor (`watch`, `unwatch`, `isWatching` in the original) live
/// on [`ActorSystem`](crate::kernel::actor_system::ActorSystem) and
/// [`ActorContext`](crate::kernel::actor::ActorContext), which lend the
/// executor at the call site; this keeps the handle independent of the
/// executor's `Rc`/`Arc` choice. `Copy` is not derived because
/// `ActorRef` is not `Copy`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct FunctionRef {
  inner: ActorRef,
}

impl FunctionRef {
  pub(crate) fn new(inner: ActorRef) -> Self {
    Self { inner }
  }

  /// Returns the handle as a plain actor reference, for the places
  /// that address it as a destination or watch it as one.
  pub fn as_actor_ref(&self) -> ActorRef {
    self.inner.clone()
  }

  /// Returns the path of the function reference, below its parent.
  pub fn path(&self) -> ActorPath {
    self.inner.path()
  }

  /// Sends `message` to the function, naming `sender` as its sender.
  ///
  /// Mirrors `ActorRef.tell` for the function reference: while it is
  /// registered the function runs; after it is removed the message
  /// goes to dead letters.
  pub fn tell<M: Any + Send + Sync>(&self, message: M, sender: ActorRef) {
    self.inner.tell(message, sender);
  }

  /// Forwards `message` keeping the sender of the current delivery, as
  /// [`ActorRef::forward`] does.
  pub fn forward(&self, message: Message, context: &mut crate::kernel::actor::ActorContext<'_>) {
    self.inner.forward(message, context);
  }
}
