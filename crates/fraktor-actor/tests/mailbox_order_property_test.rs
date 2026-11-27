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

//! Property P5 of stage 3 of #234 (design, section 11.11.8): with a mailbox
//! per actor and the hand-driven dispatcher, messages from one sender to one
//! recipient arrive in the order they were sent (invariant I9), and every
//! message arrives once or goes to dead letters (invariant I8).
//!
//! Every sequence of up to eight steps over two actors is tried; longer
//! sequences over up to six actors come from a fixed seed, so a failure
//! names a sequence that replays. No case is left out.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::{Actor, ActorContext, DeadLetter, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::message::Message;

/// A sender's tag: the external senders are 0, 1, 2, and actor `i` is
/// `ACTOR_TAG + i`.
const ACTOR_TAG: usize = 100;

/// A message that carries its sender's tag, its number in the order that
/// sender sent to this recipient, and an identifier of its own.
#[derive(Clone, Copy, Debug)]
struct Tagged {
  from: usize,
  seq: u64,
  id: u64,
}

/// Asks the actor to send a new `Tagged` to actor `to`.
#[derive(Clone, Copy, Debug)]
struct Forward {
  to: usize,
  id: u64,
}

#[derive(Default)]
struct Record {
  // (recipient, sender tag, seq) in the order the recipients received them.
  received: Vec<(usize, usize, u64)>,
  delivered: Vec<u64>,
  dead: Vec<u64>,
  sent: Vec<u64>,
}

struct Shared {
  record: Mutex<Record>,
  ids: AtomicU64,
  actors: Mutex<Vec<ActorRef>>,
}

impl Shared {
  fn next_id(&self) -> u64 {
    self.ids.fetch_add(1, Ordering::SeqCst)
  }

  fn sent(&self, id: u64) {
    self.record.lock().unwrap().sent.push(id);
  }
}

struct Node {
  index: usize,
  shared: Arc<Shared>,
  seqs: BTreeMap<usize, u64>,
}

impl Actor for Node {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(tagged) = message.downcast_ref::<Tagged>() {
      let mut record = self.shared.record.lock().unwrap();
      record.received.push((self.index, tagged.from, tagged.seq));
      record.delivered.push(tagged.id);
    } else if let Some(forward) = message.downcast_ref::<Forward>() {
      self.shared.record.lock().unwrap().delivered.push(forward.id);
      let seq = self.seqs.entry(forward.to).or_insert(0);
      *seq += 1;
      let tagged = Tagged { from: ACTOR_TAG + self.index, seq: *seq, id: self.shared.next_id() };
      self.shared.sent(tagged.id);
      let target = self.shared.actors.lock().unwrap()[forward.to].clone();
      context.tell(&target, tagged);
    }
  }
}

#[derive(Clone, Copy, Debug)]
enum Step {
  /// External sender `from` sends a `Tagged` to actor `to`.
  Send {
    from: usize,
    to: usize,
  },
  /// External sender `from` asks actor `via` to send to actor `to`.
  Forward {
    from: usize,
    via: usize,
    to: usize,
  },
  RunOne,
  RunUntilIdle,
  /// The system stops actor `actor`.
  Stop {
    actor: usize,
  },
}

struct World {
  system: ActorSystem,
  shared: Arc<Shared>,
  external: Vec<ActorRef>,
  seqs: BTreeMap<(usize, usize), u64>,
}

impl World {
  fn new(actors: usize, senders: usize) -> Self {
    let mut system = ActorSystem::new("MailboxOrderProperty", Arc::new(ChannelBoundary::new()));
    let shared = Arc::new(Shared {
      record: Mutex::new(Record::default()),
      ids: AtomicU64::new(0),
      actors: Mutex::new(Vec::new()),
    });
    let sink = Arc::clone(&shared);
    let collector = system.function_ref("dead-letters", move |_: &ActorRef, message: Message| {
      if let Some(letter) = message.downcast_ref::<DeadLetter>() {
        let mut record = sink.record.lock().unwrap();
        if let Some(tagged) = letter.message().downcast_ref::<Tagged>() {
          record.dead.push(tagged.id);
        } else if let Some(forward) = letter.message().downcast_ref::<Forward>() {
          record.dead.push(forward.id);
        }
      }
    });
    system.event_stream().subscribe(Some(collector), EventClass::of::<DeadLetter>()).expect("a collector");
    for index in 0..actors {
      let node_shared = Arc::clone(&shared);
      let actor =
        system.actor_of(Props::new(move || Node { index, shared: Arc::clone(&node_shared), seqs: BTreeMap::new() }));
      shared.actors.lock().unwrap().push(actor);
    }
    let external = (0..senders).map(|_| system.function_ref("sender", |_: &ActorRef, _: Message| {})).collect();
    Self { system, shared, external, seqs: BTreeMap::new() }
  }

