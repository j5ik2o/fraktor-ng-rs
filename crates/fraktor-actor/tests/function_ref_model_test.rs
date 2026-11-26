// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2018-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Model-based tests of FunctionRef (the design of #192,
// docs/design/issue-192-specs.md section 3, F1-F6). A fixed-seed
// Mirrors the registry semantics of Pekko's `org.apache.pekko.actor.FunctionRef`.
// SplitMix64 draws operations (add, remove, probe watch, ref watch,
// unwatch, send, and finally the parent's stop); a registry model
// predicts every Terminated delivery and every dead letter, and the
// probe's observations are compared against it. These are not ports
// of Pekko cases and are not counted in the ledger.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use fraktor_actor::adaptor::{ChannelBoundary, UnwindCatcher};
use fraktor_actor::kernel::actor::{Actor, ActorContext, DeadLetter, FunctionRef, Props, Terminated};
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

// The parent actor: creates function refs that forward a description
// of what they receive to the probe, and applies remove / watch /
// unwatch commands from the test.
struct Parent {
  probe: ActorRef,
  refs: Vec<FunctionRef>,
}

impl Actor for Parent {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(command) = message.downcast_ref::<String>().cloned() else {
      return;
    };
    let sender = context.sender();
    if command == "add" {
      let probe = self.probe.clone();
      let reference = context.add_function_ref(move |sender, forwarded| {
        let description = if forwarded.downcast_ref::<Terminated>().is_some() {
          String::from("Terminated")
        } else if let Some(text) = forwarded.downcast_ref::<String>() {
          text.clone()
        } else {
          String::from("other")
        };
        let from = sender.path().to_string();
        probe.tell((description, from), sender.clone());
      });
      self.refs.push(reference.clone());
      context.tell(&sender, reference);
      return;
    }
    let parts: Vec<&str> = command.splitn(3, ':').collect();
    match parts.as_slice() {
      | ["remove", index] => {
        let index: usize = index.parse().expect("index");
        context.remove_function_ref(&self.refs[index]);
      },
      | ["watch", watcher, target] => {
        let watcher: usize = watcher.parse().expect("watcher");
        let target: usize = target.parse().expect("target");
        let (watcher, target) = (self.refs[watcher].clone(), self.refs[target].clone());
        context.function_ref_watch(&watcher, &target.as_actor_ref());
      },
      | ["send", index, payload] => {
        let index: usize = index.parse().expect("index");
        let sender = context.sender();
        self.refs[index].tell(String::from(*payload), sender);
      },
      | ["unwatch", watcher, target] => {
        let watcher: usize = watcher.parse().expect("watcher");
        let target: usize = target.parse().expect("target");
        let (watcher, target) = (self.refs[watcher].clone(), self.refs[target].clone());
        context.function_ref_unwatch(&watcher, &target.as_actor_ref());
      },
      | _ => {},
    }
  }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Observed {
  // A real Terminated delivered to the probe: the ended ref's index,
  // or usize::MAX for the parent itself.
  ProbeTerminated(usize),
  // A Terminated forwarded by a watching ref: (watcher, target).
  ForwardedTerminated(usize, usize),
  // A payload forwarded by a ref: the payload text.
  Forwarded(String),
  // A dead letter: (payload, recipient index).
  Dead(String, usize),
}

struct Model {
  registered: Vec<bool>,
  probe_watching: Vec<bool>,
  watching: BTreeSet<(usize, usize)>,
}

impl Model {
  fn new() -> Self {
    Self { registered: Vec::new(), probe_watching: Vec::new(), watching: BTreeSet::new() }
  }

  // The deliveries one end makes, and the state change it causes.
  fn end(&mut self, target: usize) -> Vec<Observed> {
    let mut deliveries = Vec::new();
    if self.probe_watching[target] {
      deliveries.push(Observed::ProbeTerminated(target));
      self.probe_watching[target] = false;
    }
    for &(watcher, watched) in &self.watching {
      if watched == target && self.registered[watcher] {
        deliveries.push(Observed::ForwardedTerminated(watcher, target));
      }
    }
    self.watching.retain(|&(watcher, _)| watcher != target);
    self.registered[target] = false;
    deliveries
  }
}

fn classify(refs: &[FunctionRef], parent: &ActorRef, message: &Message) -> Observed {
  if let Some(terminated) = message.downcast_ref::<Terminated>() {
    let ended = terminated.target();
    if ended == parent {
      return Observed::ProbeTerminated(usize::MAX);
    }
    let index = refs.iter().position(|reference| reference.as_actor_ref() == *ended).expect("known ref");
    return Observed::ProbeTerminated(index);
  }
  if let Some((description, from)) = message.downcast_ref::<(String, String)>() {
    if description == "Terminated" {
      let target =
        refs.iter().position(|reference| reference.path().to_string() == *from).expect("the sender is the ended ref");
      // The watcher is not in the message; the model comparison is a
      // multiset match, so the watcher is recovered by the caller.
      return Observed::ForwardedTerminated(usize::MAX, target);
    }
    return Observed::Forwarded(description.clone());
  }
  if let Some(dead) = message.downcast_ref::<DeadLetter>() {
    let payload = dead.message().downcast_ref::<String>().cloned().unwrap_or_default();
    let index = refs.iter().position(|reference| reference.as_actor_ref() == dead.recipient()).expect("known ref");
    return Observed::Dead(payload, index);
  }
  panic!("unexpected message {message:?}");
}

