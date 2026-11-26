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

//! P2 of the design of #234 (section 7.3): the order in which each kernel
//! queue hands out envelopes, against a model of the queue.

use std::collections::VecDeque;
use std::vec::Vec;

use super::MessageQueue;
use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_path::RootActorPath;
use crate::kernel::actor_ref::next_actor_ref;
use crate::kernel::address::Address;
use crate::kernel::dispatch::BoundedQueue;
use crate::kernel::dispatch::ControlAwareQueue;
use crate::kernel::dispatch::MessagePriority;
use crate::kernel::dispatch::PriorityQueue;
use crate::kernel::dispatch::UnboundedQueue;
use crate::kernel::executor::NoOpBoundary;
use crate::kernel::message::Envelope;
use crate::kernel::message::Message;
use crate::kernel::sync::Arc;
use crate::kernel::sync::SpinLockFactory;

// A message: its number, and a class in 0..3 that is its priority or, for the
// control-aware queue, a control message when it is 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Item {
  id: u32,
  class: i32,
}

fn envelope(item: Item) -> Envelope {
  let root = RootActorPath::new(Address::new("fraktor", "message-queue-test"));
  let actor = next_actor_ref(ActorPath::from_root(root), Arc::new(NoOpBoundary));
  Envelope::new(actor.clone(), Message::new(item), actor)
}

fn item_of(envelope: &Envelope) -> Item {
  *envelope.message().downcast_ref::<Item>().expect("an Item message")
}

fn class_of(message: &Message) -> i32 {
  message.downcast_ref::<Item>().expect("an Item message").class
}

// The class as a priority: class 0 goes first.
fn priority_of(message: &Message) -> MessagePriority {
  MessagePriority::new(class_of(message))
}

#[derive(Clone, Copy, Debug)]
enum Op {
  Enqueue(i32),
  Dequeue,
}

const ALPHABET: [Op; 4] = [Op::Enqueue(0), Op::Enqueue(1), Op::Enqueue(2), Op::Dequeue];

/// Every sequence over the alphabet up to `max` long: enqueues of every class,
/// with repeats, mixed with dequeues.
fn sequences(max: usize) -> Vec<Vec<Op>> {
  let mut all = std::vec![Vec::new()];
  let mut frontier: Vec<Vec<Op>> = std::vec![Vec::new()];
  for _ in 0..max {
    let mut next = Vec::new();
    for sequence in &frontier {
      for op in ALPHABET {
        let mut longer = sequence.clone();
        longer.push(op);
        next.push(longer);
      }
    }
    all.extend(next.iter().cloned());
    frontier = next;
  }
  all
}

/// What the model of each queue does.
trait Model {
  /// Returns whether the model accepts the item.
  fn enqueue(&mut self, item: Item) -> bool;
  /// Checks the item the queue handed out, and takes it out of the model.
  fn check_dequeued(&mut self, got: Option<Item>, context: &str);
  fn len(&self) -> usize;
}

struct Fifo {
  items: VecDeque<Item>,
  capacity: Option<usize>,
}

impl Model for Fifo {
  fn enqueue(&mut self, item: Item) -> bool {
    if self.capacity.is_some_and(|capacity| self.items.len() >= capacity) {
      return false;
    }
    self.items.push_back(item);
    true
  }

  fn check_dequeued(&mut self, got: Option<Item>, context: &str) {
    assert_eq!(got, self.items.pop_front(), "{context}");
  }

  fn len(&self) -> usize {
    self.items.len()
  }
}

struct Priority {
  items: Vec<Item>,
  stable: bool,
}

impl Model for Priority {
  fn enqueue(&mut self, item: Item) -> bool {
    self.items.push(item);
    true
  }

  fn check_dequeued(&mut self, got: Option<Item>, context: &str) {
    let Some(lowest) = self.items.iter().map(|item| item.class).min() else {
      assert_eq!(got, None, "{context}");
      return;
    };
    let got = got.unwrap_or_else(|| panic!("an envelope is queued: {context}"));
    assert_eq!(got.class, lowest, "the highest priority goes first: {context}");
    let position = if self.stable {
      // Within one priority, the earliest goes first.
      self.items.iter().position(|item| item.class == lowest).expect("a lowest item")
    } else {
      self.items.iter().position(|item| *item == got).unwrap_or_else(|| panic!("{got:?} was queued: {context}"))
    };
    assert_eq!(self.items.remove(position), got, "{context}");
  }

