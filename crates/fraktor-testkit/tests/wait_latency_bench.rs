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

// Stage 3 of #234 (issue #252): how long the testkit waits for a message that
// another thread sends (design, section 11.11.6). The wait sleeps on the
// system's wake signal; were it to fall back to sleeping in steps of its
// poll interval (10 ms), the times below would sit near that interval.
//
// Run it with:
//   cargo test --release -p fraktor-testkit --test wait_latency_bench -- --ignored --nocapture

use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::test_kit::TestKit;

const SAMPLES: usize = 200;

/// Sends every message it receives on to `to`.
struct Relay {
  to: ActorRef,
}

impl Actor for Relay {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    context.tell(&self.to, message);
  }
}

fn summary(label: &str, mut waits: Vec<Duration>) {
  waits.sort();
  let at = |q: f64| waits[((waits.len() - 1) as f64 * q) as usize];
  println!(
    "WAIT {label}: median {:?}, p90 {:?}, p99 {:?}, max {:?} over {} sends",
    at(0.5),
    at(0.9),
    at(0.99),
    waits[waits.len() - 1],
    waits.len()
  );
}

/// Another thread sends to `target` after a short pause; returns the time
/// from the send to `expect_msg` returning, one per sample.
fn measure(kit: &mut TestKit, target: ActorRef) -> Vec<Duration> {
  let mut waits = Vec::with_capacity(SAMPLES);
  for _ in 0..SAMPLES {
    let (sent_at, sent) = mpsc::channel();
    let to = target.clone();
    let sender = kit.test_actor();
    let sending = thread::spawn(move || {
      thread::sleep(Duration::from_millis(2));
      let at = Instant::now();
      to.tell("ping", sender);
      sent_at.send(at).unwrap();
    });
    kit.expect_msg(&"ping");
    let returned = Instant::now();
    sending.join().unwrap();
    waits.push(returned.duration_since(sent.recv().unwrap()));
  }
  waits
}

#[test]
#[ignore = "wait measurement of design section 11.11.6; run it by hand"]
fn testkit_waits_end_when_the_message_comes() {
  let system = ActorSystem::new("WaitLatency", Arc::new(ChannelBoundary::new()));
  let mut kit = TestKit::new(system);

  let direct = kit.test_actor();
  summary("to the test actor from another thread", measure(&mut kit, direct));

  let to = kit.test_actor();
  let relay = kit.system_mut().actor_of(Props::new(move || Relay { to: to.clone() }));
  summary("to an actor from another thread, relayed to the test actor", measure(&mut kit, relay));
}
