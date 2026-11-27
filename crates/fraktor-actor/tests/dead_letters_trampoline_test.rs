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
// Licensed under the Apache License, version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Issue #296, middle 1 (design `docs/design/issue-296-297-dead-letters-followup.md`,
// section 7.1): a dead letter subscriber that resends to a stopped actor must
// not nest its delivery in the delivery that was already running, or a long
// enough chain overflows the sender's stack.
//
// In Pekko 2.0.0 a stopped actor's mailbox is the dead letter mailbox, whose
// `enqueue` sends the message to `deadLetters`. A subscriber that runs where it
// is told (a function reference, as the test actor on the calling-thread
// dispatcher) sees it, and a handler that resends to a stopped actor sends
// again from inside the first delivery. Pekko's dispatcher keeps that flat
// because the subscriber is an actor whose mailbox handles one message after
// another; the port delivers the subscriber on the sender's stack, so the
// resend recurses. The design fixes it with a trampoline (design section 3):
// only the outermost publication runs the loop, and a nested publication is
// queued and run by that loop in first-in, first-out order.
//
// This file is the red test for that fix. It is kept apart from
// `dead_letters_on_sender_thread_test.rs` because today 1000 resends overflow
// the stack and abort the whole test binary (design section 7.1, review middle
// 5): keeping it in its own file keeps the other tests runnable. Run it alone:
//
//   cargo test -p fraktor-actor --test dead_letters_trampoline_test
//
// The test is split in two stages, as the design asks:
//
//   (i) a stage that uses only the public surface that exists today
//       (`ActorSystem::new`, `function_ref`, the event stream, `ActorRef::tell`).
//       Before the fix, 1000 resends overflowed the thread's stack and the
//       process aborted with `fatal runtime error: stack overflow, aborting`
//       (a process crash, not an assertion); that was the red for the fix.
//
//   (ii) the stage that drives the trampoline explicitly, through
//        `ActorSystem::with_dead_letter_trampoline` and
//        `SenderThreadTrampoline`, which #296 adds, so the std adaptor's
//        thread-local marker, queue, and `try_with` are exercised even though
//        the observation matches the default `SharedTrampoline` on one thread.

use std::string::String;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread::ThreadId;
use std::vec::Vec;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::adaptor::SenderThreadTrampoline;
use fraktor_actor::kernel::actor::Actor;
use fraktor_actor::kernel::actor::ActorContext;
use fraktor_actor::kernel::actor::DeadLetter;
use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::message::Message;

/// How many times the subscriber resends. The last resend carries the final
/// number, so the subscriber expects `RESENDS + 1` letters, numbered 0 to
/// `RESENDS`.
const RESENDS: u32 = 1000;

/// The stack the resends run on. Small enough that 1000 nested deliveries
/// overflow it on every platform, and fixed so the test does not depend on
/// the platform's default stack size (design section 7.1).
const STACK_BYTES: usize = 512 * 1024;

struct Sink;

