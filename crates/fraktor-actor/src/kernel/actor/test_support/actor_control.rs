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

//! The probe a test sends to read the actor it wraps.

/// The numbered probe a read of the underlying actor sends.
///
/// Mirrors `InternalGetActor`, which `TestActorRef.underlyingActor` asks
/// the actor when the instance is not already visible: the probe travels
/// in mailbox order like any other message, and the executor answers it
/// before the behaviour can see it, so a behaviour installed with
/// `change_behavior` or a message held in the stash cannot hide the
/// actor from the test that wraps it.
pub(crate) struct ActorControl {
  /// The number the handle assigned to this probe.
  pub(crate) sequence: u64,
}
