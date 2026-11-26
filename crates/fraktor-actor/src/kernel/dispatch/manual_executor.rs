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

//! The dispatcher that runs mailboxes when a caller drives it by hand.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::kernel::dispatch::Mailbox;
use crate::kernel::dispatch::MailboxCell;
use crate::kernel::dispatch::ManualTask;
use crate::kernel::dispatch::WakeSignal;
use crate::kernel::sync::Arc;
use crate::kernel::sync::LockCell;
use crate::kernel::sync::LockFactory;

/// The work of a dispatcher that runs only when its caller drives it
/// (`run_one`, `run_until_idle`), first in, first out.
///
/// Plays the part of Pekko's `CallingThreadDispatcher` for the hand-driven
/// system (design of #234, sections 3.5 and 11.11.1): a mailbox that wins its
/// scheduled bit is put here as a [`ManualTask::Run`], and the caller's thread
/// takes it out and runs it. A mailbox reaches this only through its `Weak`
/// back-edge (condition 2 of the ownership model's revision). Once its system
/// terminates, it takes no more work.
pub(crate) struct ManualExecutor<C> {
  tasks: LockCell<Option<VecDeque<ManualTask<C>>>>,
  signal: Option<Arc<dyn WakeSignal>>,
  key: usize,
}

impl<C: MailboxCell> ManualExecutor<C> {
  /// Creates an empty dispatcher for the system whose shard key is `key`.
  /// `signal`, when given, is raised whenever the list of work stops being
  /// empty.
  pub(crate) fn new(key: usize, signal: Option<Arc<dyn WakeSignal>>, locks: &dyn LockFactory) -> Self {
    Self { tasks: LockCell::new(locks.new_lock(), Some(VecDeque::new())), signal, key }
  }

  /// Returns the shard key of the system this dispatcher belongs to.
  pub(crate) fn key(&self) -> usize {
    self.key
  }

  /// Puts `task` at the end of the list. After the system has terminated,
  /// or when it is a run of a mailbox that has closed, the task is dropped,
  /// outside the lock.
  ///
  /// The closed check is inside the lock: a stop closes the mailbox and then
  /// takes its runs under the same lock, so a run reserved before the close
  /// and handed over after it cannot stay behind.
  pub(crate) fn submit(&self, task: ManualTask<C>) {
    let refused = self.tasks.with(|tasks| match tasks {
      | Some(_) if matches!(&task, ManualTask::Run(run) if run.mailbox_is_closed()) => Err(task),
      | Some(tasks) => {
        let was_empty = tasks.is_empty();
        tasks.push_back(task);
        Ok(was_empty)
      },
      | None => Err(task),
    });
    match refused {
      | Ok(true) => {
        if let Some(signal) = &self.signal {
          signal.notify();
        }
      },
      | Ok(false) => {},
      | Err(task) => drop(task),
    }
  }

  /// Takes the oldest task.
  pub(crate) fn pop(&self) -> Option<ManualTask<C>> {
    self.tasks.with(|tasks| tasks.as_mut().and_then(VecDeque::pop_front))
  }

  /// Returns whether no task waits.
  pub(crate) fn is_empty(&self) -> bool {
    self.tasks.with(|tasks| tasks.as_ref().is_none_or(VecDeque::is_empty))
  }

  /// Takes the runs of `mailbox` that wait, for a mailbox that has closed.
  /// The caller drops them outside the lock.
  pub(crate) fn take_runs_of(&self, mailbox: &Mailbox<C>) -> Vec<ManualTask<C>> {
    self.tasks.with(|tasks| {
      let mut taken = Vec::new();
      let Some(tasks) = tasks else {
        return taken;
      };
      let mut kept = VecDeque::with_capacity(tasks.len());
      for task in tasks.drain(..) {
        match &task {
          | ManualTask::Run(run) if run.is_for(mailbox) => taken.push(task),
          | _ => kept.push_back(task),
        }
      }
      *tasks = kept;
      taken
    })
  }

  /// Takes every task and refuses any later one, for a system that
  /// terminates. The caller drops them outside the lock.
  pub(crate) fn close(&self) -> VecDeque<ManualTask<C>> {
    self.tasks.with(Option::take).unwrap_or_default()
  }
}
