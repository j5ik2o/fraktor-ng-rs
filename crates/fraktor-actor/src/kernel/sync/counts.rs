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

//! What a test counts on its own thread: the clones of handles and of
//! mailbox runs, the walks of a mailbox's way back, and the locks taken.
//!
//! The kernel's `Arc` cannot count its own clones, so the few places that
//! clone a handle or a mailbox count here. Each thread keeps its own counts,
//! so tests that run at the same time do not mix them.

use std::cell::Cell;

std::thread_local! {
  static COUNTS: Cell<Counts> = const { Cell::new(Counts::ZERO) };
}

/// The counts of one thread since it started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
  /// Clones of a handle's path (`ActorRef::clone`).
  pub(crate) paths: u64,
  /// Clones of an endpoint that is a mailbox.
  pub(crate) mailbox_endpoints: u64,
  /// Clones of an endpoint that is a shard boundary.
  pub(crate) boundary_endpoints: u64,
  /// Mailbox runs made by reserving a mailbox.
  pub(crate) runs: u64,
  /// Walks of a mailbox's `Weak` back to its dispatcher.
  pub(crate) upgrades: u64,
  /// Sections run under a kernel lock.
  pub(crate) locks: u64,
}

impl Counts {
  const ZERO: Self = Self { paths: 0, mailbox_endpoints: 0, boundary_endpoints: 0, runs: 0, upgrades: 0, locks: 0 };

  /// Returns the counts since `earlier`.
  pub(crate) fn since(self, earlier: Self) -> Self {
    Self {
      paths: self.paths - earlier.paths,
      mailbox_endpoints: self.mailbox_endpoints - earlier.mailbox_endpoints,
      boundary_endpoints: self.boundary_endpoints - earlier.boundary_endpoints,
      runs: self.runs - earlier.runs,
      upgrades: self.upgrades - earlier.upgrades,
      locks: self.locks - earlier.locks,
    }
  }

  /// Returns how many `Arc`s were cloned.
  pub(crate) fn arc_clones(self) -> u64 {
    self.paths + self.mailbox_endpoints + self.boundary_endpoints + self.runs
  }

  /// Returns how many atomic increments of a strong count were made: the
  /// clones and the walks back.
  pub(crate) fn atomic_ops(self) -> u64 {
    self.arc_clones() + self.upgrades
  }
}

/// Returns the counts of this thread so far.
pub(crate) fn snapshot() -> Counts {
  COUNTS.with(Cell::get)
}

fn bump(change: impl FnOnce(&mut Counts)) {
  COUNTS.with(|counts| {
    let mut value = counts.get();
    change(&mut value);
    counts.set(value);
  });
}

pub(crate) fn count_path() {
  bump(|counts| counts.paths += 1);
}

pub(crate) fn count_mailbox_endpoint() {
  bump(|counts| counts.mailbox_endpoints += 1);
}

pub(crate) fn count_boundary_endpoint() {
  bump(|counts| counts.boundary_endpoints += 1);
}

pub(crate) fn count_run() {
  bump(|counts| counts.runs += 1);
}

pub(crate) fn count_upgrade() {
  bump(|counts| counts.upgrades += 1);
}

pub(crate) fn count_lock() {
  bump(|counts| counts.locks += 1);
}
