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

//! The receive timeout an actor asks for.

use core::time::Duration;

/// The receive timeout an actor sets through
/// [`ActorContext::set_receive_timeout`](super::ActorContext::set_receive_timeout).
///
/// Mirrors the `scala.concurrent.duration.Duration` values that
/// `ActorContext.setReceiveTimeout` accepts, without a general
/// infinite-duration algebra: `Duration.Undefined` and `Duration.Inf` turn
/// the timeout off, and a finite duration turns it on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiveTimeoutSetting {
  /// `Duration.Undefined`: no receive timeout is in effect.
  Undefined,
  /// `Duration.Inf`: no receive timeout is in effect.
  Infinite,
  /// A finite inactivity duration after which `ReceiveTimeout` is sent.
  Finite(Duration),
}
