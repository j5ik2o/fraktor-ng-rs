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

//! The trait that produces DNS message identifiers.

use super::IdGenerationError;

/// Produces the identifier of a DNS query.
///
/// Mirrors `org.apache.pekko.io.dns.IdGenerator`, whose `nextId(): Short`
/// returns the identifier. The method takes `&self` so a single generator may be
/// shared across threads, as the original's rustdoc promises.
pub trait IdGenerator: Send + Sync {
  /// Returns the next identifier, or [`IdGenerationError`] when the source of
  /// randomness failed.
  fn next_id(&self) -> Result<i16, IdGenerationError>;
}
