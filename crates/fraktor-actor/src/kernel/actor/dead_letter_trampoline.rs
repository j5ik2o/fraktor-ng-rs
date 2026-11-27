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

//! The port that runs dead letter publications without nesting them, and
//! the kernel's own implementation of it.

use alloc::boxed::Box;
use alloc::collections::VecDeque;

use spin::Mutex;

/// One dead letter publication, as a unit of work for a trampoline.
pub type DeadLetterJob = Box<dyn FnOnce() + Send + 'static>;

/// Runs dead letter publications one at a time on the calling path.
///
/// A publication that arrives while another publication is already running
/// on the same trampoline is queued behind it and returns at once; the
/// outermost run works the queue in first-in, first-out order, so a dead
/// letter subscriber that resends to a stopped actor does not nest its
/// delivery inside the delivery already under way.
pub trait DeadLetterTrampoline: Send + Sync {
  /// Runs `job` now, or queues it behind the run already under way and
  /// returns.
  fn run(&self, job: DeadLetterJob);
}

/// The kernel's trampoline: one flag and one queue behind a spin mutex,
/// shared by every publication of one actor system.
///
/// The lock is held only while a job is taken from or added to the queue,
/// never while a job runs, so a publication raised inside a job queues
/// instead of waiting on the lock. This is the trampoline for an
/// environment driven by one execution subject: with several threads a
/// job queued by one thread may run on the thread that is already
/// running the queue.
pub(crate) struct SharedTrampoline {
  state: Mutex<SharedState>,
}

struct SharedState {
  running: bool,
  queue: VecDeque<DeadLetterJob>,
}

impl SharedTrampoline {
  pub(crate) fn new() -> Self {
    Self { state: Mutex::new(SharedState { running: false, queue: VecDeque::new() }) }
  }
}

impl DeadLetterTrampoline for SharedTrampoline {
  fn run(&self, job: DeadLetterJob) {
    {
      let mut state = self.state.lock();
      if state.running {
        state.queue.push_back(job);
        return;
      }
      state.running = true;
    }
    let mut guard = SharedGuard { state: &self.state, armed: true };
    job();
    loop {
      // The empty check and clearing the flag happen inside one lock
      // section: a job another thread queues in between would otherwise
      // be left in a queue nobody runs, with the flag already down. What
      // is still queued on the normal path stays queued for the next
      // outermost run; only an unwind discards it (the guard below).
      let next = {
        let mut state = self.state.lock();
        match state.queue.pop_front() {
          | Some(next) => Some(next),
          | None => {
            state.running = false;
            None
          },
        }
      };
      match next {
        | Some(next) => next(),
        | None => break,
      }
    }
    guard.armed = false;
    drop(guard);
  }
}

/// On an unwind, clears the flag and discards whatever is still queued,
/// so a panicking subscriber does not leave every later publication of
/// the system queued for ever. The queued jobs are taken out under the
/// lock and dropped after it is released: dropping one while holding it
/// could re-enter this trampoline and wait on the lock for ever.
struct SharedGuard<'a> {
  state: &'a Mutex<SharedState>,
  armed: bool,
}

impl Drop for SharedGuard<'_> {
  fn drop(&mut self) {
    if !self.armed {
      return;
    }
    let queued = {
      let mut state = self.state.lock();
      state.running = false;
      core::mem::take(&mut state.queue)
    };
    drop(queued);
  }
}
