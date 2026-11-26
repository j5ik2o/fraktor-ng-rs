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

//! P3 of the design of #234 (section 7.3) and the gate of a mailbox: what is
//! sent after closing is turned away, and every message sent is handled, still
//! queued, or a dead letter. A queue shared by a team stays when one member
//! closes.

use std::collections::VecDeque;
use std::vec::Vec;

use super::Mailbox;
use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_path::RootActorPath;
use crate::kernel::actor_ref::next_actor_ref;
use crate::kernel::address::Address;
use crate::kernel::dispatch::MailboxCell;
use crate::kernel::dispatch::MessageQueue;
use crate::kernel::dispatch::Next;
use crate::kernel::dispatch::RunLimits;
use crate::kernel::dispatch::RunOutcome;
use crate::kernel::dispatch::SystemMessage;
use crate::kernel::dispatch::Throughput;
use crate::kernel::dispatch::UnboundedQueue;
use crate::kernel::executor::NoOpBoundary;
use crate::kernel::message::Envelope;
use crate::kernel::message::Message;
use crate::kernel::sync::Arc;
use crate::kernel::sync::SpinLockFactory;

// A cell that keeps the numbers of the messages it handled.
#[derive(Default)]
struct Keeper {
  handled: Vec<u32>,
}

impl MailboxCell for Keeper {
  type Env = ();

  fn invoke(&mut self, _env: &mut (), _mailbox: &Mailbox<Self>, envelope: Envelope) -> Next {
    self.handled.push(number(&envelope));
    Next::Continue
  }

  fn system_invoke(&mut self, _env: &mut (), _mailbox: &Mailbox<Self>, _message: SystemMessage) {}
}

fn envelope(value: u32) -> Envelope {
  let root = RootActorPath::new(Address::new("fraktor", "mailbox-test"));
  let actor = next_actor_ref(ActorPath::from_root(root), Arc::new(NoOpBoundary));
  Envelope::new(actor.clone(), Message::new(value), actor)
}

fn number(envelope: &Envelope) -> u32 {
  *envelope.message().downcast_ref::<u32>().expect("a u32 message")
}

fn unbounded() -> Arc<dyn MessageQueue> {
  Arc::new(UnboundedQueue::new(&SpinLockFactory))
}

fn mailbox(queue: Arc<dyn MessageQueue>, shared: bool) -> Mailbox<Keeper> {
  if shared {
    Mailbox::sharing(Keeper::default(), queue, RunLimits::new(Throughput::new(5)), &SpinLockFactory)
  } else {
    Mailbox::new(Keeper::default(), queue, RunLimits::new(Throughput::new(5)), &SpinLockFactory)
  }
}

#[derive(Clone, Copy, Debug)]
enum Op {
  // A sender enqueues a normal message to member 0 or 1.
  Send(usize),
  // A sender enqueues a system message to member 0 or 1.
  SystemSend(usize),
  // The queue hands out one message (whoever runs it).
  Take,
  // Member 0 or 1 takes one system message.
  SystemTake(usize),
  // Member 0 or 1 closes.
  Close(usize),
}

const ALPHABET: [Op; 9] = [
  Op::Send(0),
  Op::Send(1),
  Op::SystemSend(0),
  Op::SystemSend(1),
  Op::Take,
  Op::SystemTake(0),
  Op::SystemTake(1),
  Op::Close(0),
  Op::Close(1),
];

#[derive(Default)]
struct Tally {
  sent: usize,
  handled: usize,
  dead: usize,
  system_sent: [usize; 2],
  system_handled: [usize; 2],
  system_dead: [usize; 2],
}

