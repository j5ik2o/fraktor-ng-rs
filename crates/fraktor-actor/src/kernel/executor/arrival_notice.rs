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

//! What tells a waiting caller that a function reference handled a message.

use portable_atomic::AtomicU64;
use portable_atomic::Ordering;

use crate::kernel::dispatch::WakeSignal;
use crate::kernel::sync::Arc;

/// Counts the messages that function references of a system have handled,
/// and raises the system's wake signal for each.
///
/// A test actor is a function reference: a message reaches it on the
/// sender's thread, and the test waits for it in `await_envelope`. The count
/// tells that wait a message has arrived apart from a signal raised for work
/// that is already done.
pub(crate) struct ArrivalNotice {
  signal: Arc<dyn WakeSignal>,
  count: AtomicU64,
}

impl ArrivalNotice {
  pub(crate) fn new(signal: Arc<dyn WakeSignal>) -> Self {
    Self { signal, count: AtomicU64::new(0) }
  }

  /// Records that a function reference handled a message, and wakes the
  /// waiting caller.
  pub(crate) fn notify(&self) {
    self.count.fetch_add(1, Ordering::AcqRel);
    self.signal.notify();
  }

  /// Returns how many messages function references have handled.
  pub(crate) fn count(&self) -> u64 {
    self.count.load(Ordering::Acquire)
  }

  /// Waits on the system's wake signal for at most `max`.
  pub(crate) fn wait(&self, max: core::time::Duration) -> bool {
    self.signal.wait(max)
  }
}
