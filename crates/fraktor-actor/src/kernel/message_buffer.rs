// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2017-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! FIFO buffers for messages that cannot be delivered yet, and a map of
//! them by id.
//!
//! Mirrors `org.apache.pekko.util.MessageBuffer` and
//! `org.apache.pekko.util.MessageBufferMap`.

mod message_buffer_map_value;
mod message_buffer_value;

pub use message_buffer_map_value::MessageBufferMap;
pub use message_buffer_value::MessageBuffer;
