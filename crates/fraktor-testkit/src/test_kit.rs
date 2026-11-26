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

//! The test kit that runs beside an actor system.

mod auto_pilot;
mod auto_pilot_control;
pub mod expected_messages;
mod receive_while;
mod received_message;
mod test_actor;
mod test_kit_base;
mod test_kit_value;
mod test_latch;
mod test_probe;

pub use auto_pilot::AutoPilot;
pub use auto_pilot_control::AutoPilotControl;
pub use receive_while::ReceiveWhile;
pub use received_message::ReceivedMessage;
pub(crate) use test_actor::TestActor;
pub(crate) use test_kit_base::TestKitBase;
pub use test_kit_value::TestKit;
pub use test_latch::TestLatch;
pub use test_probe::TestProbe;
