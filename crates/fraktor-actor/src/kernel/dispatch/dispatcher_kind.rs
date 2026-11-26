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

//! The kind of a dispatcher.

/// The kind of a dispatcher.
///
/// Configuration verification currently only checks whether a kind is
/// `Balancing`; dispatcher execution belongs to #234.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DispatcherKind {
  /// The default dispatcher.
  Default,
  /// A fork-join pool dispatcher.
  ForkJoin,
  /// A dispatcher that pins its actors to one thread.
  Pinned,
  /// A dispatcher that shares one mailbox between its actors.
  Balancing,
  /// A dispatcher that runs on the calling thread.
  CallingThread,
  /// A dispatcher the test kit drives by hand.
  Manual,
}
