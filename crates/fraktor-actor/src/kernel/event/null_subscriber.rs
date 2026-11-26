// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2023 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The rejection of a null event stream subscriber.

use core::fmt;

/// The rejection of a null event stream subscriber.
///
/// Mirrors the `IllegalArgumentException("subscriber is null")` that
/// `org.apache.pekko.event.EventStream.subscribe` throws when it is given
/// `null`. The port makes the null case explicit in the type, so a caller that
/// passes `None` learns about the refusal instead of subscribing nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NullSubscriber;

impl fmt::Display for NullSubscriber {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("subscriber is null")
  }
}
