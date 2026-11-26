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

//! P1 of the design of #234 (section 7.3): every sequence of operations on a
//! [`MailboxStatus`](super::MailboxStatus) gives the same results and the same
//! state as the status part of `specs/mailbox_scheduling.qnt`.

use std::collections::BTreeSet;
use std::vec::Vec;

use portable_atomic::Ordering;

use super::MailboxStatus;

/// The status part of the Quint state (`closed`, `scheduled`, `suspendCount`,
/// `sysFlag`), changed as the actions of the spec change it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Model {
  closed: bool,
  scheduled: bool,
  suspend_count: u32,
  sys_flag: bool,
}

#[derive(Clone, Copy, Debug)]
enum Op {
  // fetchOr / regFetchOr / finishFetchOr: the second step of a reservation.
  SetAsScheduled,
  // finishIdle.
  SetAsIdle,
  // processSystem of Suspend, and failInvoke.
  Suspend,
  // processSystem of Resume.
  Resume,
  // processSystem of Terminate.
  BecomeClosed,
  // control and decide: the enqueue raises the flag.
  RaiseFlag,
  // clearSysFlag.
  ClearFlag,
}

const OPS: [Op; 7] =
  [Op::SetAsScheduled, Op::SetAsIdle, Op::Suspend, Op::Resume, Op::BecomeClosed, Op::RaiseFlag, Op::ClearFlag];

impl Model {
  /// Applies `op` and returns what the operation returns.
  fn apply(&mut self, op: Op) -> Option<bool> {
    match op {
      | Op::SetAsScheduled => {
        // fetchedOr: only a caller that finds both bits clear submits.
        let won = !self.closed && !self.scheduled;
        self.scheduled = true;
        Some(won)
      },
      | Op::SetAsIdle => {
        self.scheduled = false;
        None
      },
      | Op::Suspend => {
        if self.closed {
          return Some(false);
        }
        self.suspend_count += 1;
        Some(self.suspend_count == 1)
      },
      | Op::Resume => {
        if self.closed {
          return Some(false);
        }
        self.suspend_count = self.suspend_count.saturating_sub(1);
        Some(self.suspend_count == 0)
      },
      | Op::BecomeClosed => {
        let was_open = !self.closed;
        self.closed = true;
        Some(was_open)
      },
      | Op::RaiseFlag => {
        self.sys_flag = true;
        None
      },
      | Op::ClearFlag => {
        self.sys_flag = false;
        None
      },
    }
  }

  // canBeScheduled of the spec, with what the queues hold given apart.
  fn can_be_scheduled(&self, has_messages: bool, has_system_messages: bool) -> bool {
    !self.closed && if self.suspend_count == 0 { has_messages || has_system_messages } else { has_system_messages }
  }
}

fn apply(status: &MailboxStatus, op: Op) -> Option<bool> {
  match op {
    | Op::SetAsScheduled => Some(status.set_as_scheduled()),
    | Op::SetAsIdle => {
      status.set_as_idle();
      None
    },
    | Op::Suspend => Some(status.suspend()),
    | Op::Resume => Some(status.resume()),
    | Op::BecomeClosed => Some(status.become_closed()),
    | Op::RaiseFlag => {
      status.raise_system_flag();
      None
    },
    | Op::ClearFlag => {
      status.clear_system_flag();
      None
    },
  }
}

/// Reads the word apart. Once closed, the other bits mean nothing, so only
/// `closed` is compared then.
fn observed(status: &MailboxStatus, model: &Model) -> Model {
  let bits = status.0.load(Ordering::SeqCst);
  let closed = bits & 1 != 0;
  if closed && model.closed {
    return *model;
  }
  Model { closed, scheduled: bits & 2 != 0, suspend_count: bits >> 3, sys_flag: bits & 4 != 0 }
}

