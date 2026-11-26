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

//! Unit tests for [`StdLockFactory`](super::StdLockFactory).

use std::panic;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;

use super::StdLockFactory;
use crate::kernel::sync::LockFactory;

#[test]
fn each_call_runs_the_section_once() {
  let lock = StdLockFactory.new_lock();
  let runs = AtomicU32::new(0);

  lock.lock(&mut || {
    runs.fetch_add(1, Ordering::SeqCst);
  });

  assert_eq!(runs.load(Ordering::SeqCst), 1);
}

// The unwind poisons the mutex; the lock ignores the poison and the next
// section runs.
#[test]
fn a_section_that_panics_does_not_stop_the_next_one() {
  let lock = StdLockFactory.new_lock();

  let unwound = panic::catch_unwind(panic::AssertUnwindSafe(|| lock.lock(&mut || panic!("inside the section"))));
  let ran = AtomicBool::new(false);
  lock.lock(&mut || ran.store(true, Ordering::SeqCst));

  assert!(unwound.is_err());
  assert!(ran.load(Ordering::SeqCst));
}

#[test]
fn two_sections_never_overlap() {
  let lock: Arc<dyn crate::kernel::sync::RawLock> = Arc::from(StdLockFactory.new_lock());
  let inside = Arc::new(AtomicU32::new(0));
  let overlaps = Arc::new(AtomicU32::new(0));
  let workers: std::vec::Vec<_> = (0..8)
    .map(|_| {
      let (lock, inside, overlaps) = (Arc::clone(&lock), Arc::clone(&inside), Arc::clone(&overlaps));
      thread::spawn(move || {
        for _ in 0..2_000 {
          lock.lock(&mut || {
            if inside.fetch_add(1, Ordering::SeqCst) != 0 {
              overlaps.fetch_add(1, Ordering::SeqCst);
            }
            inside.fetch_sub(1, Ordering::SeqCst);
          });
        }
      })
    })
    .collect();
  for worker in workers {
    worker.join().expect("a worker");
  }

  assert_eq!(overlaps.load(Ordering::SeqCst), 0);
}

// D1's second condition: the mailbox takes its shape from the std lock too.
// Four senders and a pool of two threads; every message is handled once, and
// no two runs of the mailbox overlap (invariant I1).
#[test]
fn a_mailbox_on_std_locks_handles_every_message_from_many_senders_once() {
  use std::sync::mpsc;
  use std::sync::Mutex;
  use std::time::Duration;
  use std::time::Instant;

  use crate::kernel::actor_path::ActorPath;
  use crate::kernel::actor_path::RootActorPath;
  use crate::kernel::actor_ref::next_actor_ref;
  use crate::kernel::address::Address;
  use crate::kernel::dispatch::Mailbox;
  use crate::kernel::dispatch::MailboxCell;
  use crate::kernel::dispatch::MailboxRun;
  use crate::kernel::dispatch::RunLimits;
  use crate::kernel::dispatch::RunOutcome;
  use crate::kernel::dispatch::SystemMessage;
  use crate::kernel::dispatch::Throughput;
  use crate::kernel::dispatch::UnboundedQueue;
  use crate::kernel::executor::NoOpBoundary;
  use crate::kernel::message::Envelope;
  use crate::kernel::message::Message;

  struct Counter {
    seen: Arc<Mutex<std::vec::Vec<u32>>>,
    running: Arc<AtomicBool>,
  }

  impl MailboxCell for Counter {
    fn invoke(&mut self, _mailbox: &Mailbox<Self>, envelope: Envelope) {
      assert!(!self.running.swap(true, Ordering::SeqCst), "two runs of one mailbox overlap");
      self.seen.lock().unwrap().push(*envelope.message().downcast_ref::<u32>().unwrap());
      self.running.store(false, Ordering::SeqCst);
    }

    fn system_invoke(&mut self, _mailbox: &Mailbox<Self>, _message: SystemMessage) {}
  }

  const SENDERS: u32 = 4;
  const EACH: u32 = 500;
  let seen = Arc::new(Mutex::new(std::vec::Vec::new()));
  let cell = Counter { seen: Arc::clone(&seen), running: Arc::new(AtomicBool::new(false)) };
  let queue = Arc::new(UnboundedQueue::new(&StdLockFactory));
  let mailbox = Arc::new(Mailbox::new(cell, queue, RunLimits::new(Throughput::new(5)), &StdLockFactory));
  // A lost message would keep the pool waiting; past this deadline the pool
  // stops, and the count below reports the loss instead of hanging.
  let deadline = Instant::now() + Duration::from_secs(30);
  let (submit, submitted) = mpsc::channel::<MailboxRun<Counter>>();
  let submitted = Arc::new(Mutex::new(submitted));

  let pool: std::vec::Vec<_> = (0..2)
    .map(|_| {
      let (submitted, again, seen) = (Arc::clone(&submitted), submit.clone(), Arc::clone(&seen));
      thread::spawn(move || loop {
        let next = submitted.lock().unwrap().recv_timeout(Duration::from_millis(10));
        match next {
          | Ok(run) => {
            if let RunOutcome::Again(run) = run.run() {
              again.send(run).unwrap();
            }
          },
          | Err(_) if Instant::now() > deadline || seen.lock().unwrap().len() == (SENDERS * EACH) as usize => return,
          | Err(_) => {},
        }
      })
    })
    .collect();
  let senders: std::vec::Vec<_> = (0..SENDERS)
    .map(|sender| {
      let (mailbox, submit) = (Arc::clone(&mailbox), submit.clone());
      thread::spawn(move || {
        let root = RootActorPath::new(Address::new("fraktor", "std-lock-test"));
        let actor = next_actor_ref(ActorPath::from_root(root), Arc::new(NoOpBoundary));
        for index in 0..EACH {
          let envelope = Envelope::new(actor.clone(), Message::new(sender * EACH + index), actor.clone());
          assert!(mailbox.enqueue(envelope).is_ok());
          if let Some(run) = Mailbox::try_schedule(&mailbox) {
            submit.send(run).unwrap();
          }
        }
      })
    })
    .collect();
  for sender in senders {
    sender.join().expect("a sender");
  }
  for worker in pool {
    worker.join().expect("a pool thread");
  }

  let mut seen = seen.lock().unwrap().clone();
  seen.sort_unstable();
  assert_eq!(seen, (0..SENDERS * EACH).collect::<std::vec::Vec<_>>());
}
