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

//! The channel that covers every dead letter event.

/// The channel that carries every dead letter event.
///
/// Mirrors `org.apache.pekko.actor.AllDeadLetters`, the trait that
/// `DeadLetter`, `SuppressedDeadLetter`, `Dropped`, and `UnhandledMessage`
/// all extend: a subscription to this channel receives each of them,
/// because the stream declares each of those classes a subclass of this
/// one when the system is created. The type itself is never constructed as
/// a value; it only names the channel, so it cannot clash with a message
/// type of the same name.
pub struct AllDeadLetters;
