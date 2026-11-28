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

//! Unit tests for one run of a mailbox (design of #234, section 3.5): the
//! order of system and normal messages, the throughput and its deadline, the
//! end of a run, and the actor's slot (invariants I1, I7, I11 and I16).

use std::boxed::Box;
use std::format;
use std::panic;
use std::string::String;
use std::sync::Mutex;
use std::time::Duration;
use std::vec::Vec;

use portable_atomic::AtomicU64;
use portable_atomic::Ordering;

use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_path::RootActorPath;
use crate::kernel::actor_ref::next_actor_ref;
use crate::kernel::address::Address;
use crate::kernel::dispatch::Mailbox;
use crate::kernel::dispatch::MailboxCell;
use crate::kernel::dispatch::Next;
use crate::kernel::dispatch::RunLimits;
use crate::kernel::dispatch::RunOutcome;
use crate::kernel::dispatch::SystemMessage;
use crate::kernel::dispatch::Throughput;
use crate::kernel::dispatch::ThroughputDeadline;
use crate::kernel::dispatch::UnboundedQueue;
use crate::kernel::executor::NoOpBoundary;
use crate::kernel::message::Envelope;
use crate::kernel::message::Message;
use crate::kernel::sync::Arc;
use crate::kernel::sync::SpinLockFactory;
use crate::kernel::Clock;

type Hook = Box<dyn FnMut(&Mailbox<Recorder>, u32) + Send>;

// Records what it handles. A Resume resumes the mailbox and a Terminate
// closes it, as the actor cell does. `on_message` runs inside `invoke`, as
// `receive` would, and may suspend the mailbox, as a failure does.
struct Recorder {
  log: Arc<Mutex<Vec<String>>>,
  on_message: Hook,
}

impl MailboxCell for Recorder {
  type Env = ();

  fn invoke(&mut self, _env: &mut (), mailbox: &Mailbox<Self>, envelope: Envelope) -> Next {
    let value = *envelope.message().downcast_ref::<u32>().expect("a u32 message");
    self.log.lock().unwrap().push(format!("m{value}"));
    (self.on_message)(mailbox, value);
    Next::Continue
  }

  fn system_invoke(&mut self, _env: &mut (), mailbox: &Mailbox<Self>, message: SystemMessage) {
    let entry = match message {
      | SystemMessage::Resume { .. } => {
        mailbox.resume();
        String::from("resume")
      },
      | SystemMessage::Terminate => {
        let (messages, system) = mailbox.close();
        format!("terminate, {} and {} to dead letters", messages.len(), system.len())
      },
      | other => format!("{other:?}"),
    };
    self.log.lock().unwrap().push(entry);
  }
}

fn envelope(value: u32) -> Envelope {
  let root = RootActorPath::new(Address::new("fraktor", "mailbox-run-test"));
  let actor = next_actor_ref(ActorPath::from_root(root), Arc::new(NoOpBoundary));
  Envelope::new(actor.clone(), Message::new(value), actor)
}

struct Fixture {
  mailbox: Arc<Mailbox<Recorder>>,
  log: Arc<Mutex<Vec<String>>>,
}

impl Fixture {
  fn new(limits: RunLimits) -> Self {
    Self::with_hook(limits, Box::new(|_, _| {}))
  }

  fn with_hook(limits: RunLimits, on_message: Hook) -> Self {
    let log = Arc::new(Mutex::new(Vec::new()));
    let cell = Recorder { log: Arc::clone(&log), on_message };
    let queue = Arc::new(UnboundedQueue::new(&SpinLockFactory));
    Self { mailbox: Arc::new(Mailbox::new(cell, queue, limits, &SpinLockFactory)), log }
  }

  fn send(&self, value: u32) {
    assert!(self.mailbox.enqueue(envelope(value)).is_ok(), "the mailbox is open");
  }

  fn send_system(&self, message: SystemMessage) {
    assert!(self.mailbox.system_enqueue(message).is_ok(), "the mailbox is open");
  }

  // Runs the mailbox, and runs it again while the run says so, as a pool
  // does. Returns how many runs it took.
  fn drive(&self) -> usize {
    let mut runs = 0;
    let mut next = Mailbox::try_schedule(&self.mailbox);
    while let Some(run) = next {
      runs += 1;
      // A mailbox that keeps asking to run without work spins a pool thread
      // (mutation R4 of the design, section 3.10).
      assert!(runs <= 100, "the mailbox keeps running: {:?}", self.log.lock().unwrap());
      next = match run.run(&mut (), None).outcome {
        | RunOutcome::Again(again) => Some(again),
        | RunOutcome::Idle => None,
      };
    }
    runs
  }

  fn log(&self) -> Vec<String> {
    core::mem::take(&mut *self.log.lock().unwrap())
  }
}

