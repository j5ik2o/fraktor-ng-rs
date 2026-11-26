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

//! The port that an executor publishes log events to.

use crate::kernel::actor::Logging::LogEvent;
use crate::kernel::sync::Arc;

/// Receives the log events an executor publishes.
///
/// Mirrors the role that `org.apache.pekko.event.EventStream` plays for the
/// failures of a faulty actor: Pekko logs an `Error` event when it stops or
/// restarts a child, and an `EventFilter` sees that event through the stream.
/// The kernel cannot reach the test kit, so it asks a port to accept the event
/// and keeps no listener of its own. A system created without a sink drops the
/// events.
pub trait EventSink: Send + Sync {
  /// Publishes `event`.
  fn publish(&self, event: &dyn LogEvent);

  /// Returns the sink as a shared `Any`, so its owner can recover the concrete
  /// type with `Arc::downcast`.
  fn into_any(self: Arc<Self>) -> Arc<dyn core::any::Any + Send + Sync>;
}
