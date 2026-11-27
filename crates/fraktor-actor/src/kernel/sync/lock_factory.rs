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

//! The port that makes the locks the kernel takes.

use alloc::boxed::Box;

use crate::kernel::sync::RawLock;

/// Makes a [`RawLock`] for each value the kernel guards.
///
/// The adaptor passes one when it builds a system or a dispatcher. The kernel's own default is
/// [`SpinLockFactory`](super::SpinLockFactory).
pub trait LockFactory: Send + Sync {
  /// Returns a new lock, not shared with any lock made before.
  fn new_lock(&self) -> Box<dyn RawLock>;
}
