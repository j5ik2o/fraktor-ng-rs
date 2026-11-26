// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2016-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The failure of creating or looking up an extension, kept as a value.

use alloc::string::String;
use core::fmt;

/// The failure of creating or looking up an extension.
///
/// Pekko throws the exception an extension's creation raised and throws it
/// again, unchanged, on every later request for the same extension. The
/// port cannot carry an arbitrary exception as a value, so a failure is
/// kept as the name of its cause and a message: two requests that fail for
/// the same reason return equal values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionError {
  cause_name: &'static str,
  message: String,
}

impl ExtensionError {
  /// Creates a failure with the name of its cause and a message.
  ///
  /// An extension id reports its own creation failure through this
  /// constructor, naming the exception the original would have thrown
  /// (for example `"TestException"`).
  pub fn new(cause_name: &'static str, message: impl Into<String>) -> Self {
    Self { cause_name, message: message.into() }
  }

  /// The failure of a request that arrived while the extension is being
  /// created, including a request the creation itself makes for its own
  /// id.
  ///
  /// Pekko blocks such a request until the creation completes. The port
  /// creates extensions only from `&mut` access to the system, where
  /// blocking would deadlock, so the request fails instead. When the
  /// system becomes usable from several threads (stage 5 of issue #234),
  /// this request is to wait for the creation, as Pekko's documentation
  /// promises.
  pub(crate) fn in_progress() -> Self {
    Self { cause_name: "ExtensionInProgress", message: String::from("extension is being created") }
  }

  /// The failure of looking up an extension that was never registered.
  ///
  /// Mirrors the `IllegalArgumentException` Pekko's `extension` lookup
  /// throws for an unregistered id: `extension` only looks up, and
  /// creating is `register_extension`'s work.
  pub(crate) fn not_registered() -> Self {
    Self { cause_name: "ExtensionNotRegistered", message: String::from("extension is not registered") }
  }

  /// Returns the name of the cause this failure stands for.
  pub fn cause_name(&self) -> &'static str {
    self.cause_name
  }

  /// Returns the message the failure carries.
  pub fn message(&self) -> &str {
    &self.message
  }
}

impl fmt::Display for ExtensionError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}: {}", self.cause_name, self.message)
  }
}

impl core::error::Error for ExtensionError {}
