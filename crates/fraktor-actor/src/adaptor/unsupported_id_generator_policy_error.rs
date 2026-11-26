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

//! The error an unsupported ID generator policy reports.

use core::fmt;

use crate::kernel::io::dns::IdGeneratorPolicy;

/// The reason [`RandomIdGenerator::new`](super::RandomIdGenerator::new) rejected
/// a policy.
///
/// The original's enhanced double hash generator is not ported, so
/// `IdGeneratorPolicy::EnhancedDoubleHashRandom` has no generator and is
/// reported here with its policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedIdGeneratorPolicyError {
  policy: IdGeneratorPolicy,
}

impl UnsupportedIdGeneratorPolicyError {
  pub(crate) fn new(policy: IdGeneratorPolicy) -> Self {
    Self { policy }
  }

  /// Returns the rejected policy.
  pub fn policy(&self) -> IdGeneratorPolicy {
    self.policy
  }
}

impl fmt::Display for UnsupportedIdGeneratorPolicyError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "the id generator policy {:?} is not supported", self.policy)
  }
}

impl std::error::Error for UnsupportedIdGeneratorPolicyError {}
