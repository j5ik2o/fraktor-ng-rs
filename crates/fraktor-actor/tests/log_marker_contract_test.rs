// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2018-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// The marker logging API is added alongside the events that already exist. The
// two ported cases in `marker_logging_spec.rs` only observe marked events, so
// they cannot tell an unmarked `Info`/`Error` from one that wrongly defaults to
// a marker. This case checks the one point the existing regression cannot: an
// event built by the existing constructors answers no marker.

use fraktor_actor::kernel::actor::Logging::{Error, Info};

const SPEC_CLASS: &str = "org.apache.pekko.event.MarkerLoggingSpec";
const SPEC_LOG_CLASS: i32 = 3;

#[test]
fn existing_events_carry_no_marker() {
  assert!(Info::new(SPEC_CLASS.to_string(), SPEC_LOG_CLASS, "plain info").marker().is_none());
  assert!(Error::new(SPEC_CLASS.to_string(), SPEC_LOG_CLASS, "plain error").marker().is_none());
}