// Runs `ops` on two mailboxes. With `shared`, both share one queue, as a
// balancing team does; without, each has its own.
fn check(ops: &[Op], shared: bool) {
  let shared_queue = unbounded();
  let members: [Mailbox<Keeper>; 2] = if shared {
    [mailbox(Arc::clone(&shared_queue), true), mailbox(Arc::clone(&shared_queue), true)]
  } else {
    [mailbox(unbounded(), false), mailbox(unbounded(), false)]
  };
  let mut tally = Tally::default();
  let mut next = 0;
  for (index, op) in ops.iter().enumerate() {
    let context = format!("shared={shared}: {:?}", &ops[..=index]);
    match *op {
      | Op::Send(to) => {
        tally.sent += 1;
        next += 1;
        let was_closed = members[to].is_closed();
        match members[to].enqueue(envelope(next)) {
          | Ok(()) => assert!(shared || !was_closed, "a closed mailbox took a message: {context}"),
          | Err(rejected) => {
            assert!(!shared && was_closed, "only the gate of a closed mailbox turns a message away: {context}");
            assert_eq!(number(&rejected), next, "{context}");
            tally.dead += 1;
          },
        }
      },
      | Op::SystemSend(to) => {
        tally.system_sent[to] += 1;
        let was_closed = members[to].is_closed();
        match members[to].system_enqueue(SystemMessage::Resume) {
          | Ok(()) => assert!(!was_closed, "a closed mailbox took a system message: {context}"),
          | Err(_) => {
            assert!(was_closed, "{context}");
            tally.system_dead[to] += 1;
          },
        }
      },
      | Op::Take => {
        // In a team the shared queue is one; each member's own queue otherwise.
        let taken = members.iter().find_map(|member| member.queue.dequeue());
        if taken.is_some() {
          tally.handled += 1;
        }
      },
      | Op::SystemTake(member) => {
        if members[member].gate.with(VecDeque::pop_front).is_some() {
          tally.system_handled[member] += 1;
        }
      },
      | Op::Close(member) => {
        let was_closed = members[member].is_closed();
        let (messages, system) = members[member].close();
        if was_closed || shared {
          assert!(messages.is_empty(), "closing gives no normal messages: {context}");
        }
        if was_closed {
          assert!(system.is_empty(), "closing twice gives nothing: {context}");
        }
        tally.dead += messages.len();
        tally.system_dead[member] += system.len();
        assert!(members[member].is_closed(), "{context}");
      },
    }
    let queued: usize =
      if shared { shared_queue.number_of_messages() } else { members.iter().map(Mailbox::number_of_messages).sum() };
    assert_eq!(tally.sent, tally.handled + queued + tally.dead, "every message is handled, queued or dead: {context}");
    if shared {
      assert_eq!(tally.dead, 0, "a team's queue never sends messages to dead letters: {context}");
    }
    for member in 0..2 {
      let system_queued = members[member].gate.with(|system| system.len());
      assert_eq!(
        tally.system_sent[member],
        tally.system_handled[member] + system_queued + tally.system_dead[member],
        "every system message is handled, queued or dead: {context}"
      );
      if members[member].is_closed() {
        assert_eq!(system_queued, 0, "a closed mailbox holds no system message: {context}");
        if !shared {
          assert_eq!(members[member].number_of_messages(), 0, "a closed mailbox holds no message: {context}");
        }
      }
    }
  }
}

fn sequences(max: usize, mut visit: impl FnMut(&[Op])) {
  let mut frontier: Vec<Vec<Op>> = std::vec![Vec::new()];
  visit(&[]);
  for _ in 0..max {
    let mut next = Vec::with_capacity(frontier.len() * ALPHABET.len());
    for sequence in &frontier {
      for op in ALPHABET {
        let mut longer = sequence.clone();
        longer.push(op);
        visit(&longer);
        next.push(longer);
      }
    }
    frontier = next;
  }
}

// Every sequence of up to 5 operations over two members.
#[test]
fn mailboxes_with_their_own_queues_turn_away_what_comes_after_closing() {
  sequences(5, |ops| check(ops, false));
}

#[test]
fn a_team_keeps_its_shared_queue_when_a_member_closes() {
  sequences(5, |ops| check(ops, true));
}