#[test]
fn a_run_handles_at_most_the_throughput_and_asks_to_run_again() {
  let fixture = Fixture::new(RunLimits::new(Throughput::new(2)));
  for value in 1..=5 {
    fixture.send(value);
  }

  let run = Mailbox::try_schedule(&fixture.mailbox).expect("there is work");
  let RunOutcome::Again(run) = run.run(&mut (), None).outcome else { panic!("three messages are left") };
  assert_eq!(fixture.log(), ["m1", "m2"]);
  // The run that comes back still holds the scheduled bit, so nobody else
  // can schedule the mailbox meanwhile.
  assert!(Mailbox::try_schedule(&fixture.mailbox).is_none());
  let RunOutcome::Again(run) = run.run(&mut (), None).outcome else { panic!("one message is left") };
  assert_eq!(fixture.log(), ["m3", "m4"]);
  assert!(matches!(run.run(&mut (), None).outcome, RunOutcome::Idle));
  assert_eq!(fixture.log(), ["m5"]);
  assert!(Mailbox::try_schedule(&fixture.mailbox).is_none(), "nothing is left");
}

#[test]
fn a_throughput_of_zero_counts_as_one() {
  let fixture = Fixture::new(RunLimits::new(Throughput::new(0)));
  fixture.send(1);
  fixture.send(2);

  assert_eq!(fixture.drive(), 2);
  assert_eq!(fixture.log(), ["m1", "m2"]);
}

#[test]
fn waiting_system_messages_go_before_the_first_normal_message() {
  let fixture = Fixture::new(RunLimits::new(Throughput::new(5)));
  fixture.send(1);
  fixture.send_system(SystemMessage::Create);
  fixture.send_system(SystemMessage::Resume { caused_by_failure: false });

  fixture.drive();

  assert_eq!(fixture.log(), ["Create", "resume", "m1"]);
}

// A system message sent while `receive` runs is handled before the next
// normal message (invariant I11): its flag stops the run from taking one.
#[test]
fn a_system_message_sent_during_receive_goes_before_the_next_message() {
  let fixture = Fixture::with_hook(
    RunLimits::new(Throughput::new(5)),
    Box::new(|mailbox, value| {
      if value == 1 {
        assert!(mailbox.system_enqueue(SystemMessage::Create).is_ok());
      }
    }),
  );
  fixture.send(1);
  fixture.send(2);

  fixture.drive();

  assert_eq!(fixture.log(), ["m1", "Create", "m2"]);
}

#[test]
fn a_suspended_mailbox_handles_only_system_messages_until_resumed() {
  let fixture = Fixture::with_hook(
    RunLimits::new(Throughput::new(5)),
    Box::new(|mailbox, value| {
      if value == 0 {
        mailbox.suspend();
      }
    }),
  );
  fixture.send(0);
  fixture.send(1);

  fixture.drive();
  assert_eq!(fixture.log(), ["m0"]);
  assert!(fixture.mailbox.is_suspended());
  // Suspended with only a normal message, the mailbox has no work to run for.
  assert!(Mailbox::try_schedule(&fixture.mailbox).is_none());

  fixture.send_system(SystemMessage::Resume { caused_by_failure: false });
  fixture.drive();
  assert_eq!(fixture.log(), ["resume", "m1"]);
}

#[test]
fn terminate_closes_the_mailbox_and_the_run_stops() {
  let fixture = Fixture::new(RunLimits::new(Throughput::new(5)));
  fixture.send_system(SystemMessage::Terminate);
  fixture.send_system(SystemMessage::Create);
  fixture.send(1);

  fixture.drive();

  assert_eq!(fixture.log(), ["terminate, 1 and 1 to dead letters"]);
  assert!(fixture.mailbox.is_closed());
  assert!(fixture.mailbox.enqueue(envelope(2)).is_err());
  assert!(Mailbox::try_schedule(&fixture.mailbox).is_none());
}

// A message that comes while the run is ending is picked up by the run's own
// second reservation (`lateMessageIsRescheduledTest` of the spec).
#[test]
fn a_message_that_comes_during_the_last_receive_runs_again() {
  let fixture = Fixture::with_hook(
    RunLimits::new(Throughput::new(1)),
    Box::new(|mailbox, value| {
      if value == 1 {
        assert!(mailbox.enqueue(envelope(2)).is_ok());
      }
    }),
  );
  fixture.send(1);

  let run = Mailbox::try_schedule(&fixture.mailbox).expect("there is work");
  let RunOutcome::Again(run) = run.run(&mut (), None).outcome else { panic!("message 2 came during the run") };
  assert!(matches!(run.run(&mut (), None).outcome, RunOutcome::Idle));
  assert_eq!(fixture.log(), ["m1", "m2"]);
}

// Invariant I1: a run that finds the slot empty means another run of the same
// mailbox is going on.
#[test]
#[should_panic(expected = "invariant I1")]
fn a_run_without_the_actor_in_its_slot_panics() {
  let fixture = Fixture::new(RunLimits::new(Throughput::new(5)));
  fixture.send(1);
  let run = Mailbox::try_schedule(&fixture.mailbox).expect("there is work");
  let _actor = fixture.mailbox.take_cell();

  let _ = run.run(&mut (), None).outcome;
}

