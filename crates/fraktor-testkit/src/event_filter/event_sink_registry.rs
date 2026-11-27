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

//! The sink that a spec attaches to its system, and the filters a test arms.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::vec::Vec;

use fraktor_actor::kernel::actor::Logging::EventSink;
use fraktor_actor::kernel::actor::Logging::LogEvent;

use crate::event_filter::EventFilter;

/// Publishes events to the armed filters that a spec has registered.
///
/// Mirrors the part of `org.apache.pekko.testkit.EventFilter` that mutes the
/// events an intercept registered: Pekko installs a filter on the event stream
/// and counts the events that match it. The port keeps the same registry, keyed
/// by nothing but "the filters armed on this spec", because a spec owns one
/// system and the ported tests arm one filter at a time.
///
/// A spec hands an `Arc` of this to its system; a test arms a filter on it for
/// the duration of `intercept` and reads the count afterwards.
pub struct EventSinkRegistry {
  armed: Mutex<Vec<ArmedFilter>>,
  next_id: Mutex<usize>,
}

/// One filter that an `intercept` armed, with the events it matched.
struct ArmedFilter {
  id: usize,
  filter: EventFilter,
  matched: usize,
}

impl EventSinkRegistry {
  /// Creates an empty registry.
  pub fn new() -> Self {
    Self { armed: Mutex::new(Vec::new()), next_id: Mutex::new(0) }
  }

  /// Arms `filter` and returns its id.
  ///
  /// The id is drawn from a counter that only grows, so arming and
  /// disarming other filters never shifts the id of one that is still
  /// armed, as a position in the list would. The caller keeps the id,
  /// runs the test body, and reads the count with
  /// [`EventSinkRegistry::matched`].
  pub fn arm(&self, filter: EventFilter) -> usize {
    let id = {
      let mut next = self.next_id.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
      let id = *next;
      *next += 1;
      id
    };
    self.lock().push(ArmedFilter { id, filter, matched: 0 });
    id
  }

  /// Removes the filter armed with `id`.
  pub fn disarm(&self, id: usize) {
    self.lock().retain(|armed| armed.id != id);
  }

  /// Returns how many events the filter armed with `id` has matched.
  pub fn matched(&self, id: usize) -> usize {
    self.lock().iter().find(|armed| armed.id == id).map(|armed| armed.matched).unwrap_or(0)
  }

  /// Returns how many events each filter armed with `ids` has matched.
  ///
  /// Every count is read under one lock, so a publish cannot land
  /// between two reads and hand back counts that never stood at the
  /// same time.
  pub(crate) fn matched_all(&self, ids: &[usize]) -> Vec<usize> {
    let armed = self.lock();
    ids.iter().map(|id| armed.iter().find(|entry| entry.id == *id).map(|entry| entry.matched).unwrap_or(0)).collect()
  }

  fn lock(&self) -> MutexGuard<'_, Vec<ArmedFilter>> {
    self.armed.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }
}

impl EventSink for EventSinkRegistry {
  fn publish(&self, event: &dyn LogEvent) {
    let mut armed = self.lock();
    for entry in armed.iter_mut() {
      if entry.filter.apply(event) {
        entry.matched += 1;
      }
    }
  }

  fn into_any(self: Arc<Self>) -> Arc<dyn core::any::Any + Send + Sync> {
    self
  }
}

impl Default for EventSinkRegistry {
  fn default() -> Self {
    Self::new()
  }
}
