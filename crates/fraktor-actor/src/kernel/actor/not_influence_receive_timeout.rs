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

//! The marker for messages that do not reset the receive timeout.

/// Marks a message type that does not reset the receive timeout.
///
/// Mirrors `org.apache.pekko.actor.NotInfluenceReceiveTimeout`. Pekko tests
/// the marker with `isInstanceOf` at run time; Rust cannot test trait
/// membership at run time, so a type that implements this trait must also
/// be registered with
/// [`ActorSystem::register_not_influence_receive_timeout`](crate::kernel::actor_system::ActorSystem::register_not_influence_receive_timeout)
/// before the first message is delivered in that system.
pub trait NotInfluenceReceiveTimeout {}
