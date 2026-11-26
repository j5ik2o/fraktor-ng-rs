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

//! The acknowledgement a request actor sends when it has nothing to return.

/// The acknowledgement payload of a successful [`StatusReply::ack`](super::StatusReply::ack).
///
/// Corresponds to Pekko's `Done`. This is the acknowledgement value, distinct
/// from the [`StatusReply`](super::StatusReply) envelope that carries success
/// or failure. `StatusReply::ack` wraps this value as its success payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusReplyAck;
