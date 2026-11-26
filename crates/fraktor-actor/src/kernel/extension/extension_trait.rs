// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2016-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The marker of an extension value.

use core::any::Any;

/// A value an actor system creates once and shares through its registry.
///
/// Mirrors `org.apache.pekko.actor.Extension`. The trait is a marker: the
/// registry only needs the value to be a concrete, shareable type.
pub trait Extension: Any + Send + Sync {}
