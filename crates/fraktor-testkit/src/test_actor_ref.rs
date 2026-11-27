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

//! A handle on a running actor that a test reads and drives directly.
//!
//! Mirrors `org.apache.pekko.testkit.TestActorRef`.

mod calling_thread_dispatcher_id;
mod test_actor_ref_value;

pub use calling_thread_dispatcher_id::CALLING_THREAD_DISPATCHER_ID;
pub use test_actor_ref_value::TestActorRef;
