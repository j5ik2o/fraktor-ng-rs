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

//! The spellings an actor name can be passed in.

use alloc::string::String;

/// A name argument for [`ActorSystem::actor_of_named`](super::ActorSystem::actor_of_named).
///
/// Pekko's `actorOf(props, name)` takes a `String`, and `null` refuses
/// the creation. A Rust caller can hold the name as a `&str`, as a
/// `&String`, or as an `Option<&str>`, where `None` stands for Pekko's
/// `null`. The earlier `impl Into<Option<&str>>` form refused
/// `&String`, because trait matching does not apply the `&String` to
/// `&str` deref coercion, so this conversion is its own trait. There is
/// no `Option<&String>` spelling: a bare `None` must keep resolving to
/// `Option<&str>`, and a second `Option` implementation would make
/// that call ambiguous.
pub trait ActorNameArg<'name> {
  /// Returns the name, or `None` for Pekko's `null` name.
  fn into_option(self) -> Option<&'name str>;
}

impl<'name> ActorNameArg<'name> for &'name str {
  fn into_option(self) -> Option<&'name str> {
    Some(self)
  }
}

impl<'name> ActorNameArg<'name> for &'name String {
  fn into_option(self) -> Option<&'name str> {
    Some(self.as_str())
  }
}

impl<'name> ActorNameArg<'name> for Option<&'name str> {
  fn into_option(self) -> Option<&'name str> {
    self
  }
}
