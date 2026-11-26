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

//! Test support for ported Pekko actor tests.
//!
//! This crate mirrors the parts of `org.apache.pekko.testkit` that the ported
//! tests need. G0b seeds it with the test kit settings, the `DefaultTimeout`
//! trait, the `ImplicitSender` sender, and the `FraktorSpec` base that owns an
//! actor system and its test actor.

pub mod coroner;
pub mod default_timeout;
pub mod event_filter;
pub mod fraktorspec;
pub mod implicit_sender;
pub mod manual_scheduler;
pub mod test_actors;
pub mod test_fsm_ref;
pub mod test_kit;
pub mod test_kit_extension;
