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

//! A type-erased message sent to an actor.

use alloc::boxed::Box;
use core::any::Any;
use core::any::TypeId;
use core::fmt;

use crate::kernel::sync::Arc;

#[cfg(test)]
#[path = "message_value_test.rs"]
mod tests;

/// The payload of a message, with the name of its type attached.
///
/// `dyn Any` cannot answer `core::any::type_name` for the value it hides,
/// but a log line — the dead letter listener's, above all — names the type
/// of the message it reports. The payload therefore travels behind this
/// private trait, whose one implementation records the name when the value
/// is wrapped, so no field is added to each message.
///
/// The blanket implementation also covers `Arc<dyn Payload>` itself, so
/// the accessors on [`Message`] call through the trait object; calling a
/// method on the `Arc` directly would resolve to the blanket
/// implementation for the `Arc` type and answer for the pointer instead
/// of the payload.
trait Payload: Any + Send + Sync {
  fn type_name(&self) -> &'static str;
  fn as_any(&self) -> &dyn Any;
}

impl<T: Any + Send + Sync> Payload for T {
  fn type_name(&self) -> &'static str {
    core::any::type_name::<T>()
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

/// A message that carries a value of any type that can cross a thread.
///
/// Mirrors the object an `org.apache.pekko.actor.Actor` receives in its
/// `receive` partial function. Pekko messages are untyped `AnyRef`; the port
/// keeps the same type erasure so that a mailbox can hold messages of
/// different types, while requiring `Send + Sync` because a message may cross
/// the boundary of its shard and because a test probe hands the same message
/// both to an auto pilot and to its queue.
///
/// A message is cheap to clone: cloning shares the payload instead of copying
/// it, which is how one delivery reaches more than one consumer.

#[derive(Clone)]
pub struct Message {
  payload: Arc<dyn Payload>,
}

impl Message {
  /// Wraps a value as a message.
  ///
  /// A `Message` given to this function is returned unchanged, so forwarding a
  /// message keeps the payload a recipient can compare against.
  pub fn new<M: Any + Send + Sync>(payload: M) -> Self {
    let boxed: Box<dyn Payload> = Box::new(payload);
    match (*boxed).as_any().downcast_ref::<Message>() {
      | Some(inner) => inner.clone(),
      | None => Self { payload: Arc::from(boxed) },
    }
  }

  /// Returns the payload as the given type, or `None` if it has another type.
  pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
    (*self.payload).as_any().downcast_ref::<T>()
  }

  /// Returns the type of the payload it carries.
  ///
  /// A publisher that receives a `Message` needs this to route the message by
  /// the class of what it carries rather than the class of `Message` itself,
  /// which is how `EventStream` behaves in Pekko: an event is published to the
  /// subscribers of `event.getClass`.
  pub(crate) fn payload_type_id(&self) -> TypeId {
    (*self.payload).as_any().type_id()
  }

  /// Returns the name of the type of the payload it carries.
  ///
  /// Mirrors the `getClass.getName` a Pekko log line prints for a message:
  /// the dead letter listener names the message it reports this way.
  pub fn payload_type_name(&self) -> &'static str {
    (*self.payload).type_name()
  }

  /// Returns whether the payload has the given type.
  pub fn is<T: Any>(&self) -> bool {
    (*self.payload).as_any().is::<T>()
  }
}

impl fmt::Debug for Message {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self.downcast_ref::<&'static str>() {
      | Some(text) => write!(formatter, "{text:?}"),
      | None => match self.downcast_ref::<i32>() {
        | Some(number) => write!(formatter, "{number}"),
        | None => formatter.write_str("<message of an unknown type>"),
      },
    }
  }
}

impl PartialEq<str> for Message {
  fn eq(&self, other: &str) -> bool {
    match self.downcast_ref::<&'static str>() {
      | Some(text) => *text == other,
      | None => false,
    }
  }
}

impl PartialEq<&str> for Message {
  fn eq(&self, other: &&str) -> bool {
    self == *other
  }
}

impl PartialEq<str> for &Message {
  fn eq(&self, other: &str) -> bool {
    *self == other
  }
}

impl PartialEq<Message> for &str {
  fn eq(&self, other: &Message) -> bool {
    other == *self
  }
}
