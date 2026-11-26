// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// contributor license agreements. See the NOTICE file distributed with
// this work for additional information regarding copyright ownership.
// The ASF licenses this file to You under the Apache License, Version 2.0
// (the "License"); you may not use this file except in compliance with
// the License. You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The ID generator policy a DNS resolver uses.

/// Which generator produces DNS message identifiers.
///
/// Mirrors `org.apache.pekko.io.dns.IdGenerator.Policy`. `EnhancedDoubleHashRandom`
/// exists only to name the original's third configured policy; this port does not
/// implement the enhanced double hash generator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdGeneratorPolicy {
  /// A non-cryptographic pseudo-random generator kept per thread.
  ///
  /// Mirrors the original `IdGenerator.Policy.ThreadLocalRandom`.
  ThreadLocalRandom,
  /// A generator drawing each identifier from the operating system's entropy.
  ///
  /// Mirrors the original `IdGenerator.Policy.SecureRandom`.
  SecureRandom,
  /// The original's enhanced double hash policy, identified but not generated.
  ///
  /// Mirrors the original `IdGenerator.Policy.EnhancedDoubleHashRandom`. No
  /// generator is ported for it, so [`crate::adaptor::RandomIdGenerator::new`]
  /// rejects it.
  EnhancedDoubleHashRandom,
}

impl IdGeneratorPolicy {
  /// Reads the `id-generator-policy` setting, as the original's
  /// `IdGeneratorPolicy` does, or returns `None` for an unknown text.
  ///
  /// The match is case-sensitive. An empty text names
  /// `EnhancedDoubleHashRandom`, as in the original.
  pub(crate) fn from_config_value(text: &str) -> Option<Self> {
    match text {
      | "thread-local-random" => Some(Self::ThreadLocalRandom),
      | "secure-random" => Some(Self::SecureRandom),
      | "" | "enhanced-double-hash-random" => Some(Self::EnhancedDoubleHashRandom),
      | _ => None,
    }
  }
}