fn check_queries(status: &MailboxStatus, model: &Model, context: &dyn Fn() -> String) {
  assert_eq!(status.is_closed(), model.closed, "is_closed after {}", context());
  if model.closed {
    // A closed mailbox asks no queue.
    let asked = core::cell::Cell::new(false);
    assert!(
      !status.can_be_scheduled(|| asked.replace(true), || asked.replace(true)),
      "a closed mailbox is never scheduled, after {}",
      context()
    );
    assert!(!asked.get(), "a closed mailbox read a queue, after {}", context());
    assert!(!status.should_process_message(), "should_process_message after {}", context());
    return;
  }
  assert_eq!(status.is_suspended(), model.suspend_count > 0, "is_suspended after {}", context());
  assert_eq!(status.has_system_messages(), model.sys_flag, "has_system_messages after {}", context());
  // canInvoke of the spec, without the queue and the batch.
  assert_eq!(status.should_process_message(), model.suspend_count == 0, "should_process_message after {}", context());
  for (m, s) in [(false, false), (false, true), (true, false), (true, true)] {
    assert_eq!(
      status.can_be_scheduled(|| m, || s),
      model.can_be_scheduled(m, s),
      "can_be_scheduled({m}, {s}) after {}",
      context()
    );
  }
  if model.suspend_count > 0 {
    // A suspended mailbox asks only the system queue.
    assert!(!status.can_be_scheduled(|| panic!("asked the normal queue"), || false), "after {}", context());
  }
}

/// Runs `ops` on a new status and on the model, comparing after every step.
/// Returns the suspend counts the open mailbox went through.
fn check_sequence(ops: &[Op]) -> BTreeSet<u32> {
  let status = MailboxStatus::new();
  let mut model = Model::default();
  let mut counts = BTreeSet::from([0]);
  for (index, op) in ops.iter().enumerate() {
    let context = || format!("{:?}", &ops[..=index]);
    assert_eq!(apply(&status, *op), model.apply(*op), "result of {:?}, after {}", op, context());
    assert_eq!(observed(&status, &model), model, "state after {}", context());
    check_queries(&status, &model, &context);
    if !model.closed {
      counts.insert(model.suspend_count);
    }
  }
  counts
}

// Every sequence of the seven operations up to length 6, so every suspend
// count from 0 to 6 is reached with every other bit set and clear.
#[test]
fn every_short_sequence_matches_the_spec() {
  let mut counts = BTreeSet::new();
  let mut sequences: Vec<Vec<Op>> = std::vec![Vec::new()];
  for _ in 0..6 {
    let mut longer = Vec::with_capacity(sequences.len() * OPS.len());
    for sequence in &sequences {
      counts.extend(check_sequence(sequence));
      for op in OPS {
        let mut next = sequence.clone();
        next.push(op);
        longer.push(next);
      }
    }
    sequences = longer;
  }
  for sequence in &sequences {
    counts.extend(check_sequence(sequence));
  }
  assert!((0..=5).all(|count| counts.contains(&count)), "suspend counts reached: {counts:?}");
}

// SplitMix64: a fixed seed, so a failure names a sequence that replays.
struct Rng(u64);

impl Rng {
  fn next(&mut self) -> u64 {
    self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = self.0;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
  }

  fn below(&mut self, bound: u64) -> u64 {
    self.next() % bound
  }
}

// Random sequences of length 0 to 64. Closing is drawn rarely, so most of a
// sequence runs on an open mailbox, and Suspend and Resume are drawn often
// enough to reach a count of 5.
#[test]
fn random_long_sequences_match_the_spec() {
  let seed = 0x2340_0250_u64;
  let mut rng = Rng(seed);
  let mut counts = BTreeSet::new();
  for _ in 0..5_000 {
    let length = rng.below(65) as usize;
    let ops: Vec<Op> = (0..length)
      .map(|_| match rng.below(40) {
        | 0 => Op::BecomeClosed,
        | 1..=8 => Op::Suspend,
        | 9..=14 => Op::Resume,
        | 15..=21 => Op::SetAsScheduled,
        | 22..=28 => Op::SetAsIdle,
        | 29..=34 => Op::RaiseFlag,
        | _ => Op::ClearFlag,
      })
      .collect();
    counts.extend(check_sequence(&ops));
  }
  assert!((0..=5).all(|count| counts.contains(&count)), "seed {seed:#x}: suspend counts reached: {counts:?}");
}

impl MailboxStatus {
  /// Returns whether the suspend count is above zero.
  pub(crate) fn is_suspended(&self) -> bool {
    self.load() >= super::SUSPEND_UNIT
  }
}
