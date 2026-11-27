// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from
// Akka.
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! A dead letter trampoline that keeps its queue on the sender's thread.

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::VecDeque;

use crate::kernel::actor::DeadLetterJob;
use crate::kernel::actor::DeadLetterTrampoline;

thread_local! {
  static RUNNING: Cell<bool> = const { Cell::new(false) };
  static QUEUE: RefCell<VecDeque<DeadLetterJob>> = const { RefCell::new(VecDeque::new()) };
}

/// Runs dead letter publications flat on the thread that raised the
/// outermost one.
///
/// The flag and the queue are thread-local and shared by every system on
/// the thread that installed this trampoline, so a publication of one
/// system raised inside a subscriber of another is run by the outermost
/// run of the thread, in first-in, first-out order across both. A
/// subscriber that drives another system and waits for it therefore waits
/// for a delivery that cannot happen until its own handler returns.
pub struct SenderThreadTrampoline;

impl SenderThreadTrampoline {
  /// Creates the trampoline. The state it works through is thread-local,
  /// so every value of this type on one thread shares one queue.
  pub fn new() -> Self {
    Self
  }
}

impl Default for SenderThreadTrampoline {
  fn default() -> Self {
    Self::new()
  }
}

impl DeadLetterTrampoline for SenderThreadTrampoline {
  fn run(&self, job: DeadLetterJob) {
    // While the thread is destroying its thread-locals, `with` panics;
    // detect that case first and run the job where it was raised, which
    // nests for that one call, as the design accepts.
    let accessible = RUNNING.try_with(|_| ()).is_ok() && QUEUE.try_with(|_| ()).is_ok();
    if !accessible {
      job();
      return;
    }
    let nested = RUNNING.with(|running| {
      if running.get() {
        true
      } else {
        running.set(true);
        false
      }
    });
    if nested {
      QUEUE.with(|queue| queue.borrow_mut().push_back(job));
      return;
    }
    let guard = ThreadGuard;
    job();
    loop {
      let next = QUEUE.with(|queue| queue.borrow_mut().pop_front());
      match next {
        | Some(next) => next(),
        | None => break,
      }
    }
    drop(guard);
  }
}

/// Clears the flag and drops whatever is still queued when the outermost
/// run unwinds, so a panicking subscriber does not leave every later
/// publication on the thread queued for ever. The queued jobs are taken
/// out of the thread-local first and dropped after the borrow ends:
/// dropping one inside the borrow could re-enter this trampoline and
/// borrow the queue again.
struct ThreadGuard;

impl Drop for ThreadGuard {
  fn drop(&mut self) {
    if let Ok(()) = RUNNING.try_with(|running| running.set(false)) {}
    let queued = QUEUE.try_with(|queue| core::mem::take(&mut *queue.borrow_mut())).ok();
    drop(queued);
  }
}