// The actor is put back when `receive` panics, so the next run does not take
// the empty slot for a second runner.
#[test]
fn the_actor_is_put_back_when_receive_panics() {
  let fixture = Fixture::with_hook(
    RunLimits::new(Throughput::new(5)),
    Box::new(|_, value| {
      assert_ne!(value, 13, "receive fails on 13");
    }),
  );
  fixture.send(13);
  let run = Mailbox::try_schedule(&fixture.mailbox).expect("there is work");

  let unwound = panic::catch_unwind(panic::AssertUnwindSafe(|| run.run(&mut (), None).outcome));

  assert!(unwound.is_err());
  let actor = fixture.mailbox.take_cell();
  fixture.mailbox.put_back_cell(actor);
}

#[derive(Default)]
struct HandClock(AtomicU64);

impl Clock for HandClock {
  fn monotonic(&self) -> Duration {
    Duration::from_nanos(self.0.load(Ordering::SeqCst))
  }
}

// I7: once the deadline has passed, the run takes no further message, but
// always handles the one it took.
#[test]
fn a_run_stops_taking_messages_after_the_deadline() {
  let clock = Arc::new(HandClock::default());
  let ticking = Arc::clone(&clock);
  let limits = RunLimits::new(Throughput::new(10))
    .with_deadline(ThroughputDeadline::new(Duration::from_millis(2)).expect("two milliseconds is a deadline"), clock);
  let fixture = Fixture::with_hook(
    limits,
    Box::new(move |_, _| {
      ticking.0.fetch_add(1_500_000, Ordering::SeqCst);
    }),
  );
  for value in 1..=4 {
    fixture.send(value);
  }

  let run = Mailbox::try_schedule(&fixture.mailbox).expect("there is work");
  let RunOutcome::Again(run) = run.run(&mut (), None).outcome else { panic!("messages are left") };
  // 1.5 ms after the first message, 3 ms after the second: past 2 ms.
  assert_eq!(fixture.log(), ["m1", "m2"]);
  assert!(matches!(run.run(&mut (), None).outcome, RunOutcome::Idle));
  assert_eq!(fixture.log(), ["m3", "m4"]);
}

// A deadline too far to add to the clock means no deadline, not a panic.
#[test]
fn a_deadline_of_duration_max_does_not_overflow() {
  let clock = Arc::new(HandClock::default());
  clock.0.store(1_000_000_000, Ordering::SeqCst);
  let deadline = ThroughputDeadline::new(Duration::MAX).expect("a deadline");
  let fixture = Fixture::new(RunLimits::new(Throughput::new(3)).with_deadline(deadline, clock));
  for value in 1..=3 {
    fixture.send(value);
  }

  assert_eq!(fixture.drive(), 1);
  assert_eq!(fixture.log(), ["m1", "m2", "m3"]);
}

#[test]
fn a_deadline_under_one_millisecond_counts_as_none() {
  assert_eq!(ThroughputDeadline::new(Duration::from_micros(999)), None);
  assert_eq!(ThroughputDeadline::new(Duration::ZERO), None);
  assert!(ThroughputDeadline::new(Duration::from_millis(1)).is_some());
}

// Unit tests for the end of a run that the actor panics out of, and for a
// run of a closed mailbox (design of #234, sections 11.11.2 and 11.11.8;
// `panicInvoke` and `panicSystem` of `specs/mailbox_scheduling.qnt`).
mod unwind {
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

  // Panics on the message 13 and on a Create, and records the rest.
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
      assert!(!matches!(message, SystemMessage::Create), "a system message fails");
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
      // The refused route every mailbox now requires: it records the
      // envelope the mailbox turns away (nothing is refused in this fixture,
      // so the list stays empty). Issue #296 makes the route a required
      // argument of `for_actor` (design section 4.1).
      let refused: Arc<Mutex<Vec<crate::kernel::message::Envelope>>> = Arc::new(Mutex::new(Vec::new()));
      let mailbox = Arc::new(Mailbox::for_actor(
        queue,
        limits,
        &SpinLockFactory,
        Arc::downgrade(&executor),
        Arc::new(move |envelope: crate::kernel::message::Envelope| {
          refused.lock().unwrap().push(envelope);
        }),
      ));
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
    assert!(Mailbox::system_send(&fixture.mailbox, SystemMessage::Create).is_ok());
    assert!(fixture.mailbox.system_enqueue(SystemMessage::Resume { caused_by_failure: false }).is_ok());

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

  // A send reserves the mailbox, a stop closes it and takes the runs that
  // wait, and only then does the send hand its run over: the run of the
  // closed mailbox must not stay behind.
  #[test]
  fn a_run_handed_over_after_the_stop_took_the_runs_is_refused() {
    let fixture = Fixture::new();
    assert!(fixture.mailbox.enqueue(envelope(1)).is_ok());
    let run = Mailbox::try_schedule(&fixture.mailbox).expect("there is work");

    let _ = fixture.mailbox.close();
    assert!(fixture.executor.take_runs_of(&fixture.mailbox).is_empty());
    fixture.executor.submit(ManualTask::Run(run));

    assert!(fixture.executor.is_empty(), "the run of a closed mailbox stayed behind");
  }
}