#[test]
fn closing_one_member_leaves_the_shared_queue_to_the_other() {
  let queue = unbounded();
  let first = Arc::new(mailbox(Arc::clone(&queue), true));
  let second = Arc::new(mailbox(Arc::clone(&queue), true));
  for value in 1..=3 {
    assert!(first.enqueue(envelope(value)).is_ok(), "a team's queue takes the message");
  }

  let (messages, system) = first.close();
  assert!(messages.is_empty() && system.is_empty());
  assert!(first.enqueue(envelope(4)).is_ok(), "a message for a closed member still goes to the team's queue");
  assert_eq!(queue.number_of_messages(), 4);
  assert!(Mailbox::try_schedule(&first).is_none(), "a closed member is never scheduled");

  let run = Mailbox::try_schedule(&second).expect("the other member has work");
  assert!(matches!(run.run(&mut (), None).outcome, RunOutcome::Idle));
  assert_eq!(second.take_cell().handled, [1, 2, 3, 4]);
  assert_eq!(queue.number_of_messages(), 0);
}

#[test]
fn closing_a_mailbox_with_its_own_queue_hands_back_what_it_held() {
  let mailbox = mailbox(unbounded(), false);
  assert!(mailbox.enqueue(envelope(1)).is_ok(), "open");
  assert!(mailbox.enqueue(envelope(2)).is_ok(), "open");
  mailbox.system_enqueue(SystemMessage::Suspend).expect("open");
  mailbox.system_enqueue(SystemMessage::Create).expect("open");

  let (messages, system) = mailbox.close();

  assert_eq!(messages.iter().map(number).collect::<Vec<_>>(), [1, 2]);
  assert!(matches!(system.as_slice(), [SystemMessage::Suspend, SystemMessage::Create]));
  assert_eq!(number(&mailbox.enqueue(envelope(3)).expect_err("closed")), 3);
  assert!(matches!(mailbox.system_enqueue(SystemMessage::Create), Err(SystemMessage::Create)));
  assert!(!mailbox.has_messages() && !mailbox.has_system_messages());
}

// A lock that panics when it is taken again while held, so that reaching the
// gate from inside the gate fails the test instead of hanging it.
struct CheckedLocks;

struct CheckedLock(std::sync::Mutex<()>, std::sync::atomic::AtomicBool);

// SAFETY: the mutex excludes and orders the sections; the flag only turns a
// re-entry, which would block forever, into a panic.
unsafe impl crate::kernel::sync::RawLock for CheckedLock {
  fn lock(&self, section: &mut dyn FnMut()) {
    use std::sync::atomic::Ordering;
    let _guard = self.0.try_lock().unwrap_or_else(|_| panic!("a lock was taken again while held"));
    self.1.store(true, Ordering::SeqCst);
    section();
    self.1.store(false, Ordering::SeqCst);
  }
}

impl crate::kernel::sync::LockFactory for CheckedLocks {
  fn new_lock(&self) -> std::boxed::Box<dyn crate::kernel::sync::RawLock> {
    std::boxed::Box::new(CheckedLock(std::sync::Mutex::new(()), std::sync::atomic::AtomicBool::new(false)))
  }
}

type Slot = Arc<std::sync::OnceLock<std::sync::Weak<Mailbox<Keeper>>>>;

fn priority_mailbox(
  priority: impl Fn(&Slot, &Message) -> crate::kernel::dispatch::MessagePriority + Send + Sync + 'static,
) -> (Arc<Mailbox<Keeper>>, Slot) {
  let slot: Slot = Arc::new(std::sync::OnceLock::new());
  let seen = Arc::clone(&slot);
  let queue =
    Arc::new(crate::kernel::dispatch::PriorityQueue::new(move |message| priority(&seen, message), true, &CheckedLocks));
  let mailbox = Arc::new(Mailbox::new(Keeper::default(), queue, RunLimits::new(Throughput::new(5)), &CheckedLocks));
  slot.set(Arc::downgrade(&mailbox)).expect("set once");
  (mailbox, slot)
}

