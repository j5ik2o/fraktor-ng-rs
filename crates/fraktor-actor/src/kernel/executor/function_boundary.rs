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

//! A boundary that hands each envelope to a function right away.

use alloc::sync::Arc;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;
use core::time::Duration;

use crate::kernel::actor::DeadLetter;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::event::EventStreamBusWeak;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;
use crate::kernel::message::Message;

/// The handler that a function reference runs for each delivered message.
pub(crate) type Handler = Arc<dyn Fn(&ActorRef, Message) + Send + Sync>;

/// A boundary whose shard holds no actors: a message posted to it runs a
/// handler on the spot.
///
/// Pekko's `TestActor` runs on the calling-thread dispatcher, so a message
/// handed to a test actor lands in its queue before the sender returns. The
/// port models that with a function reference: the reference shares this
/// boundary, and posting to it calls the handler instead of queueing. Nothing
/// is ever waiting here, so `poll` and `has_pending` report emptiness and the
/// handler alone decides what the message does.
///
/// `post` reads `open` once and the handler runs to its end; `close` only stops
/// a post that starts afterwards. A post that observed an open boundary before
/// the close therefore still delivers, which is what Pekko does when a message
/// is already being handled while the actor stops: the message in flight is
/// finished, and its effect on the test actor's queue stays. The same holds for
/// `ChannelBoundary`, where a post that takes the lock first is accepted.
pub(crate) struct FunctionBoundary {
  handler: Handler,
  open: AtomicBool,
  dead_letters: Option<EventStreamBusWeak>,
}

impl FunctionBoundary {
  pub(crate) fn new(handler: Handler) -> Self {
    Self { handler, open: AtomicBool::new(true), dead_letters: None }
  }

  /// Creates a boundary that, once closed, publishes a
  /// [`DeadLetter`] for every later post on the stream behind
  /// `dead_letters`, as Pekko's `deadLetterMessageHandler` receives the
  /// messages of a stopped function reference. The route is fixed at
  /// creation and read without a lock, so the post path stays as cheap
  /// as it was (re-review N3 of #192).
  pub(crate) fn new_with_dead_letters(handler: Handler, dead_letters: Option<EventStreamBusWeak>) -> Self {
    Self { handler, open: AtomicBool::new(true), dead_letters }
  }
}

impl ShardBoundary for FunctionBoundary {
  fn post(&self, envelope: Envelope) {
    if !self.open.load(Ordering::Acquire) {
      let (recipient, message, sender) = envelope.into_parts();
      if let Some(dead_letters) = &self.dead_letters {
        dead_letters.publish(DeadLetter::new(message, sender, recipient));
      }
      return;
    }
    let (recipient, message, sender) = envelope.into_parts();
    let _ = recipient;
    (self.handler)(&sender, message);
  }

  fn poll(&self) -> Option<Envelope> {
    None
  }

  fn poll_within(&self, _max: Duration) -> Option<Envelope> {
    None
  }

  fn has_pending(&self) -> bool {
    false
  }

  fn close(&self) {
    self.open.store(false, Ordering::Release);
  }
}
