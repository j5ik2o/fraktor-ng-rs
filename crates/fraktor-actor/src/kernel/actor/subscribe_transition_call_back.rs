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

//! The message that asks a state machine to report its transitions.

use crate::kernel::actor_ref::ActorRef;

/// The message that asks the machine to report its transitions to a listener.
///
/// Mirrors `FSM.SubscribeTransitionCallBack`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubscribeTransitionCallBack(pub ActorRef);
