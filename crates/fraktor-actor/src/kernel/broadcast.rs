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

//! A broadcaster's subscriber set and the messages that manage and use it.
//!
//! Mirrors `org.apache.pekko.routing.Listeners`: an actor owns the set of
//! subscribers, updates it through the management messages, applies an
//! operation to each current subscriber, and broadcasts a message to them.
//! The port keeps only that actor-local set; it has no system-wide table and
//! does not watch the subscribers.

mod subscribe;
mod subscriber_registry;
mod unsubscribe;
mod with_subscribers;

pub use subscribe::Subscribe;
pub use subscriber_registry::Subscribers;
pub use unsubscribe::Unsubscribe;
pub use with_subscribers::WithSubscribers;
