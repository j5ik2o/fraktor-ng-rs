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

//! The first report a new transition listener receives.

use crate::kernel::actor_ref::ActorRef;

/// The first report a new listener receives: the state the machine is in.
///
/// Mirrors `FSM.CurrentState`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentState<S> {
  /// The machine that sent the report.
  pub fsm: ActorRef,
  /// The state the machine was in when it was subscribed.
  pub state: S,
}
