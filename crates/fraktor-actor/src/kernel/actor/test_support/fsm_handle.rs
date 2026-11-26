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

//! The state a test and the state machine it drives hold between them.

use alloc::string::String;
use alloc::vec::Vec;

use spin::Mutex;

use crate::kernel::actor::test_support::FsmControl;
use crate::kernel::actor::test_support::FsmStateTimerRecord;
use crate::kernel::actor::test_support::FsmTimerRecord;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::sync::Arc;

/// How a snapshot copies the machine's data, set up by the machine when
/// it attaches: the snapshot is the port's replacement for the reference
/// Pekko's `stateData` hands out, so it holds a copy, and the copy is
/// made through this when the machine writes its state.
pub type FsmDataCloner<D> = Arc<dyn Fn(&D) -> D + Send + Sync>;

struct FsmShared<S, D> {
  snapshot: Option<(S, D)>,
  data_cloner: Option<FsmDataCloner<D>>,
  timers: Vec<(String, FsmTimerRecord)>,
  state_timer: Option<FsmStateTimerRecord>,
  state_timer_active: bool,
  processed_sequence: u64,
  next_sequence: u64,
  generation: u64,
  actor_ref: Option<ActorRef>,
  started: bool,
}

/// The handle a test and a state machine hold between them.
///
/// The machine writes its current state name and data here when it is
/// attached and after every move it makes, mirrors the timers it holds,
/// and reports the control messages it has processed; the test reads the
/// same values back, which is the grip `TestFSMRef` has on the machine
/// in place of the actor itself. One handle serves one machine for its
/// whole life: when the machine restarts, the fresh instance attaches to
/// the same handle again, so the sequence numbers and the generation
/// counter run on across the restart while the snapshot and the timer
/// records start over.
///
/// Mirrors the state `org.apache.pekko.testkit.TestFSMRef` reads from the
/// actor it wraps.
pub struct FsmHandle<S, D> {
  shared: Arc<Mutex<FsmShared<S, D>>>,
}

impl<S, D> Clone for FsmHandle<S, D> {
  fn clone(&self) -> Self {
    Self { shared: self.shared.clone() }
  }
}

impl<S, D> Default for FsmHandle<S, D> {
  fn default() -> Self {
    Self::new()
  }
}

impl<S, D> FsmHandle<S, D> {
  /// Creates an empty handle: no snapshot, no timers, no actor yet.
  pub fn new() -> Self {
    Self {
      shared: Arc::new(Mutex::new(FsmShared {
        snapshot: None,
        data_cloner: None,
        timers: Vec::new(),
        state_timer: None,
        state_timer_active: false,
        processed_sequence: 0,
        next_sequence: 0,
        generation: 0,
        actor_ref: None,
        started: false,
      })),
    }
  }

  /// Returns the state name of the snapshot, or `None` before the
  /// machine has written one.
  pub fn state_name(&self) -> Option<S>
  where
    S: Clone,
  {
    self.shared.lock().snapshot.as_ref().map(|(state, _)| state.clone())
  }

  /// Returns a copy of the state data of the snapshot, or `None` before
  /// the machine has written one.
  pub fn state_data(&self) -> Option<D>
  where
    D: Clone,
  {
    self.shared.lock().snapshot.as_ref().map(|(_, data)| data.clone())
  }

  /// Returns whether a timer of `name` stands in the machine's table.
  pub fn is_timer_active(&self, name: &str) -> bool {
    self.shared.lock().timers.iter().any(|(used, _)| used == name)
  }

  /// Returns whether the state timer reads as active. As in Pekko, the
  /// state timer still reads as active after it has fired, until the
  /// next ordinary message the machine receives cancels it.
  pub fn is_state_timer_active(&self) -> bool {
    self.shared.lock().state_timer_active
  }

  /// Returns the timer records the firing side works from.
  pub fn timer_records(&self) -> Vec<(String, FsmTimerRecord)> {
    self.shared.lock().timers.clone()
  }

  /// Returns the record of the armed state timer, if one is armed.
  pub fn state_timer_record(&self) -> Option<FsmStateTimerRecord> {
    self.shared.lock().state_timer
  }

  /// Returns the highest sequence number among the control messages the
  /// machine has processed.
  pub fn processed_sequence(&self) -> u64 {
    self.shared.lock().processed_sequence
  }

  /// Hands out the next sequence number for a control message.
  pub fn allocate_sequence(&self) -> u64 {
    let mut shared = self.shared.lock();
    shared.next_sequence += 1;
    shared.next_sequence
  }

