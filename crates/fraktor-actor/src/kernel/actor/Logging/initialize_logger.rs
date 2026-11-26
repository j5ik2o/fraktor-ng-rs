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

//! The request that hands a logger the bus it subscribes to.

use crate::kernel::event::EventStreamBus;

/// The request that a logging bus sends to a logger it wants to start.
///
/// Mirrors `org.apache.pekko.event.Logging.InitializeLogger`. A logger answers
/// it with [`LoggerInitialized`](super::LoggerInitialized) after it has
/// subscribed to the channels it wants on the bus it is given. Pekko asks the
/// logger with an `ask` and waits for the answer; the port sends the request
/// and drives the system until the logger has handled it.
pub struct InitializeLogger {
  bus: EventStreamBus,
}

impl InitializeLogger {
  /// Creates the request that carries `bus`.
  pub fn new(bus: EventStreamBus) -> Self {
    Self { bus }
  }

  /// Returns the bus the logger subscribes to.
  pub fn bus(&self) -> &EventStreamBus {
    &self.bus
  }
}
