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

// Stage 1 of #234 (issue #249): the speed measurement of design section 7.5.
//
// One ignored test measures S1 to S7 on the manual dispatcher
// (ActorSystem::new, single thread): 200,000 messages per run (100,000 for
// S7), seven runs per scenario, the median of the seven in nanoseconds per
// message. S5 to S7 came with stage 3 (issue #252): messages from outside to
// 100, 1,000 and 10,000 actors in turn. A
// control loop that never touches the executor is measured the same way;
// scripts/stage1-bench.sh runs this test alternately in two checkouts
// (A, B, B, A, A, B, B, A), discards a pair whose control values drift by
// more than 5 percent, and reports the per-pair B/A ratios. Speed is
// reported only; it never stops the work (docs/decisions/
// pekko-semantics-first.md).
//
// Run it with:
//   cargo test --release -p fraktor-actor --test actor_system_send_stage1_bench -- --ignored --nocapture

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;

const MESSAGES: usize = 200_000;
const RUNS: usize = 7;

struct Noop;

impl Actor for Noop {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

/// Sends itself the next message until it has handled `MESSAGES` in all.
struct SelfSender {
  handled: usize,
}

impl Actor for SelfSender {
  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    self.handled += 1;
    if self.handled < MESSAGES {
      let me = context.self_ref();
      me.tell(1_u32, me.clone());
    }
  }
}

/// Answers the sender until the shared counter reaches `MESSAGES`
/// deliveries between the two actors.
struct PingPong {
  handled: Arc<AtomicUsize>,
}

impl Actor for PingPong {
  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    if self.handled.fetch_add(1, Ordering::Relaxed) + 1 < MESSAGES {
      let me = context.self_ref();
      context.sender().tell(1_u32, me);
    }
  }
}

fn median_ns_per_message(mut samples: Vec<f64>) -> f64 {
  samples.sort_by(|a, b| a.partial_cmp(b).expect("samples are finite"));
  samples[samples.len() / 2]
}

fn bench_s1() -> f64 {
  let mut samples = Vec::new();
  for _ in 0..RUNS {
    let mut system = ActorSystem::new("Stage1BenchS1", Arc::new(ChannelBoundary::new()));
    let actor = system.actor_of(Props::new(|| Noop));
    let sender = system.dead_letters();
    let started = Instant::now();
    for _ in 0..MESSAGES {
      actor.tell(1_u32, sender.clone());
    }
    system.run_until_idle();
    samples.push(started.elapsed().as_nanos() as f64 / MESSAGES as f64);
    system.terminate();
  }
  median_ns_per_message(samples)
}

fn bench_s2() -> f64 {
  let mut samples = Vec::new();
  for _ in 0..RUNS {
    let mut system = ActorSystem::new("Stage1BenchS2", Arc::new(ChannelBoundary::new()));
    let actor = system.actor_of(Props::new(|| SelfSender { handled: 0 }));
    let sender = system.dead_letters();
    let started = Instant::now();
    actor.tell(1_u32, sender);
    system.run_until_idle();
    samples.push(started.elapsed().as_nanos() as f64 / MESSAGES as f64);
    system.terminate();
  }
  median_ns_per_message(samples)
}

fn bench_s3() -> f64 {
  let mut samples = Vec::new();
  for _ in 0..RUNS {
    let mut system = ActorSystem::new("Stage1BenchS3", Arc::new(ChannelBoundary::new()));
    let actor = system.actor_of(Props::new(|| SelfSender { handled: 0 }));
    let sender = system.dead_letters();
    let weak = system.downgrade();
    let started = Instant::now();
    actor.tell(1_u32, sender);
    while weak.run_one() {}
    samples.push(started.elapsed().as_nanos() as f64 / MESSAGES as f64);
    system.terminate();
  }
  median_ns_per_message(samples)
}

fn bench_s4() -> f64 {
  let mut samples = Vec::new();
  for _ in 0..RUNS {
    let mut system = ActorSystem::new("Stage1BenchS4", Arc::new(ChannelBoundary::new()));
    let handled = Arc::new(AtomicUsize::new(0));
    let first = system.actor_of(Props::new({
      let handled = Arc::clone(&handled);
      move || PingPong { handled: Arc::clone(&handled) }
    }));
    let second = system.actor_of(Props::new({
      let handled = Arc::clone(&handled);
      move || PingPong { handled: Arc::clone(&handled) }
    }));
    let started = Instant::now();
    first.tell(1_u32, second);
    system.run_until_idle();
    samples.push(started.elapsed().as_nanos() as f64 / MESSAGES as f64);
    system.terminate();
  }
  median_ns_per_message(samples)
}

/// S5 to S7: `messages` from outside to `actors` actors in turn, then
/// `run_until_idle`.
fn bench_many(actors: usize, messages: usize) -> f64 {
  let mut samples = Vec::new();
  for _ in 0..RUNS {
    let mut system = ActorSystem::new("Stage3BenchMany", Arc::new(ChannelBoundary::new()));
    let targets: Vec<_> = (0..actors).map(|_| system.actor_of(Props::new(|| Noop))).collect();
    let sender = system.dead_letters();
    let started = Instant::now();
    for index in 0..messages {
      targets[index % actors].tell(1_u32, sender.clone());
    }
    system.run_until_idle();
    samples.push(started.elapsed().as_nanos() as f64 / messages as f64);
    system.terminate();
  }
  median_ns_per_message(samples)
}

fn bench_control() -> f64 {
  let mut samples = Vec::new();
  for _ in 0..RUNS {
    let started = Instant::now();
    let mut acc = 0_u64;
    for i in 0..MESSAGES as u64 {
      acc = acc.wrapping_add(i ^ acc.rotate_left(7));
    }
    std::hint::black_box(acc);
    samples.push(started.elapsed().as_nanos() as f64 / MESSAGES as f64);
  }
  median_ns_per_message(samples)
}

#[test]
#[ignore = "speed measurement of design section 7.5; run it through scripts/stage1-bench.sh"]
fn stage1_bench_s1_to_s7() {
  println!("BENCH CONTROL {:.3}", bench_control());
  println!("BENCH S1 {:.3}", bench_s1());
  println!("BENCH S2 {:.3}", bench_s2());
  println!("BENCH S3 {:.3}", bench_s3());
  println!("BENCH S4 {:.3}", bench_s4());
  println!("BENCH S5 {:.3}", bench_many(100, MESSAGES));
  println!("BENCH S6 {:.3}", bench_many(1_000, MESSAGES));
  println!("BENCH S7 {:.3}", bench_many(10_000, MESSAGES / 2));
}
