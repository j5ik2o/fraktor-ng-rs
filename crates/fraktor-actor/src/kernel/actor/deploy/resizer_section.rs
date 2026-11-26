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

//! The resizer part of a deployment section.

/// The `resizer` part of a section, without an `enabled` field: the
/// presence of the section is what enables the resizer (see
/// `DeploymentSection::resizer`).
#[derive(Clone, PartialEq, Debug)]
pub struct ResizerSection {
  /// The `lower-bound`; the default is one.
  pub lower_bound: u32,
  /// The `upper-bound`; the default is ten.
  pub upper_bound: u32,
  /// The `pressure-threshold`; the default is one. Signed, as in the
  /// original: its pressure test treats every value below one (zero
  /// and the negatives alike) by the same branch, so the value is
  /// held as written.
  pub pressure_threshold: i32,
  /// The `rampup-rate`; the default is 0.2.
  pub rampup_rate: f64,
  /// The `backoff-threshold`; the default is 0.3.
  pub backoff_threshold: f64,
  /// The `backoff-rate`; the default is 0.1.
  pub backoff_rate: f64,
  /// The `messages-per-resize`; the default is ten.
  pub messages_per_resize: u32,
}

impl Default for ResizerSection {
  /// The values of reference.conf's `pekko.actor.deployment.default.resizer`.
  fn default() -> Self {
    Self {
      lower_bound: 1,
      upper_bound: 10,
      pressure_threshold: 1,
      rampup_rate: 0.2,
      backoff_threshold: 0.3,
      backoff_rate: 0.1,
      messages_per_resize: 10,
    }
  }
}
