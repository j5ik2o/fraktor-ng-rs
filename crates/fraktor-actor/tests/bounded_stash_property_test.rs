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

// Property-based tests of the bounded stash (the design of #192,
// docs/design/issue-192-specs.md section 3, B1-B4). A fixed-seed
// Mirrors the bounded stash semantics of Pekko's `org.apache.pekko.actor.Stash`.
// SplitMix64 draws the capacities and the number of stashed messages;
// the actor's observed replies, forwarded messages, and dead letters
// are compared against a VecDeque model of the stash and the bounded
// mailbox. These are not ports of Pekko cases and are not counted in
// the ledger.

use std::collections::VecDeque;
use std::sync::Arc;

use fraktor_actor::adaptor::{ChannelBoundary, UnwindCatcher};
use fraktor_actor::kernel::actor::dispatch_settings::{DispatchSettings, MailboxDefinition};
use fraktor_actor::kernel::actor::{Actor, ActorContext, DeadLetter, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::setup::ActorSystemSetup;
use fraktor_testkit::test_kit::TestKit;

struct SplitMix64 {
  state: u64,
}

impl SplitMix64 {
  fn new(seed: u64) -> Self {
    Self { state: seed }
  }

  fn next_u64(&mut self) -> u64 {
    self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }

  fn below(&mut self, bound: u64) -> u64 {
    self.next_u64() % bound
  }
}

// Stashes until "world", answering "ok" for a stashed message and
// "overflow" for one the stash capacity refuses; on "world" it
// unstashes and forwards everything it receives from then on.
struct ModelStasher {
  probe: ActorRef,
  forwarding: bool,
}

impl Actor for ModelStasher {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if self.forwarding {
      context.tell(&self.probe, message);
      return;
    }
    if message == "world" {
      self.forwarding = true;
      context.unstash_all();
      return;
    }
    let sender = context.sender();
    match context.try_stash() {
      | Ok(()) => context.tell(&sender, "ok"),
      | Err(_) => context.tell(&sender, "overflow"),
    }
  }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Observed {
  Text(String),
  Dead(String),
}

#[test]
fn bounded_stash_matches_the_vecdeque_model() {
  let seed = 0xB0ADED;
  let mut rng = SplitMix64::new(seed);
  for round in 0..40 {
    let mailbox_cap = 1 + rng.below(3) as usize; // 1..=3
    let stash_cap = 1 + rng.below(4) as usize; // 1..=4
    let count = rng.below(9) as usize; // 0..=8 messages

    let mut settings = DispatchSettings::new();
    settings.define_dispatcher("model-dispatcher", MailboxDefinition::bounded(mailbox_cap, stash_cap));
    let setup = ActorSystemSetup::empty().with_setup(settings);
    let system = ActorSystem::with_setup_and_failure_catcher(
      "BoundedStashProperty",
      Arc::new(ChannelBoundary::new()),
      setup,
      Arc::new(UnwindCatcher),
    );
    let mut kit = TestKit::new(system);
    let test_actor = kit.test_actor();
    kit.system().event_stream().subscribe(Some(test_actor.clone()), EventClass::of::<DeadLetter>()).expect("subscribe");
    let stasher = kit.system_mut().actor_of(
      Props::new({
        let probe = test_actor.clone();
        move || ModelStasher { probe: probe.clone(), forwarding: false }
      })
      .with_dispatcher("model-dispatcher"),
    );

    // Model: the stash holds the first `stash_cap` messages; the rest
    // are refused. At unstash, the mailbox (empty here) keeps the
    // newest `mailbox_cap` stashed messages and the oldest excess
    // goes to dead letters, newest victim first (B3).
    let mut stash: VecDeque<String> = VecDeque::new();
    let mut expected: Vec<Observed> = Vec::new();
    for i in 0..count {
      let text = format!("m{i}");
      let fits = stash.len() < stash_cap;
      if fits {
        stash.push_back(text.clone());
      }
      stasher.tell(Box::leak(text.clone().into_boxed_str()) as &'static str, test_actor.clone());
      let reply = kit.expect_msg_type::<&'static str>();
      assert_eq!(reply, if fits { "ok" } else { "overflow" }, "seed {seed} round {round} message {i}");
    }
    let room = mailbox_cap;
    let excess = stash.len().saturating_sub(room);
    let victims: Vec<String> = (0..excess).filter_map(|_| stash.pop_front()).collect();
    for victim in victims.iter().rev() {
      expected.push(Observed::Dead(victim.clone()));
    }
    for kept in stash.iter() {
      expected.push(Observed::Text(kept.clone()));
    }

    stasher.tell("world", test_actor.clone());
    let mut observed: Vec<Observed> = Vec::new();
    for _ in 0..expected.len() {
      let received = kit.receive_n(1, std::time::Duration::from_secs(3));
      assert_eq!(received.len(), 1, "a predicted observation did not arrive");
      let message = received[0].message();
      if let Some(dead) = message.downcast_ref::<DeadLetter>() {
        observed.push(Observed::Dead(dead.message().downcast_ref::<&'static str>().copied().unwrap_or("").to_string()));
      } else if let Some(text) = message.downcast_ref::<&'static str>() {
        observed.push(Observed::Text((*text).to_string()));
      } else {
        panic!("unexpected message {message:?}");
      }
    }
    assert_eq!(observed, expected, "seed {seed} round {round} mailbox {mailbox_cap} stash {stash_cap} count {count}");
  }
}
