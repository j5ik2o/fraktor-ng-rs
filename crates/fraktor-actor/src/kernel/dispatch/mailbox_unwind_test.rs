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

//! Unit tests for the end of a run that the actor panics out of, and for a
//! run of a closed mailbox (design of #234, sections 11.11.2 and 11.11.8;
//! `panicInvoke` and `panicSystem` of `specs/mailbox_scheduling.qnt`).

use std::panic;
use std::sync::Mutex;
use std::vec::Vec;

use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_path::RootActorPath;
use crate::kernel::actor_ref::next_actor_ref;
use crate::kernel::address::Address;
use crate::kernel::dispatch::Mailbox;
use crate::kernel::dispatch::MailboxCell;
use crate::kernel::dispatch::ManualExecutor;
use crate::kernel::dispatch::ManualTask;
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

// Panics on the message 13 and on a Suspend, and records the rest.
struct Fragile {
  seen: Arc<Mutex<Vec<u32>>>,
}

impl MailboxCell for Fragile {
  type Env = ();

  fn invoke(&mut self, _env: &mut (), _mailbox: &Mailbox<Self>, envelope: Envelope) -> Next {
    let value = *envelope.message().downcast_ref::<u32>().expect("a u32 message");
    assert_ne!(value, 13, "receive fails on 13");
    self.seen.lock().unwrap().push(value);
    Next::Continue
  }

  fn system_invoke(&mut self, _env: &mut (), _mailbox: &Mailbox<Self>, message: SystemMessage) {
    assert!(!matches!(message, SystemMessage::Suspend), "a system message fails");
  }
}

fn envelope(value: u32) -> Envelope {
  let root = RootActorPath::new(Address::new("fraktor", "mailbox-unwind-test"));
  let actor = next_actor_ref(ActorPath::from_root(root), Arc::new(NoOpBoundary));
  Envelope::without_recipient(Message::new(value), actor)
}

struct Fixture {
  executor: Arc<ManualExecutor<Fragile>>,
  mailbox: Arc<Mailbox<Fragile>>,
  seen: Arc<Mutex<Vec<u32>>>,
}

impl Fixture {
  fn new() -> Self {
    let executor = Arc::new(ManualExecutor::new(0, None, &SpinLockFactory));
    let queue = Arc::new(UnboundedQueue::new(&SpinLockFactory));
    let limits = RunLimits::new(Throughput::new(5));
    let mailbox = Arc::new(Mailbox::for_actor(queue, limits, &SpinLockFactory, Arc::downgrade(&executor)));
    let seen = Arc::new(Mutex::new(Vec::new()));
    mailbox.put_back_cell(std::boxed::Box::new(Fragile { seen: Arc::clone(&seen) }));
    Self { executor, mailbox, seen }
  }

  fn send(&self, value: u32) {
    assert!(Mailbox::send(&self.mailbox, envelope(value)).is_ok());
  }

  fn take_run(&self) -> crate::kernel::dispatch::MailboxRun<Fragile> {
    match self.executor.pop() {
      | Some(ManualTask::Run(run)) => run,
      | _ => panic!("no run waits"),
    }
  }

  fn run_unwinding(&self) {
    let run = self.take_run();
    let unwound = panic::catch_unwind(panic::AssertUnwindSafe(|| run.run(&mut (), None)));
    assert!(unwound.is_err(), "the run did not panic");
  }
}

// The guard does what the `finally` of Pekko's `Mailbox.run` does: the actor
// goes back, the bit comes down, and the mailbox is reserved again for the
// messages left, with the run handed to the dispatcher.
#[test]
fn a_panic_in_receive_clears_the_bit_and_hands_the_rest_to_the_dispatcher() {
  let fixture = Fixture::new();
  fixture.send(13);
  fixture.send(14);

  fixture.run_unwinding();

  assert!(fixture.mailbox.has_cell(), "the actor was not put back");
  let run = fixture.take_run();
  assert!(fixture.executor.is_empty());
  assert!(matches!(run.run(&mut (), None).outcome, RunOutcome::Idle));
  assert_eq!(*fixture.seen.lock().unwrap(), [14]);
}

// Without messages left, the bit still comes down: the next send reserves
// the mailbox again.
#[test]
fn a_panic_in_receive_leaves_the_mailbox_free_to_be_reserved() {
  let fixture = Fixture::new();
  fixture.send(13);

  fixture.run_unwinding();

  assert!(fixture.executor.is_empty(), "a run was handed over with nothing to do");
  fixture.send(15);
  let run = fixture.take_run();
  assert!(matches!(run.run(&mut (), None).outcome, RunOutcome::Idle));
  assert_eq!(*fixture.seen.lock().unwrap(), [15]);
}

// A panic while the system queue is being emptied leaves the flag down with
// a message still queued; the guard raises it again, so the next run does
// not skip that message (invariant noHiddenSystemMessage).
#[test]
fn a_panic_among_system_messages_raises_the_flag_again() {
  let fixture = Fixture::new();
  assert!(Mailbox::system_send(&fixture.mailbox, SystemMessage::Suspend).is_ok());
  assert!(fixture.mailbox.system_enqueue(SystemMessage::Resume).is_ok());

  fixture.run_unwinding();

  assert!(fixture.mailbox.has_system_messages());
  assert!(fixture.mailbox.status_has_system_flag(), "the flag stayed down with a message queued");
  let run = fixture.take_run();
  assert!(matches!(run.run(&mut (), None).outcome, RunOutcome::Idle));
  assert!(!fixture.mailbox.has_system_messages(), "the next run skipped the queued message");
}

// A run of a closed mailbox handles nothing and leaves the actor where it is
// (design of #234, section 11.11.2, "the start of a run").
#[test]
fn a_run_of_a_closed_mailbox_does_not_take_the_actor() {
  let fixture = Fixture::new();
  fixture.send(1);
  let run = fixture.take_run();
  let (held, _) = fixture.mailbox.close();
  assert_eq!(held.len(), 1);

  let report = run.run(&mut (), None);

  assert!(matches!(report.outcome, RunOutcome::Idle));
  assert_eq!(report.delivered, 0);
  assert!(fixture.mailbox.has_cell(), "the run took the actor out of a closed mailbox");
  assert!(fixture.seen.lock().unwrap().is_empty());
}
