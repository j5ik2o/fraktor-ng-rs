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

//! The mailbox of one actor.

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::mem;

use crate::kernel::dispatch::mailbox_status::MailboxStatus;
use crate::kernel::dispatch::MailboxCell;
use crate::kernel::dispatch::MailboxRun;
use crate::kernel::dispatch::MessageQueue;
use crate::kernel::dispatch::RunLimits;
use crate::kernel::dispatch::SystemMessage;
use crate::kernel::message::Envelope;
use crate::kernel::sync::Arc;
use crate::kernel::sync::LockCell;
use crate::kernel::sync::LockFactory;

#[cfg(test)]
#[path = "mailbox_test.rs"]
mod tests;

/// The mailbox of one actor: its status, the gate, its queue of normal
/// messages, its queue of system messages, and the actor itself.
///
/// Mirrors `org.apache.pekko.dispatch.Mailbox` (design of #234, section 3.2).
/// The behaviour of the status and of scheduling is the fixed mailbox of
/// `specs/mailbox_scheduling.qnt`.
///
/// **The gate.** The mailbox, not its queue, knows whether it is closed, and
/// its gate also guards the queue of system messages. A mailbox of a
/// balancing team shares its queue with the team: a sender puts the message
/// on the shared queue without looking at the gate, and closing leaves the
/// shared queue to the rest of the team (Pekko's
/// `BalancingDispatcher.scala:75-89` and `:106-109`).
///
/// **Scheduling.** [`try_schedule`](Self::try_schedule) reads whether a run
/// would have work, then sets the scheduled bit with one `fetch_or`; only the
/// caller that turned the bit on gets the [`MailboxRun`] to hand to the pool.
/// Every path reserves this way: a sender, a sender of a system message, and
/// the end of a run.
// The gate gives invariant I6 of the design (design of #234, section 3.2).
// It is a choice inside what Pekko allows, not a promise to users, so it is
// written here and not in the rustdoc (docs/decisions/pekko-semantics-first.md):
// a sender looks at the closed bit and enqueues inside the gate's lock, and
// `close` closes and drains inside the same lock. A mailbox with a queue of
// its own takes the gate and then the queue's lock; nothing takes them the
// other way round. Nothing that runs inside the gate is user code: queues
// classify an envelope before the gate is taken (invariant I16).
pub struct Mailbox<C> {
  status: MailboxStatus,
  gate: LockCell<VecDeque<SystemMessage>>,
  queue: Arc<dyn MessageQueue>,
  shares_queue: bool,
  cell: LockCell<Option<C>>,
  limits: RunLimits,
}

impl<C: MailboxCell> Mailbox<C> {
  /// Creates the mailbox of `cell` with a queue of its own. Closing the
  /// mailbox takes what the queue still holds.
  pub fn new(cell: C, queue: Arc<dyn MessageQueue>, limits: RunLimits, locks: &dyn LockFactory) -> Self {
    Self::with_queue(cell, queue, false, limits, locks)
  }

  /// Creates the mailbox of `cell` for a balancing team that shares `queue`.
  /// Closing the mailbox leaves the queue to the rest of the team.
  pub fn sharing(cell: C, queue: Arc<dyn MessageQueue>, limits: RunLimits, locks: &dyn LockFactory) -> Self {
    Self::with_queue(cell, queue, true, limits, locks)
  }

  fn with_queue(
    cell: C,
    queue: Arc<dyn MessageQueue>,
    shares_queue: bool,
    limits: RunLimits,
    locks: &dyn LockFactory,
  ) -> Self {
    Self {
      status: MailboxStatus::new(),
      gate: LockCell::new(locks.new_lock(), VecDeque::new()),
      queue,
      shares_queue,
      cell: LockCell::new(locks.new_lock(), Some(cell)),
      limits,
    }
  }

  /// Puts a normal message on the queue.
  ///
  /// # Errors
  ///
  /// Hands the envelope back when the mailbox has a queue of its own and is
  /// closed, or when a bounded queue is full. The caller sends it to dead
  /// letters.
  pub fn enqueue(&self, envelope: Envelope) -> Result<(), Envelope> {
    if self.shares_queue {
      return self.queue.enqueue(envelope);
    }
    // The queue may run a function the user gave it; it runs here, before
    // the gate is taken, and a panic in it drops the envelope outside every
    // lock (invariant I16).
    let envelope = self.queue.classify(envelope);
    let status = &self.status;
    let queue = &self.queue;
    self
      .gate
      .with(|_| if status.is_closed() { Err(envelope.into_envelope()) } else { queue.enqueue_classified(envelope) })
  }

  /// Puts a system message on the system queue and raises the "has system
  /// messages" flag, both inside the gate, so that nobody who looks at the
  /// queue sees the message before the flag.
  ///
  /// # Errors
  ///
  /// Hands the message back when the mailbox is closed. The caller sends it
  /// to dead letters, which answer a `Watch` with a `DeathWatchNotification`
  /// (Pekko's `Mailbox.systemEnqueue`, invariant I12).
  pub fn system_enqueue(&self, message: SystemMessage) -> Result<(), SystemMessage> {
    let status = &self.status;
    self.gate.with(|system| {
      if status.is_closed() {
        Err(message)
      } else {
        system.push_back(message);
        status.raise_system_flag();
        Ok(())
      }
    })
  }

