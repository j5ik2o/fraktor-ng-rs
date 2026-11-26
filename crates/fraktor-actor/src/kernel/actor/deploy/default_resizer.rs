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
//! The default resizer, as a value.

use super::deploy_error::DeployError;

/// The resizer that grows and shrinks a pool, as a value.
///
/// Mirrors `org.apache.pekko.routing.DefaultResizer`. The dynamic
/// behaviour (when to resize, by how much) is not built in this port;
/// the value and the constructor's validation are.
#[derive(Clone, PartialEq, Debug)]
pub struct DefaultResizer {
  /// The lower bound of the pool size.
  pub lower_bound: u32,
  /// The upper bound of the pool size.
  pub upper_bound: u32,
  /// The pressure threshold.
  pub pressure_threshold: u32,
  /// The ramp-up rate.
  pub rampup_rate: f64,
  /// The backoff threshold.
  pub backoff_threshold: f64,
  /// The backoff rate.
  pub backoff_rate: f64,
  /// The number of messages between resizes.
  pub messages_per_resize: u32,
}

impl DefaultResizer {
  /// Builds a resizer, applying the original constructor's validation.
  ///
  /// Mirrors the `IllegalArgumentException`s of the original's
  /// constructor: the upper bound must be at least the lower bound,
  /// the rates must not be negative, the backoff threshold must be at
  /// most 1.0, and the messages per resize must be positive. The
  /// non-negativity of the bounds is not checked: the fields are
  /// unsigned, so a negative bound cannot be written (a narrowing by
  /// the input types, recorded in the design of #195). `NaN` rates
  /// pass, as in the original.
  ///
  /// # Errors
  ///
  /// Returns [`DeployError::InvalidResizer`] with the original's
  /// reason when a value is refused.
  pub fn new(
    lower_bound: u32,
    upper_bound: u32,
    pressure_threshold: u32,
    rampup_rate: f64,
    backoff_threshold: f64,
    backoff_rate: f64,
    messages_per_resize: u32,
  ) -> Result<Self, DeployError> {
    if upper_bound < lower_bound {
      return Err(DeployError::InvalidResizer {
        reason: alloc::format!("upperBound must be >= lowerBound, was: [{upper_bound}] < [{lower_bound}]"),
      });
    }
    if rampup_rate < 0.0 {
      return Err(DeployError::InvalidResizer {
        reason: alloc::format!("rampupRate must be >= 0.0, was [{rampup_rate}]"),
      });
    }
    if backoff_threshold > 1.0 {
      return Err(DeployError::InvalidResizer {
        reason: alloc::format!("backoffThreshold must be <= 1.0, was [{backoff_threshold}]"),
      });
    }
    if backoff_rate < 0.0 {
      return Err(DeployError::InvalidResizer {
        reason: alloc::format!("backoffRate must be >= 0.0, was [{backoff_rate}]"),
      });
    }
    if messages_per_resize == 0 {
      return Err(DeployError::InvalidResizer {
        reason: alloc::format!("messagesPerResize must be > 0, was [{messages_per_resize}]"),
      });
    }
    Ok(Self {
      lower_bound,
      upper_bound,
      pressure_threshold,
      rampup_rate,
      backoff_threshold,
      backoff_rate,
      messages_per_resize,
    })
  }

  /// The resizer with every value at its default.
  ///
  /// Mirrors `DefaultResizer()` with the defaults of reference.conf:
  /// lower 1, upper 10, pressure 1, rampup 0.2, backoff threshold 0.3,
  /// backoff rate 0.1, messages per resize 10.
  #[must_use]
  pub fn defaults() -> Self {
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
