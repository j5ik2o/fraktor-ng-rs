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

//! The name of the dispatcher a test actor ref starts its actor on.

/// The id of the calling-thread dispatcher, which a `TestActorRef`
/// starts its actor on when the recipe names no dispatcher.
///
/// Mirrors `CallingThreadDispatcher.Id`, and the name the testkit's
/// `reference.conf` defines `pekko.test.calling-thread-dispatcher`
/// under.
pub const CALLING_THREAD_DISPATCHER_ID: &str = "pekko.test.calling-thread-dispatcher";
