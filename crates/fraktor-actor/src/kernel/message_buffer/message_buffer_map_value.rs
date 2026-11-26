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

//! A map of message buffers by id.

use alloc::collections::BTreeMap;
use core::borrow::Borrow;

use super::MessageBuffer;

/// A map from ids to [`MessageBuffer`]s.
///
/// Mirrors `org.apache.pekko.util.MessageBufferMap` (marked `@InternalApi`
/// in Pekko). The original is generic over the id only and buffers
/// `(Any, ActorRef)` pairs; the port carries the message and sender types
/// through. Ids iterate in sorted order, where the original's hash map order
/// is unspecified. The original's `drop` method, which reports a buffer's
/// contents to dead letters, is not ported: it needs the actor system's dead
/// letters, which this kernel type does not have.
#[derive(Debug, Clone)]
pub struct MessageBufferMap<I, M, S> {
  buffers: BTreeMap<I, MessageBuffer<M, S>>,
}

impl<I, M, S> Default for MessageBufferMap<I, M, S> {
  fn default() -> Self {
    Self { buffers: BTreeMap::new() }
  }
}

impl<I: Ord, M, S> MessageBufferMap<I, M, S> {
  /// Creates an empty map.
  #[must_use]
  pub fn new() -> Self {
    Self { buffers: BTreeMap::new() }
  }

  /// Returns whether the map holds no buffer.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.buffers.is_empty()
  }

  /// Returns the number of buffers in the map.
  #[must_use]
  pub fn size(&self) -> usize {
    self.buffers.len()
  }

  /// Returns the total number of entries across every buffer.
  #[must_use]
  pub fn total_size(&self) -> usize {
    self.buffers.values().map(MessageBuffer::size).sum()
  }

  /// Creates an empty buffer for `id` when the map has none.
  pub fn add(&mut self, id: I) {
    self.buffers.entry(id).or_default();
  }

  /// Appends `(message, sender)` to the buffer for `id`, creating the
  /// buffer when the map has none.
  pub fn append(&mut self, id: I, message: M, sender: S) {
    self.buffers.entry(id).or_default().append(message, sender);
  }

  /// Removes the buffer for `id`, if any.
  pub fn remove<Q>(&mut self, id: &Q)
  where
    I: Borrow<Q>,
    Q: Ord + ?Sized,
  {
    self.buffers.remove(id);
  }

  /// Returns whether the map holds a buffer for `id`.
  #[must_use]
  pub fn contains<Q>(&self, id: &Q) -> bool
  where
    I: Borrow<Q>,
    Q: Ord + ?Sized,
  {
    self.buffers.contains_key(id)
  }

  /// Returns the buffer for `id`, or `None` when the map has none.
  ///
  /// The original's `getOrEmpty` returns a shared empty buffer instead; the
  /// port returns an `Option`.
  #[must_use]
  pub fn get<Q>(&self, id: &Q) -> Option<&MessageBuffer<M, S>>
  where
    I: Borrow<Q>,
    Q: Ord + ?Sized,
  {
    self.buffers.get(id)
  }

  /// Calls `f` once for every entry of every buffer, in id order, and in
  /// append order within a buffer.
  pub fn for_each<F>(&self, mut f: F)
  where
    F: FnMut(&I, &M, &S),
  {
    for (id, buffer) in &self.buffers {
      buffer.for_each(|message, sender| f(id, message, sender));
    }
  }
}
