// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2018-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! Unit tests for [`FunctionRef`](super::FunctionRef).
//!
//! Mirrors the value semantics of Pekko's `org.apache.pekko.actor.FunctionRef`.

use super::FunctionRef;

#[test]
fn function_ref_is_send_and_sync_so_it_can_travel_as_a_message() {
  fn assert_send_sync<T: Send + Sync>() {}
  assert_send_sync::<FunctionRef>();
}
