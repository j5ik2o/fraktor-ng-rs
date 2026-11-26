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

//! The error a parent raises when a child name is already taken.

use alloc::format;
use alloc::string::String;
use core::any::Any;
use core::fmt;

/// The error a parent raises when it is asked for a child whose name a live
/// child of the same parent already uses.
///
/// Mirrors `org.apache.pekko.actor.InvalidActorNameException`. Pekko throws it
/// from `Children.attachChild` when a name is taken or the parent is
/// terminating; the port returns it from `ActorContext::actor_of` so a caller
/// can react to it without unwinding. The name may be reused once the child
/// that held it has stopped.
#[derive(Clone, PartialEq, Eq)]
pub struct InvalidActorNameException {
  cause: String,
}

impl InvalidActorNameException {
  /// Creates the error for `name` being taken by a live child of the parent.
  pub fn name_taken(name: &str) -> Self {
    Self { cause: format!("Actor name [{}] is not unique!", name) }
  }

  /// Creates the error for a name that fails validation, carrying the
  /// message of `ActorPath::validate_path_element` unchanged.
  ///
  /// Mirrors the `InvalidActorNameException` that `Children.checkName`
  /// and `ActorPath.validatePathElement` raise together in Pekko: the
  /// entry point checks only for null and empty, and every other
  /// refusal (a leading `$`, a bad `%` escape, a non-ASCII character, a
  /// forbidden character and its position) is the one message of
  /// `validatePathElement`.
  pub fn invalid_name(message: String) -> Self {
    Self { cause: message }
  }

  /// Creates the error for an empty name.
  ///
  /// Mirrors the empty half of `Children.checkName`'s
  /// "actor name must not be empty" refusal. The null half has no
  /// counterpart: a Rust `&str` cannot be null.
  pub fn empty_name() -> Self {
    Self { cause: String::from("actor name must not be empty") }
  }

  /// Creates the error for a parent that is terminating or terminated.
  ///
  /// Mirrors the `IllegalStateException` that `Children.attachChild` throws
  /// when a defunct guardian is asked for a child.
  pub fn terminating() -> Self {
    Self { cause: String::from("cannot create children while terminating or terminated") }
  }

  /// Returns whether this error is an instance of `T`.
  ///
  /// Mirrors Pekko's `e match { case _: T => ... }`: a caller that holds a
  /// concrete error narrows it the same way it would narrow a caught
  /// exception. The port keeps the shape on the error so a caller reads a
  /// single value instead of matching a wrapper.
  pub fn is<T: Any>(&self) -> bool {
    core::any::TypeId::of::<T>() == core::any::TypeId::of::<Self>()
  }
}

impl fmt::Display for InvalidActorNameException {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.cause)
  }
}

impl fmt::Debug for InvalidActorNameException {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.debug_tuple("InvalidActorNameException").field(&self.cause).finish()
  }
}

impl core::error::Error for InvalidActorNameException {}
