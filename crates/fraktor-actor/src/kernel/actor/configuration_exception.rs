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

//! The error raised when a configuration fails verification.

use alloc::string::String;
use core::fmt;

/// The error for a configuration that fails verification when an actor
/// is created.
///
/// Mirrors `org.apache.pekko.ConfigurationException`, which
/// `LocalActorRefProvider.actorOf` and the `RoutedActorRef` constructor
/// throw when a dispatcher is not configured or a router head would run
/// on a balancing dispatcher.
#[derive(Clone, PartialEq, Eq)]
pub struct ConfigurationException {
  cause: String,
}

impl ConfigurationException {
  /// Creates the error with its message.
  pub fn new(message: String) -> Self {
    Self { cause: message }
  }
}

impl fmt::Display for ConfigurationException {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.cause)
  }
}

impl fmt::Debug for ConfigurationException {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.debug_tuple("ConfigurationException").field(&self.cause).finish()
  }
}

impl core::error::Error for ConfigurationException {}