fn drain(kit: &mut TestKit, refs: &[FunctionRef], parent: &ActorRef, count: usize) -> Vec<Observed> {
  let mut observed = Vec::new();
  for _ in 0..count {
    let received = kit.receive_n(1, Duration::from_secs(3));
    assert_eq!(received.len(), 1, "a predicted delivery did not arrive");
    observed.push(classify(refs, parent, received[0].message()));
  }
  observed
}

// ForwardedTerminated carries an unknown watcher from the wire; the
// model knows it. Compare with the watcher erased on both sides.
fn erased(deliveries: &[Observed]) -> Vec<Observed> {
  deliveries
    .iter()
    .map(|delivery| match delivery {
      | Observed::ForwardedTerminated(_, target) => Observed::ForwardedTerminated(usize::MAX, *target),
      | other => other.clone(),
    })
    .collect()
}

fn sorted(deliveries: &[Observed]) -> Vec<Observed> {
  let mut deliveries = erased(deliveries);
  deliveries.sort();
  deliveries
}

#[test]
fn function_ref_matches_the_registry_model() {
  let seed = 0xF192_u64;
  let mut rng = SplitMix64::new(seed);
  for round in 0..3 {
    let system = ActorSystem::with_setup_and_failure_catcher(
      "FunctionRefModel",
      Arc::new(ChannelBoundary::new()),
      ActorSystemSetup::empty(),
      Arc::new(UnwindCatcher),
    );
    let mut kit = TestKit::new(system);
    let test_actor = kit.test_actor();
    kit.system().event_stream().subscribe(Some(test_actor.clone()), EventClass::of::<DeadLetter>()).expect("subscribe");
    let parent = kit.system_mut().actor_of(Props::new({
      let probe = test_actor.clone();
      move || Parent { probe: probe.clone(), refs: Vec::new() }
    }));
    kit.watch(&parent);

    let mut model = Model::new();
    let mut refs: Vec<FunctionRef> = Vec::new();
    let mut sends = 0_u64;
    for step in 0..40 {
      let created = model.registered.len();
      let live: Vec<usize> = (0..created).filter(|&index| model.registered[index]).collect();
      let op = if created == 0 { 0 } else { rng.below(6) };
      match op {
        | 0 => {
          parent.tell(String::from("add"), test_actor.clone());
          let reference = kit.expect_msg_type::<FunctionRef>();
          refs.push(reference);
          model.registered.push(true);
          model.probe_watching.push(false);
        },
        | 1 if !live.is_empty() => {
          let target = live[rng.below(live.len() as u64) as usize];
          parent.tell(format!("remove:{target}"), test_actor.clone());
          let expected = model.end(target);
          let observed = drain(&mut kit, &refs, &parent, expected.len());
          assert_eq!(sorted(&observed), sorted(&expected), "seed {seed} round {round} step {step} remove {target}");
        },
        | 2 if !live.is_empty() => {
          let target = live[rng.below(live.len() as u64) as usize];
          if !model.probe_watching[target] {
            let target_ref = refs[target].as_actor_ref();
            kit.watch(&target_ref);
            model.probe_watching[target] = true;
          }
        },
        | 3 if created >= 2 => {
          let watcher = (rng.below(created as u64)) as usize;
          let target = (rng.below(created as u64)) as usize;
          if watcher != target && model.registered[watcher] {
            parent.tell(format!("watch:{watcher}:{target}"), test_actor.clone());
            if model.registered[target] {
              model.watching.insert((watcher, target));
            } else {
              // Watching an ended ref answers at once.
              let observed = drain(&mut kit, &refs, &parent, 1);
              assert_eq!(
                sorted(&observed),
                sorted(&[Observed::ForwardedTerminated(watcher, target)]),
                "seed {seed} round {round} step {step} watch ended {target}"
              );
            }
          }
        },
        | 4 if !model.watching.is_empty() => {
          let pairs: Vec<(usize, usize)> = model.watching.iter().copied().collect();
          let (watcher, target) = pairs[rng.below(pairs.len() as u64) as usize];
          parent.tell(format!("unwatch:{watcher}:{target}"), test_actor.clone());
          model.watching.remove(&(watcher, target));
        },
        | _ if created > 0 => {
          let target = (rng.below(created as u64)) as usize;
          sends += 1;
          let payload = format!("ping-{round}-{sends}");
          parent.tell(format!("send:{target}:{payload}"), test_actor.clone());
          let expected = if model.registered[target] {
            vec![Observed::Forwarded(payload)]
          } else {
            vec![Observed::Dead(payload, target)]
          };
          let observed = drain(&mut kit, &refs, &parent, 1);
          assert_eq!(sorted(&observed), sorted(&expected), "seed {seed} round {round} step {step} send {target}");
        },
        | _ => {},
      }
    }

    // The parent's stop ends every live ref and the parent itself.
    let mut expected = vec![Observed::ProbeTerminated(usize::MAX)];
    let live: Vec<usize> = (0..model.registered.len()).filter(|&index| model.registered[index]).collect();
    for target in live {
      expected.extend(model.end(target));
    }
    kit.system_mut().stop(&parent);
    let observed = drain(&mut kit, &refs, &parent, expected.len());
    assert_eq!(sorted(&observed), sorted(&expected), "seed {seed} round {round} parent stop");
  }
}
