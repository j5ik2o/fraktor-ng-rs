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

//! One entry of the extension registry: the state of one id in one system.

use core::any::Any;

use crate::kernel::extension::ExtensionError;
use crate::kernel::sync::Arc;

/// The state of one extension id in one system.
///
/// The four states mirror the life of Pekko's `ConcurrentHashMap` entry
/// and its kept failure: not yet created (no entry), being created,
/// created, and failed with the failure kept. A request that finds
/// `Creating` fails with an in-progress error; it does not wait, because
/// creation runs under `&mut` access to the system (see [`ExtensionError`]).
pub(crate) enum ExtensionEntry {
  /// Creation has started and has not finished. Also the state a panicked
  /// creation leaves behind.
  Creating,
  /// Creation succeeded; the value is shared by every later request.
  Created(Arc<dyn Any + Send + Sync>),
  /// Creation failed; the failure is kept and returned to later requests.
  Failed(ExtensionError),
}
