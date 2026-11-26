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

use super::actor_name_arg_seal::ActorNameArgSeal;

/// A name argument for [`ActorSystem::actor_of_named`](super::ActorSystem::actor_of_named).
///
/// Pekko's `actorOf(props, name)` takes a `String`, and `null` refuses
/// the creation. A Rust caller can hold the name as any reference to a
/// string (`&str`, `&String`, `&Box<str>`, `&Cow<str>`, `&&str`, and so
/// on), or as an `Option<&str>`, where `None` stands for Pekko's `null`.
///
/// The trait is sealed: only the spellings this crate provides
/// implement it.
pub trait ActorNameArg<'name>: ActorNameArgSeal {
  /// Returns the name, or `None` for Pekko's `null` name.
  fn into_option(self) -> Option<&'name str>;
}

impl<'name, S: AsRef<str> + ?Sized> ActorNameArg<'name> for &'name S {
  fn into_option(self) -> Option<&'name str> {
    Some(self.as_ref())
  }
}

impl<'name> ActorNameArg<'name> for Option<&'name str> {
  fn into_option(self) -> Option<&'name str> {
    self
  }
}

impl<S: AsRef<str> + ?Sized> ActorNameArgSeal for &S {}

impl ActorNameArgSeal for Option<&str> {}
