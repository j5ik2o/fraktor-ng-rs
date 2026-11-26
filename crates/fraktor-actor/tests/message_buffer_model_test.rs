// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2017-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Model-based property tests for MessageBuffer and MessageBufferMap.
//
// Mirrors `org.apache.pekko.util.MessageBuffer` and
// `org.apache.pekko.util.MessageBufferMap` (the models state the originals'
// observable behaviour).
//
// The models are a VecDeque<(i32, i32)> and a BTreeMap<i32, VecDeque<(i32,
// i32)>>. A fixed-seed SplitMix64 drives 400 random operation sequences of
// 120 operations each; after every operation the port and the model are
// compared through the public API (size, is_empty, head, for_each order, and
// for the map also total_size and per-id contents).

use std::collections::{BTreeMap, VecDeque};

use fraktor_actor::kernel::message_buffer::{MessageBuffer, MessageBufferMap};

struct SplitMix64(u64);

impl SplitMix64 {
  fn next_u64(&mut self) -> u64 {
    self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.0;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }

  fn below(&mut self, bound: u64) -> u64 {
    self.next_u64() % bound
  }
}

fn buffer_contents(buffer: &MessageBuffer<i32, i32>) -> Vec<(i32, i32)> {
  let mut out = Vec::new();
  buffer.for_each(|message, sender| out.push((*message, *sender)));
  out
}

#[test]
fn message_buffer_matches_the_model_over_random_operation_sequences() {
  for seed in 0..400u64 {
    let mut buffer: MessageBuffer<i32, i32> = MessageBuffer::empty();
    let mut model: VecDeque<(i32, i32)> = VecDeque::new();
    let mut random = SplitMix64(0x1873_0000 + seed);
    for _ in 0..120 {
      match random.below(4) {
        | 0 | 1 => {
          let entry = (random.below(6) as i32, random.below(6) as i32);
          buffer.append(entry.0, entry.1);
          model.push_back(entry);
        },
        | 2 => {
          buffer.drop_head();
          model.pop_front();
        },
        | _ => {
          let banned = random.below(6) as i32;
          buffer.filter_not(|message, _| *message == banned);
          model.retain(|(message, _)| *message != banned);
        },
      }
      assert_eq!(buffer.size(), model.len());
      assert_eq!(buffer.is_empty(), model.is_empty());
      assert_eq!(buffer.head().map(|(message, sender)| (*message, *sender)), model.front().copied());
      assert_eq!(buffer_contents(&buffer), model.iter().copied().collect::<Vec<_>>());
    }
  }
}

#[test]
fn message_buffer_map_matches_the_model_over_random_operation_sequences() {
  for seed in 0..400u64 {
    let mut map: MessageBufferMap<i32, i32, i32> = MessageBufferMap::new();
    let mut model: BTreeMap<i32, VecDeque<(i32, i32)>> = BTreeMap::new();
    let mut random = SplitMix64(0x1874_0000 + seed);
    for _ in 0..120 {
      let id = random.below(3) as i32;
      match random.below(5) {
        | 0 => {
          map.add(id);
          model.entry(id).or_default();
        },
        | 1 | 2 => {
          let entry = (random.below(6) as i32, random.below(6) as i32);
          map.append(id, entry.0, entry.1);
          model.entry(id).or_default().push_back(entry);
        },
        | 3 => {
          map.remove(&id);
          model.remove(&id);
        },
        | _ => {
          let mut seen: Vec<(i32, i32, i32)> = Vec::new();
          map.for_each(|buffer_id, message, sender| seen.push((*buffer_id, *message, *sender)));
          let mut expected: Vec<(i32, i32, i32)> = Vec::new();
          for (buffer_id, queue) in &model {
            for (message, sender) in queue {
              expected.push((*buffer_id, *message, *sender));
            }
          }
          assert_eq!(seen, expected);
        },
      }
      assert_eq!(map.is_empty(), model.values().all(|queue| queue.is_empty()) && model.is_empty());
      assert_eq!(map.size(), model.len());
      assert_eq!(map.total_size(), model.values().map(|queue| queue.len()).sum::<usize>());
      for probe in 0..3 {
        assert_eq!(map.contains(&probe), model.contains_key(&probe));
        assert_eq!(
          map.get(&probe).map(buffer_contents),
          model.get(&probe).map(|queue| queue.iter().copied().collect::<Vec<_>>())
        );
      }
    }
  }
}