  fn actor(&self, index: usize) -> ActorRef {
    self.shared.actors.lock().unwrap()[index].clone()
  }

  fn apply(&mut self, step: Step) {
    match step {
      | Step::Send { from, to } => {
        let seq = self.seqs.entry((from, to)).or_insert(0);
        *seq += 1;
        let tagged = Tagged { from, seq: *seq, id: self.shared.next_id() };
        self.shared.sent(tagged.id);
        self.actor(to).tell(tagged, self.external[from].clone());
      },
      | Step::Forward { from, via, to } => {
        let forward = Forward { to, id: self.shared.next_id() };
        self.shared.sent(forward.id);
        self.actor(via).tell(forward, self.external[from].clone());
      },
      | Step::RunOne => {
        self.system.run_one();
      },
      | Step::RunUntilIdle => self.system.run_until_idle(),
      | Step::Stop { actor } => {
        let actor = self.actor(actor);
        self.system.stop(&actor);
      },
    }
  }

  /// Drains the system and checks I9 and I8.
  fn check(mut self, steps: &[Step], context: &str) {
    self.system.run_until_idle();
    let record = self.shared.record.lock().unwrap();
    let mut last: BTreeMap<(usize, usize), u64> = BTreeMap::new();
    for &(to, from, seq) in &record.received {
      let previous = last.insert((from, to), seq).unwrap_or(0);
      assert!(seq > previous, "{context}: {from} -> {to} received {seq} after {previous}; steps {steps:?}");
    }
    let mut seen = BTreeSet::new();
    for id in record.delivered.iter().chain(&record.dead) {
      assert!(seen.insert(*id), "{context}: message {id} arrived twice; steps {steps:?}");
    }
    let sent: BTreeSet<u64> = record.sent.iter().copied().collect();
    assert_eq!(seen, sent, "{context}: a message was lost or made up; steps {steps:?}");
    drop(record);
    self.shared.actors.lock().unwrap().clear();
    self.system.terminate();
  }
}

const SHORT_STEPS: [Step; 4] = [
  Step::Send { from: 0, to: 0 },
  Step::Send { from: 0, to: 1 },
  Step::Forward { from: 0, via: 0, to: 1 },
  Step::RunOne,
];

#[test]
fn every_sequence_of_up_to_eight_steps_keeps_the_order_and_loses_nothing() {
  let mut tried = 0_u64;
  for length in 0..=8_u32 {
    for code in 0..SHORT_STEPS.len().pow(length) {
      let mut code = code;
      let steps: Vec<Step> = (0..length)
        .map(|_| {
          let step = SHORT_STEPS[code % SHORT_STEPS.len()];
          code /= SHORT_STEPS.len();
          step
        })
        .collect();
      let mut world = World::new(2, 1);
      for &step in &steps {
        world.apply(step);
      }
      world.check(&steps, "exhaustive");
      tried += 1;
    }
  }
  assert_eq!(tried, (0..=8).map(|length| 4_u64.pow(length)).sum::<u64>());
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

  fn below(&mut self, bound: usize) -> usize {
    (self.next() % bound as u64) as usize
  }
}

#[test]
fn random_long_sequences_keep_the_order_and_lose_nothing() {
  let seed = 0x0234_0252_u64;
  let mut rng = Rng(seed);
  for case in 0..500 {
    let actors = 1 + rng.below(6);
    let senders = 1 + rng.below(3);
    let length = 1 + rng.below(64);
    let steps: Vec<Step> = (0..length)
      .map(|_| match rng.below(20) {
        | 0..=7 => Step::Send { from: rng.below(senders), to: rng.below(actors) },
        | 8..=12 => Step::Forward { from: rng.below(senders), via: rng.below(actors), to: rng.below(actors) },
        | 13..=17 => Step::RunOne,
        | 18 => Step::RunUntilIdle,
        | _ => Step::Stop { actor: rng.below(actors) },
      })
      .collect();
    let mut world = World::new(actors, senders);
    for &step in &steps {
      world.apply(step);
    }
    world.check(&steps, &format!("seed {seed:#x}, case {case}"));
  }
}
