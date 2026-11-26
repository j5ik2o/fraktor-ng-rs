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

//! How many messages one run of a mailbox may handle.

/// The number of normal messages one run handles at most before it hands the
/// thread back.
///
/// Mirrors `throughput` of a Pekko dispatcher (`reference.conf:622`), which
/// Pekko reads as `max(throughput, 1)`: a throughput of 0 is a throughput of 1,
/// so a run always handles at least one message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Throughput(u32);

impl Throughput {
  /// A throughput of `messages`, where 0 counts as 1.
  pub const fn new(messages: u32) -> Self {
    Self(if messages == 0 { 1 } else { messages })
  }

  /// Returns the number of messages, at least 1.
  pub const fn messages(self) -> u32 {
    self.0
  }
}