impl Actor for Sink {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

/// What the subscriber saw, in the order it saw it, and on which thread.
#[derive(Clone, Debug, Default)]
struct Seen {
  numbers: Vec<u32>,
  threads: Vec<ThreadId>,
}

type Log = Arc<Mutex<Seen>>;

/// Subscribes a function reference to `DeadLetter`.
///
/// The handler reads the number out of the `DeadLetter`'s payload and, while
/// it is below `final_number`, resends the next number to the same stopped
/// actor. Every resend is another dead letter, delivered from inside this
/// one: that is the nesting the trampoline is to flatten.
fn subscribe_resender(system: &mut ActorSystem, target: ActorRef, final_number: u32) -> Log {
  let log: Log = Arc::new(Mutex::new(Seen::default()));
  let sink = Arc::clone(&log);
  let collector = system.function_ref("dead-letter-resender", move |_self: &ActorRef, message: Message| {
    let Some(letter) = message.downcast_ref::<DeadLetter>() else {
      return;
    };
    let Some(&number) = letter.message().downcast_ref::<u32>() else {
      return;
    };
    sink.lock().unwrap().numbers.push(number);
    sink.lock().unwrap().threads.push(std::thread::current().id());
    if number < final_number {
      target.tell(number + 1, letter.sender().clone());
    }
  });
  let stream = system.event_stream();
  stream.subscribe(Some(collector), EventClass::of::<DeadLetter>()).expect("a subscriber");
  log
}

/// Starts an actor and stops it, so its mailbox is closed.
fn stopped_actor(system: &mut ActorSystem) -> ActorRef {
  let target = system.actor_of(Props::new(|| Sink));
  system.stop(&target);
  system.run_until_idle();
  assert!(system.is_actor_terminated(&target), "the actor has stopped");
  target
}

// Stage (i): the public surface that exists today. On the fixed stack, 1000
// resends overflow the stack and abort the process, which is the red for
// #296. Once the trampoline is in place this test runs the resends flat and
// passes with the four observations the design names:
//
// - the 1000 resends finish without overflowing the stack,
// - the subscriber receives all 1001 letters,
// - the numbers arrive in order, 0 to 1000,
// - every handler runs on the thread that made the outermost `tell`, and all
//   1001 letters are there by the time that `tell` returns (no
//   `run_until_idle` in between).
#[test]
fn a_thousand_resends_to_a_stopped_actor_finish_flat_on_the_senders_thread() {
  let mut system = ActorSystem::new("DeadLettersTrampoline", Arc::new(ChannelBoundary::new()));
  let target = stopped_actor(&mut system);
  let log = subscribe_resender(&mut system, target.clone(), RESENDS);
  let sender = system.function_ref("sender", |_: &ActorRef, _: Message| {});

  let outer_thread = std::thread::current().id();
  let seen = {
    let target = target.clone();
    let log = Arc::clone(&log);
    let sender = sender.clone();
    std::thread::Builder::new()
      .name(String::from("trampoline-resends"))
      .stack_size(STACK_BYTES)
      .spawn(move || {
        // The outermost send. If the trampoline works, every nested resend is
        // run by this call's loop, on this thread, before it returns.
        target.tell(0u32, sender);
        (std::thread::current().id(), log.lock().unwrap().clone())
      })
      .expect("the resend thread")
      .join()
      .expect("the resend thread finishes")
  };
  let (sending_thread, seen) = seen;

  assert_eq!(
    seen.numbers.len(),
    (RESENDS + 1) as usize,
    "the subscriber receives every letter before the outermost tell returns: {} received",
    seen.numbers.len()
  );
  let expected: Vec<u32> = (0..=RESENDS).collect();
  assert_eq!(seen.numbers, expected, "the numbers arrive in order, 0 to {RESENDS}");
  assert!(
    seen.threads.iter().all(|thread| *thread == sending_thread),
    "every handler runs on the thread that made the outermost tell"
  );
  assert_ne!(sending_thread, outer_thread);
}

// Stage (ii): the same resends, with the standard library's trampoline
// installed explicitly through `ActorSystem::with_dead_letter_trampoline`,
// so the thread-local marker, the queue, and `try_with` of
// `SenderThreadTrampoline` are exercised even though the single-thread
// observation is the same as the default `SharedTrampoline` (design section
// 3.1 and 7.1). The four observations are the ones the design lists:
// 1. the 1000 resends finish without overflowing the stack (the join returns);
// 2. the subscriber receives all 1001 letters;
// 3. the numbers arrive in order, 0 to 1000;
// 4. every handler runs on the thread that made the outermost `tell`, and all
//    1001 letters are there by the time that `tell` returns (no
//    `run_until_idle` in between).
#[test]
fn a_thousand_resends_finish_flat_with_the_sender_thread_trampoline() {
  let mut system = ActorSystem::with_dead_letter_trampoline(
    "DeadLettersSenderThreadTrampoline",
    Arc::new(ChannelBoundary::new()),
    Arc::new(SenderThreadTrampoline::new()),
  );
  let target = stopped_actor(&mut system);
  let log = subscribe_resender(&mut system, target.clone(), RESENDS);
  let sender = system.function_ref("sender", |_: &ActorRef, _: Message| {});

  let outer_thread = std::thread::current().id();
  let seen = {
    let target = target.clone();
    let log = Arc::clone(&log);
    let sender = sender.clone();
    std::thread::Builder::new()
      .name(String::from("trampoline-resends"))
      .stack_size(STACK_BYTES)
      .spawn(move || {
        let sending_thread = std::thread::current().id();
        target.tell(0u32, sender);
        (sending_thread, log.lock().unwrap().clone())
      })
      .expect("the resend thread")
      .join()
      .expect("the resend thread finishes")
  };
  let (sending_thread, seen) = seen;

  assert_eq!(
    seen.numbers.len(),
    (RESENDS + 1) as usize,
    "the subscriber receives every letter before the outermost tell returns: {} received",
    seen.numbers.len()
  );
  let expected: Vec<u32> = (0..=RESENDS).collect();
  assert_eq!(seen.numbers, expected, "the numbers arrive in order, 0 to {RESENDS}");
  assert!(
    seen.threads.iter().all(|thread| *thread == sending_thread),
    "every handler runs on the thread that made the outermost tell"
  );
  assert_ne!(sending_thread, outer_thread);
}
