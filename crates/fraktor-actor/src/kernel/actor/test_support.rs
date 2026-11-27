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

//! The face a state machine shows to a test that drives it from outside.
//!
//! This module exists only under the `test-support` feature. A machine a
//! test starts carries a shared handle ([`FsmHandle`]) that the test reads
//! its state through and sends control messages ([`FsmControl`]) to, which
//! is how the port gives `TestFSMRef` its synchronous grip on the machine
//! without handing out the actor itself. Without the feature, none of this
//! surface exists and a machine costs exactly what it cost before.

mod actor_control;
mod actor_handle;
mod fsm_control;
mod fsm_handle;
mod fsm_state_timer_record;
mod fsm_timer_kind;
mod fsm_timer_record;

pub(crate) use actor_control::ActorControl;
pub use actor_handle::ActorHandle;
pub use fsm_control::FsmControl;
pub use fsm_handle::FsmDataCloner;
pub use fsm_handle::FsmHandle;
pub use fsm_state_timer_record::FsmStateTimerRecord;
pub use fsm_timer_kind::FsmTimerKind;
pub use fsm_timer_record::FsmTimerRecord;