  /// Reserves the mailbox for a run, in two steps: reads whether a run would
  /// have work, then sets the scheduled bit with one `fetch_or`. Returns the
  /// run when this call turned the bit on in an open mailbox; the caller hands
  /// it to the pool. Pekko's `registerForExecution` up to `execute`.
  ///
  /// Between the two steps the work may be handled by the run that is still
  /// going, so the run returned may find nothing to do. Pekko has the same
  /// empty run, and the user never sees it (design of #234, section 3.10).
  pub fn try_schedule(mailbox: &Arc<Self>) -> Option<MailboxRun<C>> {
    if mailbox.reserve() {
      Some(MailboxRun::new(Arc::clone(mailbox)))
    } else {
      None
    }
  }

  pub(crate) fn reserve(&self) -> bool {
    self.can_be_scheduled() && self.status.set_as_scheduled()
  }

  // Reads what the queues hold, not the flag: the flag may stay up after its
  // message has been handled, and reading it would reserve runs with nothing
  // to do. The system queue is read only when the normal queue gives no work.
  // The status is read once, and a queue is read only when the status leaves
  // it a say: never for a closed mailbox, and only the system queue for a
  // suspended one.
  fn can_be_scheduled(&self) -> bool {
    self.status.can_be_scheduled(|| self.queue.has_messages(), || self.has_queued_system_messages())
  }

  fn has_queued_system_messages(&self) -> bool {
    self.gate.with(|system| !system.is_empty())
  }

  /// Adds one to the suspend count and returns whether the mailbox was not
  /// suspended before. Only the thread that runs the actor calls it; a suspend
  /// from outside is a [`SystemMessage::Suspend`] (invariant I4).
  pub fn suspend(&self) -> bool {
    self.status.suspend()
  }

  /// Takes one from the suspend count, stopping at zero, and returns whether
  /// the mailbox is no longer suspended. Only the thread that runs the actor
  /// calls it.
  pub fn resume(&self) -> bool {
    self.status.resume()
  }

  /// Closes the mailbox and returns what it held: the normal messages (none
  /// when the queue is shared) and the system messages, in order. The caller
  /// sends both to dead letters, after this returns and outside every lock.
  /// Closing a closed mailbox returns nothing.
  pub fn close(&self) -> (Vec<Envelope>, Vec<SystemMessage>) {
    // Draining inside the gate means that nobody sees the mailbox closed
    // with messages still in its own queue, and that a second `close` cannot
    // return before the first has taken everything. Nothing is dropped
    // inside: what was held is moved out and returned.
    let status = &self.status;
    let queue = &self.queue;
    let shares_queue = self.shares_queue;
    let held = self.gate.with(|system| {
      if status.become_closed() {
        let messages = if shares_queue { Vec::new() } else { queue.drain() };
        Some((messages, mem::take(system)))
      } else {
        None
      }
    });
    match held {
      | None => (Vec::new(), Vec::new()),
      | Some((messages, system)) => (messages, Vec::from(system)),
    }
  }

  /// Returns whether the mailbox is closed.
  pub fn is_closed(&self) -> bool {
    self.status.is_closed()
  }

  /// Returns whether the mailbox is suspended.
  pub fn is_suspended(&self) -> bool {
    self.status.is_suspended()
  }

  /// Returns whether the queue of normal messages holds a message. For a
  /// shared queue, the messages may be for any member of the team.
  pub fn has_messages(&self) -> bool {
    self.queue.has_messages()
  }

  /// Returns how many normal messages the queue holds.
  pub fn number_of_messages(&self) -> usize {
    self.queue.number_of_messages()
  }

  /// Returns whether a system message is waiting.
  pub fn has_system_messages(&self) -> bool {
    self.has_queued_system_messages()
  }

  /// Takes the actor out for a run. Panics when it is not there: another
  /// thread is running the same actor, which breaks invariant I1.
  pub(crate) fn take_cell(&self) -> C {
    self.cell.with(Option::take).expect("two runs of one mailbox at once (invariant I1)")
  }

  /// Puts the actor back after a run, before the scheduled bit is cleared.
  pub(crate) fn put_back_cell(&self, cell: C) {
    let previous = self.cell.with(|slot| slot.replace(cell));
    debug_assert!(previous.is_none(), "the cell was put back twice");
    drop(previous);
  }

  pub(crate) fn set_as_idle(&self) {
    self.status.set_as_idle();
  }

  /// Handles every waiting system message (Pekko's
  /// `processAllSystemMessages`). The flag comes down first and the queue is
  /// emptied after it, so a message that arrives meanwhile raises the flag
  /// again and is handled before the next normal message. When the flag is
  /// down, it costs one load and no lock.
  pub(crate) fn process_all_system_messages(&self, cell: &mut C) {
    while !self.status.is_closed() && self.status.has_system_messages() {
      self.status.clear_system_flag();
      while !self.status.is_closed() {
        let Some(message) = self.gate.with(VecDeque::pop_front) else { break };
        cell.system_invoke(self, message);
      }
    }
  }

  /// Hands normal messages to the actor, up to the throughput and the
  /// deadline, and handles the system messages that arrive after each one
  /// (Pekko's `processMailbox`). It looks at the flag, not at the system
  /// queue, after each message.
  pub(crate) fn process_mailbox(&self, cell: &mut C) {
    let mut left = self.limits.throughput().messages();
    let deadline = self.limits.deadline_from_now();
    while self.status.should_process_message() {
      let Some(envelope) = self.queue.dequeue() else { break };
      cell.invoke(self, envelope);
      self.process_all_system_messages(cell);
      let before_deadline = match deadline {
        | None => true,
        | Some((at, clock)) => clock.monotonic() < at,
      };
      if left > 1 && before_deadline {
        left -= 1;
      } else {
        break;
      }
    }
  }
}
