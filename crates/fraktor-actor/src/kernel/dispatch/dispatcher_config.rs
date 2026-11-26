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

//! One dispatcher definition.

use alloc::string::String;

use crate::kernel::dispatch::DispatcherKind;

/// One dispatcher definition, built in code (no config file).
///
/// Mirrors one entry of the `pekko.actor.dispatchers` configuration in
/// `org.apache.pekko.dispatch.Dispatchers`, with the configurator class
/// name replaced by the [`DispatcherKind`] enum.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DispatcherConfig {
  /// The id actors name through `Props::with_dispatcher`.
  pub id: String,
  /// The kind of dispatcher the id stands for.
  pub kind: DispatcherKind,
}