// Invariant I16: the priority function is user code and runs outside the
// gate. Here it reaches the same mailbox's gate, which would panic inside it.
#[test]
fn the_priority_function_runs_outside_the_gate() {
  let (mailbox, _slot) = priority_mailbox(|slot, message| {
    let mailbox = slot.get().and_then(std::sync::Weak::upgrade).expect("the mailbox");
    assert!(!mailbox.has_system_messages());
    crate::kernel::dispatch::MessagePriority::new(*message.downcast_ref::<u32>().expect("a u32") as i32)
  });

  assert!(mailbox.enqueue(envelope(2)).is_ok());
  assert!(mailbox.enqueue(envelope(1)).is_ok());

  assert_eq!(mailbox.queue.drain().iter().map(number).collect::<Vec<_>>(), [1, 2]);
}

// A payload whose drop reaches the mailbox it was sent to.
struct ReachesTheGate(Slot, Arc<std::sync::atomic::AtomicBool>);

impl Drop for ReachesTheGate {
  fn drop(&mut self) {
    if let Some(mailbox) = self.0.get().and_then(std::sync::Weak::upgrade) {
      let _ = mailbox.has_system_messages();
      self.1.store(true, std::sync::atomic::Ordering::SeqCst);
    }
  }
}

// Invariant I16, the drop clause: when the priority function panics, the
// envelope is dropped outside the gate, so a drop that reaches the gate runs.
#[test]
fn an_envelope_is_dropped_outside_the_gate_when_the_priority_function_panics() {
  let (mailbox, slot) = priority_mailbox(|_, _| panic!("the priority function fails"));
  let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
  let root = RootActorPath::new(Address::new("fraktor", "mailbox-test"));
  let actor = next_actor_ref(ActorPath::from_root(root), Arc::new(NoOpBoundary));
  let payload = ReachesTheGate(Arc::clone(&slot), Arc::clone(&dropped));
  let sent = Envelope::new(actor.clone(), Message::new(payload), actor);

  let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| mailbox.enqueue(sent).is_ok()));

  assert!(unwound.is_err());
  assert!(dropped.load(std::sync::atomic::Ordering::SeqCst), "the drop reached the gate");
  assert!(!mailbox.has_messages());
}

// The ways the tests make and read a mailbox. A mailbox of an actor is made
// with `Mailbox::for_actor`, which names its dispatcher.
impl<C: MailboxCell> Mailbox<C> {
  /// Creates the mailbox of `cell` with a queue of its own and no way back to
  /// a dispatcher. Closing the mailbox takes what the queue still holds.
  pub(crate) fn new(
    cell: C,
    queue: Arc<dyn crate::kernel::dispatch::MessageQueue>,
    limits: RunLimits,
    locks: &dyn crate::kernel::sync::LockFactory,
  ) -> Self {
    Self::with_queue(Some(std::boxed::Box::new(cell)), queue, false, limits, locks, crate::kernel::sync::Weak::new())
  }

  /// Creates the mailbox of `cell` for a balancing team that shares `queue`.
  /// Closing the mailbox leaves the queue to the rest of the team.
  pub(crate) fn sharing(
    cell: C,
    queue: Arc<dyn crate::kernel::dispatch::MessageQueue>,
    limits: RunLimits,
    locks: &dyn crate::kernel::sync::LockFactory,
  ) -> Self {
    Self::with_queue(Some(std::boxed::Box::new(cell)), queue, true, limits, locks, crate::kernel::sync::Weak::new())
  }

  /// Returns whether the mailbox is suspended.
  pub(crate) fn is_suspended(&self) -> bool {
    self.status.is_suspended()
  }

  /// Returns whether the queue of normal messages holds a message. For a
  /// shared queue, the messages may be for any member of the team.
  pub(crate) fn has_messages(&self) -> bool {
    self.queue.has_messages()
  }

  /// Returns whether a system message is waiting.
  pub(crate) fn has_system_messages(&self) -> bool {
    self.has_queued_system_messages()
  }

  /// Returns whether the "has system messages" flag is up.
  pub(crate) fn status_has_system_flag(&self) -> bool {
    self.status.has_system_messages()
  }
}
