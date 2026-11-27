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

//! The state a test and the actor it wraps hold between them.

use alloc::boxed::Box;
use core::any::Any;

use spin::Mutex;

use crate::kernel::actor::test_support::ActorControl;
use crate::kernel::actor::ReceiveTimeoutSetting;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::dispatch::DispatcherKind;
use crate::kernel::sync::Arc;

struct ActorHandleState {
  has_started: bool,
  terminated: bool,
  generation: u64,
  next_sequence: u64,
  processed_sequence: u64,
  snapshot: Option<Box<dyn Any + Send + Sync>>,
  receive_timeout: ReceiveTimeoutSetting,
  dispatcher_kind: Option<DispatcherKind>,
  dead_letters: Option<ActorRef>,
}

/// The handle a test shares with one actor it started through a
/// `TestActorRef`. The executor writes it; the test reads it.
///
/// One handle serves one actor for its whole life: when the actor
/// restarts, the fresh instance writes into the same handle, so the
/// probe numbers and the generation counter run on across the restart
/// while the snapshot is taken again by the next probe.
#[derive(Clone)]
pub struct ActorHandle {
  shared: Arc<Mutex<ActorHandleState>>,
}

impl Default for ActorHandle {
  fn default() -> Self {
    Self::new()
  }
}

impl ActorHandle {
  /// Creates an empty handle: the actor has not started, no probe has
  /// been processed, and no snapshot has been taken.
  pub fn new() -> Self {
    Self {
      shared: Arc::new(Mutex::new(ActorHandleState {
        has_started: false,
        terminated: false,
        generation: 0,
        next_sequence: 0,
        processed_sequence: 0,
        snapshot: None,
        receive_timeout: ReceiveTimeoutSetting::Undefined,
        dispatcher_kind: None,
        dead_letters: None,
      })),
    }
  }

  /// Returns whether the first instance has run `pre_start`.
  pub fn has_started(&self) -> bool {
    self.shared.lock().has_started
  }

  /// Returns whether the actor has stopped for good. The executor sets
  /// this before any watcher is told, so a read that races the stop
  /// answers at once instead of waiting for a probe that will never be
  /// processed.
  pub fn is_terminated(&self) -> bool {
    self.shared.lock().terminated
  }

  /// Returns the generation of the instance; it rises at every restart.
  pub fn generation(&self) -> u64 {
    self.shared.lock().generation
  }

  /// Returns the highest probe number the actor has processed.
  pub fn processed_sequence(&self) -> u64 {
    self.shared.lock().processed_sequence
  }

  /// Sends the next probe to `target` and returns its number. The probe
  /// names the dead letters handle as its sender, as the port's
  /// convention for a message with no real sender; the probe's
  /// processing does not read the sender.
  pub fn send_probe(&self, target: &ActorRef) -> u64 {
    let (sequence, sender) = {
      let mut shared = self.shared.lock();
      shared.next_sequence += 1;
      (shared.next_sequence, shared.dead_letters.clone().unwrap_or_else(|| target.clone()))
    };
    target.tell(ActorControl { sequence }, sender);
    sequence
  }

  /// Returns the snapshot the last processed probe took, cloned, when
  /// its type is `T`. The snapshot stays in the handle, so a test may
  /// read it more than once.
  pub fn read_snapshot<T: Any + Clone>(&self) -> Option<T> {
    let shared = self.shared.lock();
    shared.snapshot.as_ref().and_then(|snapshot| snapshot.downcast_ref::<T>()).cloned()
  }

  /// Returns the receive timeout the actor had when the last probe was
  /// processed; `Undefined` before the first probe.
  pub fn receive_timeout(&self) -> ReceiveTimeoutSetting {
    self.shared.lock().receive_timeout
  }

  /// Returns the kind the executor resolved for the actor's dispatcher
  /// at creation; `None` when only a `DispatchSettings` defines the
  /// name.
  pub fn dispatcher_kind(&self) -> Option<DispatcherKind> {
    self.shared.lock().dispatcher_kind
  }

  /// Records that an instance of the actor has finished its
  /// `pre_start`. The mark stays across restarts.
  pub(crate) fn mark_started(&self) {
    self.shared.lock().has_started = true;
  }

  /// Records that the actor has stopped for good.
  pub(crate) fn mark_terminated(&self) {
    self.shared.lock().terminated = true;
  }

  /// Records one restart of the actor: the generation rises by one.
  pub(crate) fn note_restart(&self) {
    self.shared.lock().generation += 1;
  }

  /// Records the kind the executor resolved for the actor's
  /// dispatcher at creation.
  pub(crate) fn set_dispatcher_kind(&self, kind: Option<DispatcherKind>) {
    self.shared.lock().dispatcher_kind = kind;
  }

  /// Records the dead letters handle the probes name as their sender.
  pub(crate) fn set_dead_letters(&self, dead_letters: ActorRef) {
    self.shared.lock().dead_letters = Some(dead_letters);
  }

  /// Records that the actor processed the probe of `sequence`: the
  /// snapshot the probe took and the receive timeout of that moment
  /// replace the ones the handle held.
  pub(crate) fn note_probe(
    &self,
    sequence: u64,
    snapshot: Option<Box<dyn Any + Send + Sync>>,
    receive_timeout: ReceiveTimeoutSetting,
  ) {
    let mut shared = self.shared.lock();
    if sequence > shared.processed_sequence {
      shared.processed_sequence = sequence;
    }
    shared.snapshot = snapshot;
    shared.receive_timeout = receive_timeout;
  }

  /// Returns whether a snapshot stands in the handle, of whatever type
  /// the actor published.
  pub fn has_snapshot(&self) -> bool {
    self.shared.lock().snapshot.is_some()
  }
}