  /// Sends the numbered control message that does nothing and returns
  /// its number. Once [`processed_sequence`](Self::processed_sequence)
  /// reaches the returned number, the machine has processed everything
  /// sent before the sentinel as well, because its mailbox is in order.
  pub fn send_sentinel(&self) -> u64
  where
    S: Send + Sync + 'static,
    D: Send + 'static,
  {
    let sequence = self.allocate_sequence();
    if let Some(actor) = self.actor_ref() {
      actor.tell(FsmControl::<S, D>::Sentinel { sequence }, actor.clone());
    }
    sequence
  }

  /// Records that an instance of the machine has finished its
  /// `pre_start`. The mark stays across restarts.
  pub fn mark_started(&self) {
    self.shared.lock().started = true;
  }

  /// Returns whether an instance of the machine has finished its
  /// `pre_start`. A machine starts in its own run, after the request that
  /// created it has returned (design of #234, section 11.12.10), so
  /// `TestFsmRef` runs the dispatcher until this holds or the machine has
  /// stopped.
  pub fn has_started(&self) -> bool {
    self.shared.lock().started
  }

  /// Records the reference of the machine this handle is attached to,
  /// which the sentinel is sent to.
  pub fn set_actor_ref(&self, actor_ref: ActorRef) {
    self.shared.lock().actor_ref = Some(actor_ref);
  }

  /// Returns the reference of the machine this handle is attached to.
  pub fn actor_ref(&self) -> Option<ActorRef> {
    self.shared.lock().actor_ref.clone()
  }

  /// Hands out the next generation. One counter serves the named timers
  /// and the machine's own generation, so a generation is never reused:
  /// a mark struck for an older timer or an older arming of the state
  /// timer can never match again, across restarts included.
  pub fn next_generation(&self) -> u64 {
    let mut shared = self.shared.lock();
    shared.generation += 1;
    shared.generation
  }

  /// Returns the current generation.
  pub fn generation(&self) -> u64 {
    self.shared.lock().generation
  }

  /// Starts the handle over for a new instance of the machine: the
  /// snapshot becomes the instance's starting state, the timer records
  /// and the state timer are emptied, and the sequence numbers and the
  /// generation counter run on. The machine calls this when it attaches,
  /// with the way to copy its data.
  pub fn install(&self, snapshot: Option<(&S, &D)>, data_cloner: FsmDataCloner<D>)
  where
    S: Clone,
  {
    let mut shared = self.shared.lock();
    shared.snapshot = snapshot.map(|(state, data)| (state.clone(), data_cloner(data)));
    shared.data_cloner = Some(data_cloner);
    shared.timers.clear();
    shared.state_timer = None;
    shared.state_timer_active = false;
  }

  /// Writes the machine's current state into the snapshot.
  pub fn write_snapshot(&self, state: &S, data: &D)
  where
    S: Clone,
  {
    let mut shared = self.shared.lock();
    if let Some(cloner) = &shared.data_cloner {
      let data = cloner(data);
      shared.snapshot = Some((state.clone(), data));
    }
  }

  /// Empties the snapshot and the timer records, as a stop does. The
  /// sequence numbers and the generation counter run on.
  pub fn clear(&self) {
    let mut shared = self.shared.lock();
    shared.snapshot = None;
    shared.timers.clear();
    shared.state_timer = None;
    shared.state_timer_active = false;
  }

  /// Records a named timer the machine started, replacing the record of
  /// the same name.
  pub fn set_timer_record(&self, name: &str, record: FsmTimerRecord) {
    let mut shared = self.shared.lock();
    shared.timers.retain(|(used, _)| used != name);
    shared.timers.push((name.into(), record));
  }

  /// Drops the record of a named timer.
  pub fn remove_timer_record(&self, name: &str) {
    self.shared.lock().timers.retain(|(used, _)| used != name);
  }

  /// Records the state timer the machine armed, or clears the record,
  /// and sets whether the state timer reads as active.
  pub fn set_state_timer(&self, record: Option<FsmStateTimerRecord>, active: bool) {
    let mut shared = self.shared.lock();
    shared.state_timer = record;
    shared.state_timer_active = active;
  }

  /// Records that the machine processed the control message of
  /// `sequence`, keeping the highest number seen.
  pub fn note_processed(&self, sequence: u64) {
    let mut shared = self.shared.lock();
    if sequence > shared.processed_sequence {
      shared.processed_sequence = sequence;
    }
  }
}
