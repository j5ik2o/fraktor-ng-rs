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

//! Unit tests for the message of
//! [`UnsupportedFraktorVersion`](super::UnsupportedFraktorVersion).

use super::UnsupportedFraktorVersion;

#[test]
fn exposes_its_three_fields() {
  let error = UnsupportedFraktorVersion::new("library", "2.5.6", "2.5.5");

  assert_eq!(error.library_name(), "library");
  assert_eq!(error.required_version(), "2.5.6");
  assert_eq!(error.current_version(), "2.5.5");
}

// The message is the original `PekkoVersion` text with Pekko replaced by
// Fraktor, so each field appears in its own placeholder.
#[test]
fn names_every_field_in_the_message() {
  let error = UnsupportedFraktorVersion::new("library", "2.5.6", "2.5.5");

  assert_eq!(error.to_string(), "Current version of Fraktor is [2.5.5], but library requires version [2.5.6]");
}
