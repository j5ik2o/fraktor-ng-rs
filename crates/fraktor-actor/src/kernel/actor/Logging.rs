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

//! The events an actor logs, mirroring `org.apache.pekko.event.Logging`.

pub type Mdc = alloc::collections::BTreeMap<alloc::string::String, alloc::string::String>;

mod debug;
mod error;
mod event_sink;
mod info;
mod initialize_logger;
mod log_class;
mod log_event;
mod log_level;
mod log_marker;
mod log_marker_adapter;
mod log_publisher;
mod logger_initialized;
mod warning;

pub use debug::Debug;
pub use error::Error;
pub use event_sink::EventSink;
pub use info::Info;
pub use initialize_logger::InitializeLogger;
pub use log_class::LogClass;
pub use log_event::LogEvent;
pub use log_level::LogLevel;
pub use log_marker::LogMarker;
pub use log_marker_adapter::LogMarkerAdapter;
pub use log_publisher::LogPublisher;
pub use logger_initialized::LoggerInitialized;
pub use warning::Warning;

/// Returns the empty diagnostic context that an event without one carries.
///
/// Mirrors `org.apache.pekko.event.Logging.emptyMDC`.
pub fn empty_mdc() -> Mdc {
  Mdc::new()
}