  fn len(&self) -> usize {
    self.items.len()
  }
}

struct ControlAware {
  control: VecDeque<Item>,
  normal: VecDeque<Item>,
}

impl Model for ControlAware {
  fn enqueue(&mut self, item: Item) -> bool {
    if item.class == 0 {
      self.control.push_back(item)
    } else {
      self.normal.push_back(item)
    }
    true
  }

  fn check_dequeued(&mut self, got: Option<Item>, context: &str) {
    let expected = self.control.pop_front().or_else(|| self.normal.pop_front());
    assert_eq!(got, expected, "{context}");
  }

  fn len(&self) -> usize {
    self.control.len() + self.normal.len()
  }
}

/// Runs every sequence on a new queue and a new model, comparing after each
/// step, and drains what is left at the end.
fn check_queue<Q: MessageQueue, M: Model>(
  name: &str,
  max: usize,
  new_queue: impl Fn() -> Q,
  new_model: impl Fn() -> M,
) {
  for ops in sequences(max) {
    let queue = new_queue();
    let mut model = new_model();
    let mut next_id = 0;
    for (index, op) in ops.iter().enumerate() {
      let context = format!("{name}: {:?}", &ops[..=index]);
      match *op {
        | Op::Enqueue(class) => {
          let item = Item { id: next_id, class };
          next_id += 1;
          let accepted = model.enqueue(item);
          match queue.enqueue(envelope(item)) {
            | Ok(()) => assert!(accepted, "accepted beyond the model: {context}"),
            | Err(rejected) => {
              assert!(!accepted, "rejected by the queue only: {context}");
              assert_eq!(item_of(&rejected), item, "the rejected envelope comes back: {context}");
            },
          }
        },
        | Op::Dequeue => model.check_dequeued(queue.dequeue().as_ref().map(item_of), &context),
      }
      assert_eq!(queue.number_of_messages(), model.len(), "number_of_messages: {context}");
      assert_eq!(queue.has_messages(), model.len() > 0, "has_messages: {context}");
    }
    let context = format!("{name}: drain after {ops:?}");
    let drained = queue.drain();
    assert_eq!(drained.len(), model.len(), "{context}");
    for envelope in &drained {
      model.check_dequeued(Some(item_of(envelope)), &context);
    }
    assert!(!queue.has_messages(), "{context}");
  }
}

#[test]
fn unbounded_queue_is_first_in_first_out() {
  check_queue(
    "unbounded",
    7,
    || UnboundedQueue::new(&SpinLockFactory),
    || Fifo { items: VecDeque::new(), capacity: None },
  );
}

// Capacities 0 to 3, with sequences long enough to go two past each.
#[test]
fn bounded_queue_is_first_in_first_out_and_never_exceeds_its_capacity() {
  for capacity in 0..=3 {
    check_queue(
      &format!("bounded {capacity}"),
      capacity + 4,
      || BoundedQueue::new(capacity, &SpinLockFactory),
      || Fifo { items: VecDeque::new(), capacity: Some(capacity) },
    );
  }
}

#[test]
fn stable_priority_queue_keeps_the_order_within_a_priority() {
  check_queue(
    "stable priority",
    7,
    || PriorityQueue::new(priority_of, true, &SpinLockFactory),
    || Priority { items: Vec::new(), stable: true },
  );
}

#[test]
fn priority_queue_hands_out_the_highest_priority_first() {
  check_queue(
    "priority",
    7,
    || PriorityQueue::new(priority_of, false, &SpinLockFactory),
    || Priority { items: Vec::new(), stable: false },
  );
}

#[test]
fn control_aware_queue_hands_out_control_messages_first() {
  check_queue(
    "control-aware",
    7,
    || ControlAwareQueue::new(|message| class_of(message) == 0, &SpinLockFactory),
    || ControlAware { control: VecDeque::new(), normal: VecDeque::new() },
  );
}
