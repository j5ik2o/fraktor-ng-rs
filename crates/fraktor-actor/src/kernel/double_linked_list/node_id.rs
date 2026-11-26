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

//! The id of a node in a [`DoubleLinkedList`](super::DoubleLinkedList).

/// Names one node of a [`DoubleLinkedList`](super::DoubleLinkedList).
///
/// Pekko's `DoubleLinkedList[Node]` links the caller's own node objects, so the
/// node itself is what its operations take and return. The port owns its
/// values, so inserting a value returns a `NodeId` for it, and the operations
/// that take or return a node in Pekko take or return its id. Two ids are equal
/// exactly when they name the same node, which is how a caller checks that an
/// operation returned an existing node. An id carries the number of the list
/// that made it, so an id from one list never names a node of another list, and
/// a list never reuses the id of a removed node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId {
  list: u64,
  node: u64,
}

impl NodeId {
  pub(crate) const fn new(list: u64, node: u64) -> Self {
    Self { list, node }
  }
}
