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

//! A read-only view of a state machine, handed to its state functions.

/// A read-only view of the machine, handed to state functions.
///
/// Mirrors what an FSM subclass reads from itself while it handles an
/// event: the number of transition listeners and the current state
/// name. The view is a snapshot taken just before the function runs;
/// subscriptions and listener stops are handled before or after a
/// state function within one delivery, never during it, so the counts
/// are exact for the delivery being handled (design of #194, H1).
pub struct FsmView<S> {
  listener_count: usize,
  state_name: S,
}

impl<S> FsmView<S> {
  /// Creates a view with `listener_count` listeners in `state_name`.
  pub(crate) fn new(listener_count: usize, state_name: S) -> Self {
    Self { listener_count, state_name }
  }

  /// Returns how many listeners the machine currently reports to.
  ///
  /// Mirrors `listeners.size` read inside an FSM subclass.
  pub fn listener_count(&self) -> usize {
    self.listener_count
  }

  /// Returns the name of the state the machine is handling an event in.
  ///
  /// Mirrors `stateName`.
  pub fn state_name(&self) -> &S {
    &self.state_name
  }
}
